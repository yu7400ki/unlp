use std::fs;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use serde_json::Value;

/// 欄を持たない設定のファイル。同梱した既定だけで採点させるために渡す。
const EMPTY_CONFIG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/empty.toml");

/// 同梱した既定の設定で実行するコマンド。
fn unlp() -> Command {
    let mut command = repo_unlp();
    command.arg("--config").arg(EMPTY_CONFIG);
    command
}

/// このリポジトリの設定を読んで実行するコマンド。
fn repo_unlp() -> Command {
    Command::cargo_bin("unlp").unwrap()
}

fn json(command: &mut Command) -> Value {
    let output = command.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).unwrap()
}

fn check_markdown(text: &str) -> Value {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), text).unwrap();
    json(unlp().args(["check", "--json"]).arg(dir.path()))
}

fn check_source(name: &str, text: &str) -> Value {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(name), text).unwrap();
    json(unlp().args(["check", "--json"]).arg(dir.path()))
}

/// 期待する Segment の日本語の文字数の合計。
fn ja_chars<'a>(segments: impl IntoIterator<Item = &'a str>) -> usize {
    segments
        .into_iter()
        .map(|text| text.chars().filter(|c| unlp::is_japanese(*c)).count())
        .sum()
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
    let text = format!(
        "{}型の doc が名乗る。",
        "この文はここでは十分に長く書いてある一文だ。".repeat(14)
    );
    let report = json(unlp().args(["stdin", "--json"]).write_stdin(text.as_str()));
    assert_eq!(report["total"]["ja_chars"], 300);
    assert_eq!(report["total"]["mode"]["per_1000"], 10.0);
    assert_eq!(report["total"]["mode"]["by_layer"]["structure"], 10.0);

    unlp()
        .args(["stdin", "--summary"])
        .write_stdin(text)
        .assert()
        .success()
        .stdout(contains("全体  300 字  15 文  正規化 10.0 点（構造 10.0）"));
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
    let report = json(repo_unlp().args(["check", "skills/", "--json"]));
    let by_rule = report["total"]["by_rule"].as_object().unwrap();
    assert!(by_rule.is_empty(), "{by_rule:?}");
}

#[test]
fn several_rules_count_their_findings_on_one_input() {
    let report = json(unlp().args(["stdin", "--json"]).write_stdin(
        "これは意図的です。つまり、私が設計しました。**意図したものです。**エラーを出さずに失敗し始めます。ご指示ください。",
    ));
    let by_rule = report["documents"][0]["score"]["by_rule"]
        .as_object()
        .unwrap();
    let rules: Vec<&str> = by_rule.keys().map(String::as_str).collect();
    assert_eq!(rules, ["F01", "F02", "F03", "L03", "S02", "S03", "S04"]);
    assert_eq!(report["total"]["by_rule"]["F03"], 1);
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
    let text = "この文はここでは十分に長く書いてある文だ。".repeat(15);
    let report = json(unlp().args(["stdin", "--json"]).write_stdin(text));
    assert_eq!(report["total"]["ja_chars"], 300);
    assert_eq!(report["total"]["sentences"], 15);
    assert_eq!(report["total"]["mode"]["kind"], "normalized");
    assert_eq!(report["total"]["mode"]["per_1000"], 0.0);
}

#[test]
fn fail_over_passes_when_the_point_is_within_the_threshold() {
    let report = json(
        unlp()
            .args(["stdin", "--json", "--fail-over=0"])
            .write_stdin("この文はここでは十分に長く書いてある文だ。".repeat(15)),
    );
    assert_eq!(report["total"]["mode"]["kind"], "normalized");
    assert_eq!(report["total"]["mode"]["per_1000"], 0.0);
}

#[test]
fn fail_over_stops_when_the_point_is_over_the_threshold() {
    unlp()
        .args(["stdin", "--fail-over=-1"])
        .write_stdin("この文はここでは十分に長く書いてある文だ。".repeat(15))
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

#[test]
fn the_contents_of_code_are_not_counted() {
    let report = check_markdown(concat!(
        "段落だ。\n\n",
        "```\nコードの中の文だ。\n```\n\n",
        "    字下げのコードだ。\n\n",
        "前に `コードスパンの文。` と続く。\n",
    ));
    assert_eq!(report["total"]["ja_chars"], 8);
    assert_eq!(report["total"]["sentences"], 2);
}

#[test]
fn a_numbered_item_is_scored_without_its_marker() {
    let report = check_markdown("1. **結論です。**\n2. 次の項目だ。\n");
    assert_eq!(report["documents"][0]["score"]["by_rule"]["F02"], 1);
    let findings = report["documents"][0]["score"]["findings"]
        .as_array()
        .unwrap();
    assert!(
        findings
            .iter()
            .any(|finding| finding["rule"] == "F02" && finding["excerpt"] == "**結論です。**"),
        "{findings:?}"
    );
}

#[test]
fn the_bold_of_a_definition_item_and_of_a_code_span_is_not_counted() {
    let report = check_markdown(concat!(
        "- **ブランチの作成**: 作業ごとに切る。\n",
        "- オプションは **`--force`** を渡す。\n\n",
        "**注意**: 値を変える。\n\n",
        "これは **重要な** 点だ。\n\n",
        "**結論です。**理由を書く。\n",
    ));
    assert_eq!(report["total"]["by_rule"]["F03"], 2);
    assert_eq!(report["total"]["by_rule"]["F02"], 1);
    let bold: Vec<&str> = report["documents"][0]["score"]["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|finding| finding["rule"] == "F03")
        .map(|finding| finding["excerpt"].as_str().unwrap())
        .collect();
    assert_eq!(bold, ["**重要な**", "**結論です。**"]);
}

#[test]
fn a_heading_is_one_sentence_without_its_marker() {
    let report = check_markdown("## これは重要だ\n\n本文だ。\n");
    assert_eq!(report["total"]["ja_chars"], 9);
    assert_eq!(report["total"]["sentences"], 2);
    assert_eq!(report["documents"][0]["score"]["by_rule"]["S02"], 1);
}

#[test]
fn a_table_cell_without_a_predicate_is_not_counted() {
    let report = check_markdown(concat!(
        "| 語 | 説明 |\n|---|---|\n",
        "| 文字コード | 述語を含む文だ。 |\n",
        "| 名詞の列挙。 | 語だけ |\n",
    ));
    assert_eq!(report["total"]["ja_chars"], 7);
    assert_eq!(report["total"]["sentences"], 1);
    let by_rule = report["total"]["by_rule"].as_object().unwrap();
    assert!(by_rule.is_empty(), "{by_rule:?}");
}

#[test]
fn stdin_reads_markdown_with_the_format_flag() {
    let markdown = "# 見出しだ\n\n```\nコードの中の文だ。\n```\n\n本文だ。\n";
    let report = json(
        unlp()
            .args(["stdin", "--json", "--format", "markdown"])
            .write_stdin(markdown),
    );
    assert_eq!(report["total"]["ja_chars"], 7);
    assert_eq!(report["total"]["sentences"], 2);

    let report = json(unlp().args(["stdin", "--json"]).write_stdin(markdown));
    assert_eq!(report["total"]["ja_chars"], 15);
}

#[test]
fn a_file_with_japanese_only_in_code_is_not_a_document() {
    let report = check_markdown("# Title\n\n```\nコードの文だ。\n```\n");
    assert_eq!(report["documents"].as_array().unwrap().len(), 0);
    assert_eq!(report["total"]["ja_chars"], 0);
}

#[test]
fn a_translated_paragraph_counts_the_literal_translations() {
    let report = json(unlp().args(["stdin", "--json"]).write_stdin(concat!(
        "既定では、キャッシュはメモリ上に保存されます。",
        "各ワーカーがそれぞれ独自のファイルを書き込むため、8つのワーカーを持つマシンでは8つのコピーができます。",
        "ディレクトリが一杯になると、書き込みは静かに失敗し、リクエストは上流 API へのフォールバックとなります。",
    )));
    assert_eq!(report["total"]["by_rule"]["L02"], 4);
}

#[test]
fn the_measures_of_a_document_are_numbers() {
    let report = json(
        unlp()
            .args(["stdin", "--json"])
            .write_stdin("設定を比べると動作が変わります。値を追加する。窓が開く。"),
    );
    let measures = &report["documents"][0]["score"]["measures"];
    for field in [
        "polite_ratio",
        "plain_ratio",
        "wago_noun_ratio",
        "wago_verb_ratio",
        "final_wago_ratio",
        "ga_per_sentence",
    ] {
        assert!(measures[field].is_number(), "{field}  {measures}");
    }
}

#[test]
fn a_run_of_conjunctions_at_the_head_of_the_sentences_is_a_finding() {
    let report = json(
        unlp()
            .args(["stdin", "--json"])
            .write_stdin("さらに規則を数える。また、規則を並べる。したがって規則が残る。"),
    );
    assert_eq!(report["total"]["by_rule"]["S06"], 1);
    let findings = report["documents"][0]["score"]["findings"]
        .as_array()
        .unwrap();
    assert!(
        findings
            .iter()
            .any(|finding| finding["rule"] == "S06"
                && finding["excerpt"] == "さらに、また、したがって"),
        "{findings:?}"
    );
}

#[test]
fn a_run_of_short_sentences_is_a_finding() {
    let text = format!(
        "{}窓が開く。鍵が回る。値が減る。",
        "この文は十五字を超える長さで書いてある。".repeat(16)
    );
    let report = json(unlp().args(["stdin", "--json"]).write_stdin(text));
    assert_eq!(report["total"]["mode"]["kind"], "normalized");
    assert_eq!(report["total"]["by_rule"]["D01"], 1);
    let findings = report["documents"][0]["score"]["findings"]
        .as_array()
        .unwrap();
    assert!(
        findings
            .iter()
            .any(|finding| finding["rule"] == "D01" && finding["excerpt"] == "窓が開く。"),
        "{findings:?}"
    );
}

#[test]
fn a_document_of_one_register_is_not_a_mixture() {
    let report = json(
        unlp()
            .args(["stdin", "--json"])
            .write_stdin("この文はここでは十分長く書いてあるのだ。".repeat(16)),
    );
    assert_eq!(report["total"]["mode"]["kind"], "normalized");
    assert_eq!(
        report["documents"][0]["score"]["measures"]["polite_ratio"],
        0.0
    );
    assert!(report["total"]["by_rule"]["R03"].is_null(), "{report}");
}

#[test]
fn a_document_of_both_registers_is_a_mixture() {
    let text = format!(
        "{}{}",
        "この文はここでは十分長く書いてあります。".repeat(8),
        "この文はここでは十分長く書いてあるのだ。".repeat(8)
    );
    let report = json(unlp().args(["stdin", "--json"]).write_stdin(text));
    assert_eq!(report["total"]["by_rule"]["R03"], 1);
    let findings = report["documents"][0]["score"]["findings"]
        .as_array()
        .unwrap();
    assert!(
        findings
            .iter()
            .any(|finding| finding["rule"] == "R03" && finding["excerpt"] == "敬体 0.50 常体 0.50"),
        "{findings:?}"
    );
}

#[test]
fn a_document_of_wago_predicates_is_a_finding() {
    let text =
        "設定を足すと読みが溜まる。窓を開けると値が下がる。本を読むと字が増える。".repeat(10);
    let report = json(unlp().args(["stdin", "--json"]).write_stdin(text));
    assert_eq!(report["total"]["mode"]["kind"], "normalized");
    assert!(
        report["documents"][0]["score"]["measures"]["final_wago_ratio"]
            .as_f64()
            .unwrap()
            > 0.7
    );
    assert_eq!(report["total"]["by_rule"]["G01"], 1);
    let findings = report["documents"][0]["score"]["findings"]
        .as_array()
        .unwrap();
    assert!(
        findings.iter().any(|finding| finding["rule"] == "G01"
            && finding["excerpt"]
                .as_str()
                .unwrap()
                .starts_with("下がる 10、増える 10、溜まる 10")),
        "{findings:?}"
    );
}

#[test]
fn a_polite_document_counts_the_colloquialisms_and_the_hedges() {
    let report = json(unlp().args(["stdin", "--json"]).write_stdin(
        "ちょっと直します。やつを消してしまいました。壊れるかもしれません。直るはずです。",
    ));
    let by_rule = &report["total"]["by_rule"];
    assert_eq!(by_rule["R01"], 3, "{by_rule}");
    assert_eq!(by_rule["D03"], 2, "{by_rule}");
}

#[test]
fn a_plain_document_leaves_the_colloquialisms_and_the_hedges_alone() {
    let report = json(
        unlp()
            .args(["stdin", "--json"])
            .write_stdin("ちょっと直す。やつを消してしまった。壊れるかもしれない。直るはずだ。"),
    );
    let by_rule = &report["total"]["by_rule"];
    assert_eq!(
        report["documents"][0]["score"]["measures"]["polite_ratio"],
        0.0
    );
    assert!(by_rule["R01"].is_null(), "{by_rule}");
    assert!(by_rule["D03"].is_null(), "{by_rule}");
}

#[test]
fn the_rule_book_stays_within_the_wago_ratio() {
    let report = json(repo_unlp().args(["check", "skills/", "--json"]));
    let documents = report["documents"].as_array().unwrap();
    assert_eq!(documents.len(), 2, "{documents:?}");
    for document in documents {
        let score = &document["score"];
        assert_eq!(score["mode"]["kind"], "normalized", "{score}");
        assert!(
            score["measures"]["final_wago_ratio"].as_f64().unwrap() < 0.7,
            "{score}"
        );
    }
}

#[test]
fn rust_comments_and_strings_are_scored_without_the_code() {
    let source = concat!(
        "//! モジュールの説明だ。\n",
        "\n",
        "/// 文書を読み込んで\n",
        "/// 一つの文にする。\n",
        "// 行のコメントだ。\n",
        "/* ブロックのコメントだ。 */\n",
        "fn read(名前: &str) -> String {\n",
        "    let message = \"エラー: 見つからない。\";\n",
        "    let raw = r#\"生の文字列だ。\"#;\n",
        "    format!(\"{message}{raw}\")\n",
        "}\n",
    );
    let report = check_source("a.rs", source);
    let score = &report["documents"][0]["score"];
    assert_eq!(
        score["ja_chars"],
        ja_chars([
            "モジュールの説明だ。",
            "文書を読み込んで一つの文にする。",
            "行のコメントだ。ブロックのコメントだ。",
            "エラー: 見つからない。",
            "生の文字列だ。",
        ]),
        "{score}"
    );
    assert_eq!(score["sentences"], 6, "{score}");
}

#[test]
fn a_placeholder_of_a_string_is_blanked() {
    let report = check_source("a.rs", "fn f() { format!(\"{名前} を読み込めない\"); }\n");
    assert_eq!(
        report["documents"][0]["score"]["ja_chars"],
        ja_chars(["を読み込めない"])
    );
}

#[test]
fn python_comments_docstrings_and_strings_are_scored() {
    let source = concat!(
        "\"\"\"モジュールの説明だ。\"\"\"\n",
        "\n",
        "# 行のコメントだ。\n",
        "def read(名前):\n",
        "    \"\"\"文書を読み込む。\"\"\"\n",
        "    message = f\"{名前} を読み込めない\"\n",
        "    return message\n",
    );
    let report = check_source("a.py", source);
    let score = &report["documents"][0]["score"];
    assert_eq!(
        score["ja_chars"],
        ja_chars([
            "モジュールの説明だ。",
            "行のコメントだ。",
            "文書を読み込む。",
            "を読み込めない",
        ]),
        "{score}"
    );
    assert_eq!(score["sentences"], 4, "{score}");
}

#[test]
fn a_file_out_of_the_formats_is_skipped_silently_in_a_walk() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.toml"), "key = \"設定の値だ。\"\n").unwrap();
    fs::write(dir.path().join("b.txt"), "本文だ。").unwrap();

    let output = unlp()
        .args(["check", "--json"])
        .arg(dir.path())
        .assert()
        .success()
        .get_output()
        .clone();
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["documents"].as_array().unwrap().len(), 1);
    assert!(
        report["documents"][0]["name"]
            .as_str()
            .unwrap()
            .ends_with("b.txt"),
        "{report}"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");

    let walked = unlp()
        .args(["check", "--json", "data/"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert_eq!(String::from_utf8(walked.stderr).unwrap(), "");
    let report: Value = serde_json::from_slice(&walked.stdout).unwrap();
    assert!(
        report["documents"].as_array().unwrap().is_empty(),
        "{report}"
    );
}

#[test]
fn a_named_path_out_of_the_formats_is_warned() {
    unlp()
        .args(["check", "Cargo.toml"])
        .assert()
        .success()
        .stderr(contains("警告"))
        .stderr(contains("Cargo.toml"));
}

#[test]
fn a_blank_comment_line_breaks_the_run_of_short_sentences() {
    let filler = "/// この文はここでは十分に長く書いてある一文だ。\n".repeat(15);
    let source = format!(
        "{filler}mod a {{}}\n\n/// 窓が開く。鍵が回る。\n///\n/// 値が減る。\nmod b {{}}\n"
    );
    let report = check_source("a.rs", &source);
    let score = &report["documents"][0]["score"];
    assert_eq!(score["mode"]["kind"], "normalized", "{score}");
    assert!(score["by_rule"]["D01"].is_null(), "{score}");
}
