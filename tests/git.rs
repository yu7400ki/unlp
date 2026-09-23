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

    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.path().join(name)).unwrap()
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

/// コミットのハッシュを除いた結果。面が違っても一致するかを比べるために使う。
fn without_commits(mut value: Value) -> Value {
    match &mut value {
        Value::Object(map) => {
            map.remove("commit");
            for field in map.values_mut() {
                *field = without_commits(field.take());
            }
        }
        Value::Array(items) => {
            for item in items {
                *item = without_commits(item.take());
            }
        }
        _ => {}
    }
    value
}

const LIB_BEFORE: &str = concat!(
    "// 触らない行のコメントだ。\n",
    "fn kept() {}\n",
    "\n",
    "// 複数行にわたる説明の一行目だ。\n",
    "// 二行目は直す対象になる。\n",
    "fn changed() {}\n",
);

const LIB_AFTER: &str = concat!(
    "// 触らない行のコメントだ。\n",
    "fn kept() {}\n",
    "\n",
    "// 複数行にわたる説明の一行目だ。\n",
    "// 二行目を直した。\n",
    "fn changed() {}\n",
    "\n",
    "// 追加した行のコメントだ。\n",
    "fn added() {}\n",
);

const DOC_BEFORE: &str = concat!(
    "# 見出しだ\n",
    "\n",
    "触らない段落だ。\n",
    "\n",
    "複数行の段落の一行目だ。\n",
    "二行目は直す対象になる。\n",
);

const DOC_AFTER: &str = concat!(
    "# 見出しだ\n",
    "\n",
    "触らない段落だ。\n",
    "\n",
    "複数行の段落の一行目だ。\n",
    "二行目を直した。\n",
    "\n",
    "追加した段落だ。\n",
);

/// 名前に空白を含むファイル。git は差分の見出しの行末に TAB を付ける。
const SPACED_PATH: &str = "docs/read me.md";

const SPACED_BEFORE: &str = "空白を含む名前の段落だ。\n";

const SPACED_AFTER: &str = "空白を含む名前の段落だ。\n\n足した段落だ。\n";

/// 差分に残る Segment。触った行に重なるノードだけが対象になる。
const TOUCHED: [&str; 5] = [
    "複数行の段落の一行目だ。二行目を直した。",
    "追加した段落だ。",
    "足した段落だ。",
    "複数行にわたる説明の一行目だ。二行目を直した。",
    "追加した行のコメントだ。",
];

/// 初期のコミットを持ち、変更を索引に載せたリポジトリ。
fn repo_with_staged_change() -> Repo {
    let repo = Repo::new();
    repo.write("src/lib.rs", LIB_BEFORE);
    repo.write("doc.md", DOC_BEFORE);
    repo.write(SPACED_PATH, SPACED_BEFORE);
    repo.write("config.toml", "値 = \"設定の文だ。\"\n");
    repo.git(&["add", "."]);
    repo.commit("chore: init\n");

    repo.write("src/lib.rs", LIB_AFTER);
    repo.write("doc.md", DOC_AFTER);
    repo.write(SPACED_PATH, SPACED_AFTER);
    repo.write("config.toml", "値 = \"直した設定の文だ。\"\n");
    repo.git(&["add", "."]);
    repo
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

#[test]
fn the_staged_diff_keeps_the_nodes_the_added_lines_touch() {
    let repo = repo_with_staged_change();
    let report = json(repo.unlp().args(["diff", "--staged", "--json"]));
    assert_eq!(
        names(&report),
        ["doc.md", SPACED_PATH, "src/lib.rs"],
        "{report}"
    );
    assert_eq!(report["total"]["ja_chars"], ja_chars(TOUCHED), "{report}");
}

#[test]
fn a_range_of_commits_yields_the_diff_of_the_range() {
    let repo = repo_with_staged_change();
    let staged = json(repo.unlp().args(["diff", "--staged", "--json"]));
    repo.commit("chore: 変更を記録する\n");
    let range = json(repo.unlp().args(["diff", "HEAD~1..HEAD", "--json"]));
    assert_eq!(
        without_commits(range.clone()),
        without_commits(staged),
        "{range}"
    );
}

#[test]
fn the_commit_msg_hook_stops_a_message_with_a_finding() {
    let repo = repo_with_staged_change();
    repo.write(".git/MESSAGE", "型の doc が名乗る。\n");

    repo.unlp()
        .args(["hook", "commit-msg", ".git/MESSAGE"])
        .assert()
        .code(1)
        .stdout(contains("S01"));

    repo.write(".git/MESSAGE", "chore: 変更を記録する\n");
    repo.unlp()
        .args(["hook", "commit-msg", ".git/MESSAGE"])
        .assert()
        .success();
}

#[test]
fn installing_the_hook_twice_leaves_the_same_file() {
    let repo = Repo::new();
    repo.unlp().args(["hook", "install"]).assert().success();
    let first = repo.read(".git/hooks/commit-msg");
    repo.unlp().args(["hook", "install"]).assert().success();
    assert_eq!(repo.read(".git/hooks/commit-msg"), first);
    assert!(first.contains("unlp hook commit-msg"), "{first}");
}

#[test]
fn installing_the_hook_leaves_another_hook_alone() {
    let repo = Repo::new();
    let other = "#!/bin/sh\necho 別の hook\n";
    repo.write(".git/hooks/commit-msg", other);

    repo.unlp().args(["hook", "install"]).assert().code(1);
    assert_eq!(repo.read(".git/hooks/commit-msg"), other);

    repo.unlp()
        .args(["hook", "install", "--force"])
        .assert()
        .success();
    assert!(
        repo.read(".git/hooks/commit-msg")
            .contains("unlp hook commit-msg"),
        "{}",
        repo.read(".git/hooks/commit-msg")
    );
}

#[test]
fn uninstalling_removes_only_the_hook_it_installed() {
    let repo = Repo::new();
    repo.unlp().args(["hook", "uninstall"]).assert().success();

    let other = "#!/bin/sh\necho 別の hook\n";
    repo.write(".git/hooks/commit-msg", other);
    repo.unlp().args(["hook", "uninstall"]).assert().success();
    assert_eq!(repo.read(".git/hooks/commit-msg"), other);

    repo.unlp()
        .args(["hook", "install", "--force"])
        .assert()
        .success();
    repo.unlp().args(["hook", "uninstall"]).assert().success();
    assert!(!repo.path().join(".git/hooks/commit-msg").exists());
}
