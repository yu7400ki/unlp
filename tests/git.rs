use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use assert_cmd::Command;
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
    repo.commit(concat!(
        "fix: 誤りを直す\n\n本文の文だ。\n\n",
        "Co-Authored-By: 手伝い <a@example.com>\n",
        "Reviewed-by: doc が名乗る <a@example.com>\n",
    ));
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

    let trailered = &report["documents"][0]["score"];
    assert_eq!(
        trailered["ja_chars"],
        ja_chars(["誤りを直す", "本文の文だ。"]),
        "{trailered}"
    );
    assert_eq!(trailered["findings"].as_array().unwrap(), &[] as &[Value]);
    assert_eq!(report["total"]["by_rule"].as_object().unwrap().len(), 0);

    let short = repo.git(&["rev-parse", "--short=7", "HEAD~1"]);
    let names = names(&report);
    assert!(names[0].starts_with(short.trim()), "{names:?}");
    assert!(names[0].contains("誤りを直す"), "{names:?}");
    assert!(names[1].contains("機能を追加する"), "{names:?}");
}

#[test]
fn commits_take_a_range_and_default_to_the_last_one() {
    let repo = Repo::new();
    repo.write("a.txt", "初めの文だ。\n");
    repo.git(&["add", "."]);
    repo.commit("chore: 土台を置く\n");
    repo.write("b.txt", "次の文だ。\n");
    repo.git(&["add", "."]);
    repo.commit("feat: 機能を追加する\n");
    repo.write("c.txt", "また次の文だ。\n");
    repo.git(&["add", "."]);
    repo.commit("chore: bump the deps\n");

    let report = json(repo.unlp().args(["commits", "HEAD~2..HEAD~1", "--json"]));
    assert_eq!(names(&report).len(), 1, "{report}");
    assert!(names(&report)[0].contains("機能を追加する"), "{report}");

    let report = json(repo.unlp().args(["commits", "HEAD~1", "--json"]));
    assert_eq!(names(&report).len(), 1, "{report}");
    assert!(names(&report)[0].contains("機能を追加する"), "{report}");

    let report = json(repo.unlp().args(["commits", "--json"]));
    assert_eq!(report["documents"].as_array().unwrap().len(), 0, "{report}");
}

#[test]
fn the_only_commit_of_a_repository_is_scored() {
    let repo = Repo::new();
    repo.write("a.txt", "初めの文だ。\n");
    repo.git(&["add", "."]);
    repo.commit("feat: 機能を追加する\n");

    for args in [
        ["commits", "--json"].as_slice(),
        ["commits", "-n", "99", "--json"].as_slice(),
    ] {
        let report = json(repo.unlp().args(args));
        assert_eq!(names(&report).len(), 1, "{args:?}  {report}");
        assert!(names(&report)[0].contains("機能を追加する"), "{report}");
    }
}

#[test]
fn a_merge_does_not_change_the_number_of_the_last_commits() {
    let repo = Repo::new();
    repo.write("a.txt", "初めの文だ。\n");
    repo.git(&["add", "."]);
    repo.commit("chore: 土台を置く\n");
    repo.write("b.txt", "幹の文だ。\n");
    repo.git(&["add", "."]);
    repo.commit("feat: 幹を進める\n");
    repo.git(&["checkout", "-q", "-b", "topic", "HEAD~1"]);
    repo.write("c.txt", "支流の文だ。\n");
    repo.git(&["add", "."]);
    repo.commit("feat: 支流を進める\n");
    repo.git(&["checkout", "-q", "main"]);
    repo.git(&["merge", "-q", "--no-ff", "--no-commit", "topic"]);
    repo.commit("chore: 支流を合わせる\n");

    let report = json(repo.unlp().args(["commits", "-n", "2", "--json"]));
    assert_eq!(names(&report).len(), 2, "{report}");
    assert!(names(&report)[0].contains("支流を合わせる"), "{report}");
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
fn a_deleted_line_that_looks_like_a_header_keeps_the_later_hunks() {
    let repo = Repo::new();
    repo.write("a.md", "前の段落だ。\n\n-- 印だ\n\n後の段落だ。\n");
    repo.git(&["add", "."]);
    repo.commit("chore: init\n");
    repo.write("a.md", "前の段落だ。\n\n\n直した後の段落だ。\n");
    repo.git(&["add", "."]);

    let report = json(repo.unlp().args(["diff", "--staged", "--json"]));
    assert_eq!(names(&report), ["a.md"], "{report}");
    assert_eq!(
        report["total"]["ja_chars"],
        ja_chars(["直した後の段落だ。"]),
        "{report}"
    );
}

#[test]
fn a_gitlink_in_the_index_does_not_stop_the_diff() {
    let repo = repo_with_staged_change();
    repo.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        "160000,0000000000000000000000000000000000000001,sub",
    ]);

    let report = json(repo.unlp().args(["diff", "--staged", "--json"]));
    assert_eq!(
        names(&report),
        ["doc.md", SPACED_PATH, "src/lib.rs"],
        "{report}"
    );

    repo.write(".git/MESSAGE", "chore: 変更を記録する\n");
    repo.unlp()
        .args(["hook", "commit-msg", ".git/MESSAGE"])
        .assert()
        .success();
}

#[test]
fn a_renamed_file_is_scored_as_an_added_one() {
    let repo = Repo::new();
    let guide = "初めの段落だ。\n\n次の段落だ。\n\n三つ目の段落だ。\n\n四つ目の段落だ。\n";
    repo.write("doc.md", guide);
    repo.git(&["add", "."]);
    repo.commit("chore: init\n");
    repo.git(&["mv", "doc.md", "guide.md"]);
    repo.write("guide.md", &format!("{guide}\n足した段落だ。\n"));
    repo.git(&["add", "."]);
    assert!(
        repo.git(&["diff", "--cached", "--name-status"])
            .starts_with('R'),
        "git が改名を検出する差分"
    );

    let report = json(repo.unlp().args(["diff", "--staged", "--json"]));
    assert_eq!(names(&report), ["guide.md"], "{report}");
    assert_eq!(
        report["total"]["ja_chars"],
        ja_chars([
            "初めの段落だ。",
            "次の段落だ。",
            "三つ目の段落だ。",
            "四つ目の段落だ。",
            "足した段落だ。"
        ]),
        "{report}"
    );
}

#[test]
fn an_external_diff_driver_does_not_hide_the_findings() {
    let repo = Repo::new();
    repo.write("doc.md", "初めの段落だ。\n");
    repo.git(&["add", "."]);
    repo.commit("chore: init\n");
    repo.git(&["config", "diff.external", "true"]);
    repo.write("doc.md", "初めの段落だ。\n\n型の doc が名乗る。\n");
    repo.git(&["add", "."]);

    repo.unlp()
        .args(["diff", "--staged"])
        .assert()
        .success()
        .stdout(contains("S01"));
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
fn a_file_that_is_not_utf8_is_warned_and_skipped() {
    let repo = Repo::new();
    repo.write("doc.md", "初めの段落だ。\n");
    repo.git(&["add", "."]);
    repo.commit("chore: init\n");
    // Shift_JIS の「日本」は NUL を含まないので、git は本文として差分に出す。
    fs::write(repo.path().join("sjis.md"), [0x93, 0xfa, 0x96, 0x7b, 0x0a]).unwrap();
    repo.write("doc.md", "初めの段落だ。\n\n足した段落だ。\n");
    repo.git(&["add", "."]);

    let report = json(repo.unlp().args(["diff", "--staged", "--json"]));
    assert_eq!(names(&report), ["doc.md"], "{report}");
    repo.unlp()
        .args(["diff", "--staged"])
        .assert()
        .success()
        .stderr(contains("警告"))
        .stderr(contains("sjis.md"));
}

#[test]
fn a_single_rev_is_the_change_of_that_commit() {
    let repo = repo_with_staged_change();
    let staged = json(repo.unlp().args(["diff", "--staged", "--json"]));
    repo.commit("chore: 変更を記録する\n");
    repo.write(
        "doc.md",
        &format!("{DOC_AFTER}\n作業ツリーだけの段落だ。\n"),
    );

    let single = json(repo.unlp().args(["diff", "HEAD", "--json"]));
    assert_eq!(
        without_commits(single.clone()),
        without_commits(staged),
        "{single}"
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
        .stdout(contains("S01"))
        .stdout(contains("src/lib.rs"));

    repo.write(".git/MESSAGE", "chore: 変更を記録する\n");
    repo.unlp()
        .args(["hook", "commit-msg", ".git/MESSAGE"])
        .assert()
        .success()
        .stdout(contains("src/lib.rs"));
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

/// 日本語 21 字の文を並べて、コミットメッセージを 300 字以上にする。
const MESSAGE_FILLER: &str = "この文はここでは十分に長く書いてある一文だ。";

/// 310 字で S01 の指摘を 2 件持つメッセージ。既定の重みでは 1000 字あたり 19.4 点になる。
fn long_message() -> String {
    format!(
        "chore: 記録する\n{}型の doc が名乗る。型の doc が名乗る。\n",
        MESSAGE_FILLER.repeat(14)
    )
}

#[test]
fn the_threshold_in_the_config_decides_the_hook() {
    let repo = Repo::new();
    repo.write("doc.md", "初めの段落だ。\n");
    repo.git(&["add", "."]);
    repo.commit("chore: init\n");
    repo.write(".git/MESSAGE", &long_message());

    repo.unlp()
        .args(["hook", "commit-msg", ".git/MESSAGE"])
        .assert()
        .code(1)
        .stdout(contains("正規化 19.4 点"));

    repo.write("unlp.toml", "threshold = 50\n");
    repo.unlp()
        .args(["hook", "commit-msg", ".git/MESSAGE"])
        .assert()
        .success()
        .stdout(contains("正規化 19.4 点"));
}

#[test]
fn the_excluded_paths_are_not_in_the_staged_diff() {
    let repo = repo_with_staged_change();
    repo.write("unlp.toml", "exclude = [\"src/**\"]\n");

    let report = json(repo.unlp().args(["diff", "--staged", "--json"]));
    assert_eq!(names(&report), ["doc.md", SPACED_PATH], "{report}");
}

#[test]
fn the_hook_is_installed_and_removed_with_a_broken_config() {
    let repo = Repo::new();
    repo.write("unlp.toml", "threshold =\n");

    repo.unlp().args(["hook", "install"]).assert().success();
    assert!(
        repo.read(".git/hooks/commit-msg")
            .contains("unlp hook commit-msg")
    );
    repo.unlp().args(["hook", "uninstall"]).assert().success();
    assert!(!repo.path().join(".git/hooks/commit-msg").exists());
}
