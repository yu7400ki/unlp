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
