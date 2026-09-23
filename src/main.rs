use std::collections::BTreeMap;
use std::io::{Read, stdin};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use unlp::extract::{self, STDIN_NAME};
use unlp::input;
use unlp::morph::Analyzer;
use unlp::score::{DEFAULT_FLOOR, DocumentScore, Report, Score, ScoreMode, Total};
use unlp::{Document, Finding, Layer, rule};

/// 日本語の文章に残る AI の癖を検出して採点する。
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    #[command(flatten)]
    options: Options,
}

#[derive(Subcommand)]
enum Command {
    /// ファイルとディレクトリを検査する
    Check {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// 標準入力を 1 つの文書として検査する
    Stdin {
        /// 標準入力を抽出する書式
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// 規則の一覧と規則集の見出しを出力する
    Rules,
}

/// 入力を抽出する書式。
#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Markdown,
}

impl Format {
    fn document(self, name: String, text: &str) -> Document {
        match self {
            Self::Text => extract::text_document(name, text),
            Self::Markdown => extract::markdown_document(name, text),
        }
    }
}

#[derive(Args)]
struct Options {
    /// JSON で出力する
    #[arg(long, global = true)]
    json: bool,
    /// 指摘の一覧を省いて集計だけを出力する
    #[arg(long, global = true)]
    summary: bool,
    /// 点がしきい値を超えたら終了コード 1 を返す
    #[arg(long, value_name = "POINT", global = true)]
    fail_over: Option<f64>,
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::from(1),
        Ok(false) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("誤り: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool> {
    let cli = Cli::parse();
    let documents = match &cli.command {
        Command::Rules => {
            print_rules();
            return Ok(false);
        }
        Command::Check { paths } => check(paths)?,
        Command::Stdin { format } => {
            let text = read_stdin()?;
            let document = format.document(STDIN_NAME.to_string(), &text);
            extract::with_japanese(document).into_iter().collect()
        }
    };

    let analyzer = Analyzer::new()?;
    let mut scores = Vec::new();
    for document in &documents {
        let sentences = analyzer.analyze_document(document);
        let context = rule::Context::for_document(&sentences);
        let findings = rule::check(&sentences, &context, DEFAULT_FLOOR);
        scores.push(DocumentScore {
            name: document.name.clone(),
            score: Score::new(
                &sentences,
                findings,
                context.measures().clone(),
                DEFAULT_FLOOR,
                context.weights(),
            ),
        });
    }
    let mut report = Report::new(scores, DEFAULT_FLOOR, rule::default_weights());
    if cli.options.summary {
        report.forget_findings();
    }

    if cli.options.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_report(&report);
    }
    Ok(cli
        .options
        .fail_over
        .is_some_and(|point| report.exceeds(point)))
}

/// 対象のパスを展開し、日本語を含むファイルを文書にする。抽出の書式を定めていない種類は、
/// 明示されたパスなら警告し、走査で見つかったファイルは黙って飛ばす。UTF-8 でないファイルは
/// 警告して飛ばす。
fn check(paths: &[PathBuf]) -> Result<Vec<Document>> {
    let mut documents = Vec::new();
    for path in paths {
        for file in input::collect_files(path)? {
            let named = file == *path;
            match input::read_document(&file) {
                Ok(input::Reading::Document(document)) => documents.push(document),
                Ok(input::Reading::NoJapanese) => {}
                Ok(input::Reading::Unsupported) => {
                    if named {
                        eprintln!("警告: {} は抽出の対象でない", file.display());
                    }
                }
                Err(error @ input::Error::NotUtf8 { .. }) => eprintln!("警告: {error}"),
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(documents)
}

fn read_stdin() -> Result<String> {
    let mut text = String::new();
    stdin()
        .read_to_string(&mut text)
        .context("標準入力を読み込めない")?;
    Ok(text)
}

fn print_report(report: &Report) {
    for document in report.documents() {
        let score = &document.score;
        if let Some(findings) = score.findings() {
            print_findings(findings);
        }
        println!("{}  {}", document.name, format_point(&score.total()));
    }
    println!("全体  {}", format_point(report.total()));
}

fn print_findings(findings: &[Finding]) {
    for finding in findings {
        let origin = finding.origin();
        println!(
            "  {}:{}  {}  {}  {}",
            origin.path,
            origin.lines.start(),
            finding.rule(),
            finding.excerpt(),
            finding.hint()
        );
    }
}

/// 規則の ID、層、重み、規則集の見出しを 1 行ずつ出力する。
fn print_rules() {
    for (rule, anchor) in rule::registered() {
        let weight = rule::default_weights()
            .get(&rule)
            .copied()
            .expect("一覧にある規則には既定の重みがある");
        let heading = rule::doc_heading(anchor).expect("一覧にある規則の anchor は見出しを指す");
        println!("{rule}  {}  {weight:.2}  {heading}", rule.layer().name());
    }
}

fn format_point(total: &Total) -> String {
    let point = match total.mode() {
        ScoreMode::Normalized { per_1000, by_layer } => {
            format!("正規化 {per_1000:.1} 点{}", format_layers(by_layer))
        }
        ScoreMode::CountOnly => format!("指摘 {} 件", total.finding_count()),
    };
    format!("{} 字  {} 文  {point}", total.ja_chars(), total.sentences())
}

/// 点を持つ層の小計。
fn format_layers(by_layer: &BTreeMap<Layer, f64>) -> String {
    if by_layer.is_empty() {
        return String::new();
    }
    let layers: Vec<String> = by_layer
        .iter()
        .map(|(layer, point)| format!("{} {point:.1}", layer.name()))
        .collect();
    format!("（{}）", layers.join("、"))
}
