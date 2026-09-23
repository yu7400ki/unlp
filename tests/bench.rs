use std::fs;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use serde_json::Value;
use tempfile::TempDir;

/// 欄を持たない設定のファイル。同梱した既定だけで採点させるために渡す。
const EMPTY_CONFIG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/empty.toml");

/// 人間側の文。下限を超える長さで指摘を 1 件も持たない。
const HUMAN: &str = "較正のコーパスは集合ごとにディレクトリを分けて配置し、一覧のファイルから参照する構成を採用している。集合の側は人間と Claude の二種類で、判定に用いる基準はそれぞれ異なる値を設定する。

コミットメッセージの集合は、保存したログのファイルを解析して取得する方式を採用しており、採点の実行時に git を起動しない構成になっている。ログのファイルはハッシュと本文を区切り文字で並べた形式を保持する。

パスの集合は、ディレクトリを再帰的に走査して対象のファイルを収集する。抽出の書式は拡張子から決定し、対応しない種類のファイルは処理の対象から除外する。

集合の文字数が下限を下回る場合は、正規化した点を算出せずに判定の対象から除外する。

規則ごとの件数は、1000 字あたりの値に換算して表に並べる。層ごとの小計も同じ表に掲載する。
";

/// Claude 側の文。下限を超える長さで構造・密度・定型句・語種の指摘を持つ。
const CLAUDE: &str = "この設定は、利用者に対して重要な点を示しています。まず、型が状態を語ることが挙げられます。設計は、複数の観点から整合性を担保することが求められます。つまり、単に動作するだけでなく、保守性も考慮する必要があります。

**結論として、この方針が最適です。**

この仕組みは、単なる機能追加ではなく、全体の整合性を高めるものです。ちなみに、既存の実装との互換性も保たれています。したがって、移行の手順は最小限で済みます。

この点は、設計の観点から見ても妥当だと考えられます。なお、細部については検討の余地が残されています。

ご不明な点がありましたら、ご指示ください。

一方で、この実装は、性能の観点からは追加の検証が必要になる可能性があります。ちなみに、計測の環境は既存の構成を流用しています。
";

/// 下限を下回る長さの文。
const SHORT: &str = "抽出は拡張子で書式を決める。\n";

/// コミットの題名と本文。
const SUBJECTS: [&str; 2] = ["一件目の題名を短く書く。", "二件目の題名を短く書く。"];
const BODY: &str = "本文は変更の理由を述べる。";

/// `git log --format=%H%x00%B%x00` の出力。井桁で始まる行とトレーラーを含む。
fn log() -> String {
    let messages = [
        format!(
            "{}\n\n{BODY}\n\n# 井桁の行は対象にしない\nReviewed-By: 担当者 <t@example.com>\n",
            SUBJECTS[0]
        ),
        format!("{}\n", SUBJECTS[1]),
    ];
    messages
        .iter()
        .enumerate()
        .map(|(at, message)| format!("{}\0{message}\0", (at + 1).to_string().repeat(40)))
        .collect()
}

/// 人間側と Claude 側を 1 つずつ持つ一覧。
const BOTH_SIDES: &str = "[[set]]
name = \"zenn\"
side = \"human\"
paths = [\"human/prose\"]

[[set]]
name = \"sonnet-prose\"
side = \"claude\"
paths = [\"claude/prose\"]
";

/// コーパスと一覧を置いた一時ディレクトリ。
fn corpus(manifest: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in [
        ("human/prose/a.md", HUMAN.to_string()),
        ("human/commits/commits.log", log()),
        ("claude/prose/a.md", CLAUDE.to_string()),
        ("claude/short/a.md", SHORT.to_string()),
        ("claude/empty/a.md", "No Japanese here.\n".to_string()),
    ] {
        let path = dir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fs::write(dir.path().join("manifest.toml"), manifest).unwrap();
    dir
}

/// 一時ディレクトリの一覧を既定の設定で較正するコマンド。
fn bench(dir: &TempDir) -> Command {
    bench_with(dir, EMPTY_CONFIG)
}

/// 一覧と設定のファイルを指定して較正するコマンド。
fn bench_with(dir: &TempDir, config: &str) -> Command {
    let mut command = Command::cargo_bin("unlp").unwrap();
    command
        .arg("bench")
        .arg(dir.path().join("manifest.toml"))
        .arg("--config")
        .arg(config);
    command
}

fn json(command: &mut Command) -> Value {
    let output = command.arg("--json").output().unwrap().stdout;
    serde_json::from_slice(&output).unwrap()
}

/// 期待する日本語の文字数。
fn ja_chars<'a>(texts: impl IntoIterator<Item = &'a str>) -> usize {
    texts
        .into_iter()
        .map(|text| text.chars().filter(|c| unlp::is_japanese(*c)).count())
        .sum()
}

#[test]
fn both_sides_within_the_criteria_meet_them() {
    bench(&corpus(BOTH_SIDES))
        .assert()
        .success()
        .stdout(contains("zenn").and(contains("human")))
        .stdout(contains("sonnet-prose").and(contains("claude")))
        .stdout(contains("S01").and(contains("G01")))
        .stdout(contains("受け入れ基準を満たす"));
}

#[test]
fn a_claude_set_inside_the_human_range_fails_the_bench() {
    let dir = corpus(
        "[[set]]\nname = \"zenn\"\nside = \"human\"\npaths = [\"human/prose\"]\n\n\
         [[set]]\nname = \"opus-prose\"\nside = \"claude\"\npaths = [\"human/prose\"]\n",
    );
    bench(&dir)
        .assert()
        .code(1)
        .stdout(contains("受け入れ基準を満たさない"))
        .stdout(contains("opus-prose  claude  0.00 点"));
}

#[test]
fn an_empty_set_is_named_in_the_judgment() {
    let dir = corpus(
        "[[set]]\nname = \"zenn\"\nside = \"human\"\npaths = [\"human/prose\"]\n\n\
         [[set]]\nname = \"empty-set\"\nside = \"claude\"\npaths = [\"claude/empty\"]\n",
    );
    bench(&dir)
        .assert()
        .success()
        .stdout(contains("集合が空").and(contains("empty-set")))
        .stdout(contains("受け入れ基準を満たす"));

    let report = json(&mut bench(&dir));
    assert_eq!(report["sets"][1]["verdict"], "empty");
    assert_eq!(report["sets"][1]["total"]["ja_chars"], 0);
}

#[test]
fn a_set_below_the_floor_stays_out_of_the_judgment() {
    let dir = corpus(
        "[[set]]\nname = \"zenn\"\nside = \"human\"\npaths = [\"human/prose\"]\n\n\
         [[set]]\nname = \"short\"\nside = \"claude\"\npaths = [\"claude/short\"]\n",
    );
    bench(&dir)
        .assert()
        .success()
        .stdout(contains("下限未満"))
        .stdout(contains("受け入れ基準を満たす"));

    let report = json(&mut bench(&dir));
    assert_eq!(report["sets"][1]["verdict"], "below_floor");
    assert_eq!(report["sets"][1]["total"]["mode"]["kind"], "count_only");
    assert_eq!(report["met"], true);
}

#[test]
fn the_json_holds_the_point_the_rates_and_the_verdict_of_every_set() {
    let report = json(&mut bench(&corpus(BOTH_SIDES)));
    assert_eq!(report["met"], true);

    let human = &report["sets"][0];
    assert_eq!(human["name"], "zenn");
    assert_eq!(human["side"], "human");
    assert_eq!(human["verdict"], "met");
    assert_eq!(human["total"]["mode"]["per_1000"], 0.0);

    let claude = &report["sets"][1];
    assert_eq!(claude["side"], "claude");
    assert_eq!(claude["verdict"], "met");
    assert!(
        claude["total"]["mode"]["per_1000"].as_f64().unwrap() >= 6.0,
        "{claude}"
    );
    assert!(
        claude["by_rule_per_1000"]["S01"].as_f64().unwrap() > 0.0,
        "{claude}"
    );

    let rates = claude["by_rule_per_1000"].as_object().unwrap();
    let rules = Command::cargo_bin("unlp")
        .unwrap()
        .arg("rules")
        .output()
        .unwrap();
    let registered = String::from_utf8(rules.stdout).unwrap().lines().count();
    assert_eq!(rates.len(), registered);
    assert!(rates.values().all(|rate| rate.is_f64()), "{claude}");
}

#[test]
fn a_commits_log_is_read_without_running_git() {
    let dir = corpus(
        "[[set]]\nname = \"commits\"\nside = \"human\"\ncommits = \"human/commits/commits.log\"\n",
    );
    let report = json(&mut bench(&dir));
    let total = &report["sets"][0]["total"];
    assert_eq!(
        total["ja_chars"],
        ja_chars([SUBJECTS[0], BODY, SUBJECTS[1]]),
        "井桁の行とトレーラーは数えない: {total}"
    );
    assert_eq!(total["sentences"], 3);
}

#[test]
fn the_bench_takes_neither_the_summary_nor_the_fail_over() {
    let dir = corpus(BOTH_SIDES);
    for option in [vec!["--summary"], vec!["--fail-over", "10"]] {
        bench(&dir)
            .args(&option)
            .assert()
            .code(2)
            .stderr(contains("--summary").and(contains("--fail-over")));
    }
}

#[test]
fn the_floor_of_the_config_reaches_the_bench() {
    let dir = corpus(BOTH_SIDES);
    let config = dir.path().join("unlp.toml");
    fs::write(&config, "floor = 1000\n").unwrap();
    bench_with(&dir, config.to_str().unwrap())
        .assert()
        .success()
        .stdout(contains("下限未満"));
}
