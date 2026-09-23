use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use serde_json::Value;
use tempfile::TempDir;

/// 一時ディレクトリに作る git のリポジトリ。利用者の設定を読み込ませない環境で git を実行する。
struct Repo {
    dir: TempDir,
}

impl Repo {
    fn new() -> Self {
        let repo = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        repo.git(&["init", "-q", "--initial-branch=main"]);
        repo.git(&["config", "user.name", "t"]);
        repo.git(&["config", "user.email", "t@example.com"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo.git(&["config", "core.autocrlf", "false"]);
        repo
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    /// 利用者の全体・システムの設定を読まない環境変数。
    fn env(&self) -> [(&'static str, PathBuf); 2] {
        [
            ("GIT_CONFIG_GLOBAL", self.path().join("no-gitconfig")),
            ("GIT_CONFIG_NOSYSTEM", PathBuf::from("1")),
        ]
    }

    fn git(&self, args: &[&str]) -> String {
        let output = process::Command::new("git")
            .current_dir(self.path())
            .envs(self.env())
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn write(&self, name: &str, text: &str) {
        let path = self.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
    }

    fn commit(&self, message: &str) {
        let file = self.path().join(".git").join("MSG");
        fs::write(&file, message).unwrap();
        self.git(&["commit", "-q", "-F", file.to_str().unwrap()]);
    }

    fn unlp(&self) -> Command {
        let mut command = Command::cargo_bin("unlp").unwrap();
        command.current_dir(self.path());
        for (name, value) in self.env() {
            command.env(name, value);
        }
        command
    }
}

fn json(command: &mut Command) -> Value {
    let output = command.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).unwrap()
}

/// 期待する Segment の日本語の文字数の合計。
fn ja_chars<'a>(segments: impl IntoIterator<Item = &'a str>) -> u64 {
    segments
        .into_iter()
        .map(|text| text.chars().filter(|c| unlp::is_japanese(*c)).count() as u64)
        .sum()
}

fn names(report: &Value) -> Vec<String> {
    report["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|document| document["name"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn commits_score_the_messages_without_the_trailers() {
    let repo = Repo::new();
    repo.write("a.txt", "初めの文だ。\n");
    repo.git(&["add", "."]);
    repo.commit("chore: init\n");
    repo.write("b.txt", "次の文だ。\n");
    repo.git(&["add", "."]);
    repo.commit("feat: 機能を追加する\n");
    repo.write("c.txt", "また次の文だ。\n");
    repo.git(&["add", "."]);
    repo.commit("fix: 誤りを直す\n\n本文の文だ。\n\nCo-Authored-By: 手伝い <a@example.com>\n");
    repo.write("d.txt", "最後の文だ。\n");
    repo.git(&["add", "."]);
    repo.commit("chore: bump the deps\n");

    let report = json(repo.unlp().args(["commits", "-n", "3", "--json"]));
    assert_eq!(report["documents"].as_array().unwrap().len(), 2, "{report}");
    assert_eq!(
        report["total"]["ja_chars"],
        ja_chars(["誤りを直す", "本文の文だ。", "機能を追加する"]),
        "{report}"
    );

    let short = repo.git(&["rev-parse", "--short=7", "HEAD~1"]);
    let names = names(&report);
    assert!(names[0].starts_with(short.trim()), "{names:?}");
    assert!(names[0].contains("誤りを直す"), "{names:?}");
    assert!(names[1].contains("機能を追加する"), "{names:?}");

    repo.unlp()
        .args(["commits", "-n", "3"])
        .assert()
        .success()
        .stdout(contains("手伝い").not());
}

#[test]
fn commits_take_a_range_and_default_to_the_last_one() {
    let repo = Repo::new();
    repo.write("a.txt", "初めの文だ。\n");
    repo.git(&["add", "."]);
    repo.commit("chore: init\n");
    repo.write("b.txt", "次の文だ。\n");
    repo.git(&["add", "."]);
    repo.commit("feat: 機能を追加する\n");
    repo.write("c.txt", "また次の文だ。\n");
    repo.git(&["add", "."]);
    repo.commit("chore: bump the deps\n");

    let report = json(repo.unlp().args(["commits", "HEAD~2..HEAD~1", "--json"]));
    assert_eq!(names(&report).len(), 1, "{report}");
    assert!(names(&report)[0].contains("機能を追加する"), "{report}");

    let report = json(repo.unlp().args(["commits", "--json"]));
    assert_eq!(report["documents"].as_array().unwrap().len(), 0, "{report}");
}

#[test]
fn commits_beyond_the_first_one_are_an_error_of_git() {
    let repo = Repo::new();
    repo.write("a.txt", "初めの文だ。\n");
    repo.git(&["add", "."]);
    repo.commit("chore: init\n");

    repo.unlp()
        .args(["commits", "-n", "99"])
        .assert()
        .code(2)
        .stderr(contains("HEAD~99..HEAD"));
}
