use std::io::{Read, stdin};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use unlp::extract::{self, STDIN_NAME};
use unlp::input;
use unlp::morph::Analyzer;
use unlp::score::{DEFAULT_FLOOR, DocumentScore, Measures, Report, Score, ScoreMode, Total};
use unlp::sentence::is_japanese;
use unlp::{Document, Finding};

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
    Stdin,
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
        Command::Check { paths } => check(paths)?,
        Command::Stdin => {
            let text = read_stdin()?;
            text.chars()
                .any(is_japanese)
                .then(|| extract::text_document(STDIN_NAME.to_string(), &text))
                .into_iter()
                .collect()
        }
    };

    let analyzer = Analyzer::new()?;
    let mut scores = Vec::new();
    for document in &documents {
        let sentences = analyzer.analyze_document(document);
        scores.push(DocumentScore {
            name: document.name.clone(),
            score: Score::new(&sentences, Vec::new(), Measures::default(), DEFAULT_FLOOR),
        });
    }
    let mut report = Report::new(scores, DEFAULT_FLOOR);
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

/// 対象のパスを展開し、日本語を含むファイルを文書にする。UTF-8 でないファイルは
/// 警告して飛ばす。
fn check(paths: &[PathBuf]) -> Result<Vec<Document>> {
    let mut files = Vec::new();
    for path in paths {
        files.extend(input::collect_files(path)?);
    }
    let mut documents = Vec::new();
    for file in files {
        match input::read_document(&file) {
            Ok(document) => documents.extend(document),
            Err(error @ input::Error::NotUtf8 { .. }) => eprintln!("警告: {error}"),
            Err(error) => return Err(error.into()),
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

fn format_point(total: &Total) -> String {
    let point = match total.mode() {
        ScoreMode::Normalized { per_1000, .. } => format!("正規化 {per_1000:.1} 点"),
        ScoreMode::CountOnly => format!("指摘 {} 件", total.finding_count()),
    };
    format!("{} 字  {} 文  {point}", total.ja_chars(), total.sentences())
}
