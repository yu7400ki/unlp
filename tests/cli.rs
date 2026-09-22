use std::fs;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use serde_json::Value;

fn unlp() -> Command {
    Command::cargo_bin("unlp").unwrap()
}

fn json(command: &mut Command) -> Value {
    let output = command.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).unwrap()
}

#[test]
fn stdin_becomes_one_document_scored_by_ja_chars() {
    let report = json(
        unlp()
            .args(["stdin", "--json"])
            .write_stdin("型の doc が名乗る。設定を比べると動作が変わる。"),
    );
    assert_eq!(report["documents"][0]["name"], "<stdin>");
    assert_eq!(report["documents"][0]["score"]["ja_chars"], 19);
    assert_eq!(report["documents"][0]["score"]["sentences"], 2);
    assert_eq!(report["total"]["ja_chars"], 19);
    assert_eq!(report["total"]["sentences"], 2);
    assert_eq!(report["total"]["mode"]["kind"], "count_only");
}

#[test]
fn text_output_lists_each_document_and_the_total() {
    unlp()
        .arg("stdin")
        .write_stdin("日本語だ。")
        .assert()
        .success()
        .stdout(contains("<stdin>"))
        .stdout(contains("4 字"))
        .stdout(contains("1 文"))
        .stdout(contains("全体"));
}

#[test]
fn a_finding_carries_its_rule_origin_excerpt_and_hint() {
    let report = json(
        unlp()
            .args(["stdin", "--json"])
            .write_stdin("型の doc が名乗る。利用者が述べる。"),
    );
    let findings = report["documents"][0]["score"]["findings"]
        .as_array()
        .unwrap();
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0]["rule"], "S01");
    assert_eq!(findings[0]["layer"], "structure");
    assert_eq!(findings[0]["excerpt"], "doc が名乗る");
    assert_eq!(findings[0]["origin"]["path"], "<stdin>");
    assert!(findings[0]["hint"].is_string(), "{findings:?}");
    assert_eq!(report["documents"][0]["score"]["by_rule"]["S01"], 1);
}

#[test]
fn the_text_output_lists_the_findings() {
    unlp()
        .arg("stdin")
        .write_stdin("型の doc が名乗る。")
        .assert()
        .success()
        .stdout(contains(
            "<stdin>:1  S01  doc が名乗る  文書や型を語り手にしない",
        ));
}

#[test]
fn the_normalized_point_weighs_the_findings() {
    let text = format!("{}型の doc が名乗る。", "日本語の文だ。".repeat(49));
    let report = json(unlp().args(["stdin", "--json"]).write_stdin(text.as_str()));
    assert_eq!(report["total"]["ja_chars"], 300);
    assert_eq!(report["total"]["mode"]["per_1000"], 10.0);
    assert_eq!(report["total"]["mode"]["by_layer"]["structure"], 10.0);

    unlp()
        .args(["stdin", "--summary"])
        .write_stdin(text)
        .assert()
        .success()
        .stdout(contains("全体  300 字  50 文  正規化 10.0 点（構造 10.0）"));
}

#[test]
fn rules_lists_the_rule_with_its_layer_weight_and_heading() {
    unlp()
        .arg("rules")
        .assert()
        .success()
        .stdout(contains("S01  構造  3.00  文書・型・検査を語り手にしない"));
}

#[test]
fn the_rule_book_has_no_findings() {
    let report = json(unlp().args(["check", "skills/", "--json"]));
    let by_rule = report["total"]["by_rule"].as_object().unwrap();
    assert!(by_rule.is_empty(), "{by_rule:?}");
}

#[test]
fn check_walks_directories_and_skips_excluded_ones() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "含まれる文だ。").unwrap();
    fs::create_dir(dir.path().join("target")).unwrap();
    fs::write(dir.path().join("target/b.txt"), "除かれる文だ。").unwrap();

    let report = json(unlp().args(["check", "--json"]).arg(dir.path()));
    assert_eq!(report["documents"].as_array().unwrap().len(), 1);
    assert_eq!(report["total"]["ja_chars"], 6);
}

#[test]
fn a_file_without_japanese_is_not_a_document() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "no japanese here\n").unwrap();
    fs::write(dir.path().join("b.txt"), "日本語だ。").unwrap();

    let report = json(unlp().args(["check", "--json"]).arg(dir.path()));
    assert_eq!(report["documents"].as_array().unwrap().len(), 1);
    assert_eq!(report["documents"][0]["score"]["ja_chars"], 4);
}

#[test]
fn the_floor_switches_the_total_to_a_normalized_point() {
    let text = "日本語の文だ。".repeat(50);
    let report = json(unlp().args(["stdin", "--json"]).write_stdin(text));
    assert_eq!(report["total"]["ja_chars"], 300);
    assert_eq!(report["total"]["sentences"], 50);
    assert_eq!(report["total"]["mode"]["kind"], "normalized");
    assert_eq!(report["total"]["mode"]["per_1000"], 0.0);
}

#[test]
fn fail_over_passes_when_the_point_is_within_the_threshold() {
    let report = json(
        unlp()
            .args(["stdin", "--json", "--fail-over=0"])
            .write_stdin("日本語の文だ。".repeat(50)),
    );
    assert_eq!(report["total"]["mode"]["kind"], "normalized");
    assert_eq!(report["total"]["mode"]["per_1000"], 0.0);
}

#[test]
fn fail_over_stops_when_the_point_is_over_the_threshold() {
    unlp()
        .args(["stdin", "--fail-over=-1"])
        .write_stdin("日本語の文だ。".repeat(50))
        .assert()
        .code(1);
}

#[test]
fn summary_keeps_the_totals_without_the_findings() {
    unlp()
        .args(["stdin", "--summary"])
        .write_stdin("日本語だ。")
        .assert()
        .success()
        .stdout(contains("全体"));

    let report = json(
        unlp()
            .args(["stdin", "--json", "--summary"])
            .write_stdin("日本語だ。"),
    );
    assert_eq!(report["documents"][0]["score"]["ja_chars"], 4);
    assert!(report["documents"][0]["score"]["by_rule"].is_object());
    assert!(report["documents"][0]["score"]["findings"].is_null());

    let report = json(unlp().args(["stdin", "--json"]).write_stdin("日本語だ。"));
    assert!(report["documents"][0]["score"]["findings"].is_array());
}

#[test]
fn stdin_without_japanese_is_not_a_document() {
    let report = json(
        unlp()
            .args(["stdin", "--json"])
            .write_stdin("no japanese\n"),
    );
    assert_eq!(report["documents"].as_array().unwrap().len(), 0);
    assert_eq!(report["total"]["ja_chars"], 0);
}

#[test]
fn a_missing_path_is_an_input_error() {
    unlp()
        .args(["check", "no-such-path"])
        .assert()
        .code(2)
        .stdout(contains("全体").not());
}

#[test]
fn check_requires_a_path() {
    unlp().arg("check").assert().code(2);
}
