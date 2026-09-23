use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::str::contains;
use serde_json::Value;
use tempfile::TempDir;

/// 作業ディレクトリを移して実行するコマンド。設定の探索の起点になる。
fn unlp(dir: &Path) -> Command {
    let mut command = Command::cargo_bin("unlp").unwrap();
    command.current_dir(dir);
    command
}

fn json(command: &mut Command) -> Value {
    let output = command.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).unwrap()
}

/// 300 字の文書にする、日本語 21 字の文。
const FILLER: &str = "この文はここでは十分に長く書いてある一文だ。";

/// 300 字で S01 の指摘を 1 件持つ文書。既定の重みでは 1000 字あたり 10.0 点になる。
fn document() -> String {
    format!("{}型の doc が名乗る。", FILLER.repeat(14))
}

/// 設定とファイルを置いた一時ディレクトリ。
fn dir_with(config: &str, files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("unlp.toml"), config).unwrap();
    for (name, text) in files {
        let path = dir.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    dir
}

#[test]
fn an_integer_threshold_and_floor_are_read() {
    let dir = dir_with(
        "threshold = 10\nfloor = 4\n",
        &[("a.txt", "型の doc が名乗る。")],
    );
    let report = json(unlp(dir.path()).args(["check", ".", "--json"]));
    assert_eq!(report["total"]["mode"]["kind"], "normalized", "{report}");
}

#[test]
fn the_floor_in_the_config_switches_the_mode() {
    let dir = dir_with("floor = 400\n", &[("a.txt", &document())]);
    let report = json(unlp(dir.path()).args(["check", ".", "--json"]));
    assert_eq!(report["total"]["ja_chars"], 300, "{report}");
    assert_eq!(report["total"]["mode"]["kind"], "count_only", "{report}");
}

#[test]
fn a_malformed_config_is_an_error_with_its_path() {
    let dir = dir_with("threshold =\n", &[("a.txt", "文だ。")]);
    unlp(dir.path())
        .args(["check", "."])
        .assert()
        .code(2)
        .stderr(contains("unlp.toml"));
}

#[test]
fn a_value_of_the_wrong_type_is_an_error_with_its_path() {
    let dir = dir_with("floor = \"300\"\n", &[("a.txt", "文だ。")]);
    unlp(dir.path())
        .args(["check", "."])
        .assert()
        .code(2)
        .stderr(contains("unlp.toml"));
}

#[test]
fn an_unknown_key_is_an_error() {
    let dir = dir_with("thresold = 10\n", &[("a.txt", "文だ。")]);
    unlp(dir.path())
        .args(["check", "."])
        .assert()
        .code(2)
        .stderr(contains("thresold"));
}

#[test]
fn the_weights_in_the_config_change_the_point() {
    let dir = dir_with("[weights]\nS01 = 6.0\n", &[("a.txt", &document())]);
    let report = json(unlp(dir.path()).args(["check", ".", "--json"]));
    assert_eq!(report["total"]["ja_chars"], 300, "{report}");
    assert_eq!(report["total"]["mode"]["per_1000"], 20.0, "{report}");
}

#[test]
fn the_weights_in_the_config_change_the_rule_list() {
    let dir = dir_with("[weights]\nS01 = 6.0\n", &[]);
    unlp(dir.path())
        .arg("rules")
        .assert()
        .success()
        .stdout(contains("S01  構造  6.00  "));
}

#[test]
fn the_config_above_the_target_applies() {
    let dir = dir_with("[weights]\nS01 = 6.0\n", &[("sub/a.txt", &document())]);
    let report = json(unlp(&dir.path().join("sub")).args(["check", "a.txt", "--json"]));
    assert_eq!(report["total"]["mode"]["per_1000"], 20.0, "{report}");
}

#[test]
fn the_config_given_on_the_command_line_wins() {
    let dir = dir_with("[weights]\nS01 = 6.0\n", &[("a.txt", &document())]);
    let other = dir_with("[weights]\nS01 = 1.5\n", &[]);
    let report = json(
        unlp(dir.path())
            .args(["check", ".", "--json", "--config"])
            .arg(other.path().join("unlp.toml")),
    );
    assert_eq!(report["total"]["mode"]["per_1000"], 5.0, "{report}");
}

#[test]
fn a_word_added_to_a_list_is_matched() {
    let dir = dir_with(
        "[lists.S01.person]\nadd = [\"型\"]\n",
        &[("a.txt", "型が述べる。")],
    );
    let report = json(unlp(dir.path()).args(["check", ".", "--json"]));
    let by_rule = report["total"]["by_rule"].as_object().unwrap();
    assert!(by_rule.is_empty(), "{report}");
}

#[test]
fn a_word_removed_from_a_list_is_not_matched() {
    let dir = dir_with(
        "[lists.S01.person]\nremove = [\"者\"]\n",
        &[("a.txt", "利用者が述べる。")],
    );
    let report = json(unlp(dir.path()).args(["check", ".", "--json"]));
    assert_eq!(report["total"]["by_rule"]["S01"], 1, "{report}");
}

#[test]
fn an_unknown_rule_id_in_the_weights_is_an_error() {
    for (config, shown) in [
        ("[weights]\nS99 = 1.0\n", "S99"),
        ("[weights]\nZZ = 1.0\n", "ZZ"),
    ] {
        let dir = dir_with(config, &[("a.txt", "文だ。")]);
        unlp(dir.path())
            .args(["check", "."])
            .assert()
            .code(2)
            .stderr(contains(shown))
            .stderr(contains("unlp.toml"));
    }
}

#[test]
fn an_unknown_rule_id_or_group_in_the_lists_is_an_error() {
    for (config, shown) in [
        ("[lists.S01.people]\nadd = [\"型\"]\n", "people"),
        ("[lists.G01.person]\nadd = [\"型\"]\n", "G01"),
    ] {
        let dir = dir_with(config, &[("a.txt", "文だ。")]);
        unlp(dir.path())
            .args(["check", "."])
            .assert()
            .code(2)
            .stderr(contains(shown))
            .stderr(contains("unlp.toml"));
    }
}
