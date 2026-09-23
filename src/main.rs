use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, stdin};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum, value_parser};
use unlp::extract::{self, STDIN_NAME};
use unlp::morph::Analyzer;
use unlp::score::{DocumentScore, Report, Score, ScoreMode, Total};
use unlp::settings::Settings;
use unlp::{Document, Finding, Layer, git, input, rule};

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
    /// 差分が触れた Segment を検査する
    Diff {
        #[command(flatten)]
        face: DiffFace,
    },
    /// コミットメッセージを検査する
    Commits {
        /// 直近の件数
        #[arg(short = 'n', value_name = "N", value_parser = value_parser!(u32).range(1..), conflicts_with = "range")]
        number: Option<u32>,
        /// コミットの範囲
        range: Option<String>,
    },
    /// 標準入力を 1 つの文書として検査する
    Stdin {
        /// 標準入力を抽出する書式
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// git の hook として動作する
    Hook {
        #[command(subcommand)]
        command: HookCommand,
    },
    /// 規則の一覧と規則集の見出しを出力する
    Rules,
}

#[derive(Subcommand)]
enum HookCommand {
    /// メッセージのファイルと索引に載せた差分をまとめて検査する
    CommitMsg { file: PathBuf },
    /// git の commit-msg hook を設置する
    Install {
        /// 目印を持たない hook を置き換える
        #[arg(long)]
        force: bool,
    },
    /// 設置した commit-msg hook を除去する
    Uninstall,
}

/// 対象の差分。`--staged` と範囲は同時に指定できない。
#[derive(Args)]
#[group(multiple = false)]
struct DiffFace {
    /// 索引に載せた変更を対象にする
    #[arg(long)]
    staged: bool,
    /// コミットの範囲
    range: Option<String>,
}

impl DiffFace {
    /// 範囲を指定したときだけその範囲を対象にし、`--staged` と指定の無い呼び出しは索引に
    /// 載せた変更を対象にする。
    fn face(&self) -> git::Diff {
        match (self.staged, &self.range) {
            (false, Some(range)) => git::Diff::Range(range.clone()),
            (true, _) | (false, None) => git::Diff::Staged,
        }
    }
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
    /// 読み込む設定のファイル
    #[arg(long, value_name = "PATH", global = true)]
    config: Option<PathBuf>,
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
    let settings = match &cli.command {
        // hook の設置と除去は設定を読まない。壊れた設定でも hook を外せる
        Command::Hook {
            command: HookCommand::Install { .. } | HookCommand::Uninstall,
        } => Settings::default(),
        command => Settings::load(cli.options.config.as_deref(), &config_start(command))?,
    };
    let documents = match &cli.command {
        Command::Rules => {
            print_rules(&settings);
            return Ok(false);
        }
        Command::Check { paths } => check(paths, &settings)?,
        Command::Diff { face } => diff(&face.face(), &settings)?,
        Command::Commits { number, range } => git::commit_documents(&commit_range(*number, range))?,
        Command::Stdin { format } => {
            let text = read_stdin()?;
            let document = format.document(STDIN_NAME.to_string(), &text);
            extract::with_japanese(document).into_iter().collect()
        }
        Command::Hook { command } => match command {
            HookCommand::CommitMsg { file } => commit_msg_documents(file, &settings)?,
            HookCommand::Install { force } => return install_hook(*force),
            HookCommand::Uninstall => {
                if let Some(path) = git::uninstall_hook()? {
                    println!("{} を除去した", path.display());
                }
                return Ok(false);
            }
        },
    };

    let analyzer = Analyzer::new()?;
    let mut scores = Vec::new();
    for document in &documents {
        let sentences = analyzer.analyze_document(document);
        let context = rule::Context::for_document(&sentences, &settings);
        let findings = rule::check(&sentences, &context, settings.floor());
        scores.push(DocumentScore {
            name: document.name.clone(),
            score: Score::new(
                &sentences,
                findings,
                context.measures().clone(),
                settings.floor(),
                context.weights(),
            ),
        });
    }
    let mut report = Report::new(scores, settings.floor(), settings.weights());
    if cli.options.summary {
        report.forget_findings();
    }

    if cli.options.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_report(&report);
    }
    let fail_over = cli
        .options
        .fail_over
        .or(default_fail_over(&cli.command, &settings));
    Ok(fail_over.is_some_and(|point| report.exceeds(point)))
}

/// 設定ファイルを探索する起点。`check` は最初の対象のパス、他の面は現在のディレクトリ。
fn config_start(command: &Command) -> PathBuf {
    let current = || PathBuf::from(".");
    match command {
        Command::Check { paths } => paths.first().cloned().unwrap_or_else(current),
        _ => current(),
    }
}

/// `--fail-over` を指定しないときのしきい値。関門は設定のしきい値で判断する。
fn default_fail_over(command: &Command, settings: &Settings) -> Option<f64> {
    match command {
        Command::Hook {
            command: HookCommand::CommitMsg { .. },
        } => Some(settings.threshold()),
        _ => None,
    }
}

/// hook を設置し、目印を持たないファイルがあれば置き換えずに知らせる。
fn install_hook(force: bool) -> Result<bool> {
    match git::install_hook(force)? {
        git::Install::Written(path) => {
            println!("{} を設置した", path.display());
            Ok(false)
        }
        git::Install::Blocked(path) => {
            eprintln!("{} が既にある。--force で置き換える", path.display());
            Ok(true)
        }
    }
}

/// 差分の文書を読む。設定が除外するファイルは飛ばす。UTF-8 でないファイルと git が内容を
/// 返さないファイルは警告して飛ばす。
fn diff(face: &git::Diff, settings: &Settings) -> Result<Vec<Document>> {
    let diffed = git::diff_documents(face)?;
    for path in &diffed.not_utf8 {
        eprintln!("警告: {path} は UTF-8 で符号化されていない");
    }
    for (path, message) in &diffed.unreadable {
        eprintln!("警告: {path} の内容を読めない: {message}");
    }
    let toplevel = git::toplevel()?;
    Ok(diffed
        .documents
        .into_iter()
        .filter(|document| !settings.excludes(&toplevel.join(&document.name)))
        .collect())
}

/// メッセージのファイルと索引に載せた差分を 1 つの入力にする。
fn commit_msg_documents(file: &Path, settings: &Settings) -> Result<Vec<Document>> {
    let message =
        fs::read_to_string(file).with_context(|| format!("{} を読み込めない", file.display()))?;
    let mut documents: Vec<Document> = git::message_document(&message).into_iter().collect();
    documents.extend(diff(&git::Diff::Staged, settings)?);
    Ok(documents)
}

/// 対象のパスを展開し、日本語を含むファイルを文書にする。設定が除外するファイルと抽出の書式を
/// 定めていない種類は、明示されたパスなら警告し、走査で見つかったファイルは黙って飛ばす。
/// UTF-8 でないファイルは警告して飛ばす。
fn check(paths: &[PathBuf], settings: &Settings) -> Result<Vec<Document>> {
    let mut documents = Vec::new();
    for path in paths {
        for file in input::collect_files(path)? {
            let named = file == *path;
            if settings.excludes(&file) {
                if named {
                    eprintln!("警告: {} は除外の対象", file.display());
                }
                continue;
            }
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

/// 範囲を指定しなければ直近の件数で、件数も指定しなければ直近の 1 件を対象にする。
fn commit_range(number: Option<u32>, range: &Option<String>) -> git::CommitRange {
    match range {
        Some(range) => git::CommitRange::Spec(range.clone()),
        None => git::CommitRange::Last(number.unwrap_or(1)),
    }
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
fn print_rules(settings: &Settings) {
    for (rule, anchor) in rule::registered() {
        let weight = settings
            .weights()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 差分の対象を 2 通り渡した呼び出し。
    #[test]
    fn the_staged_flag_and_a_range_do_not_go_together() {
        assert!(Cli::try_parse_from(["unlp", "diff", "--staged"]).is_ok());
        assert!(Cli::try_parse_from(["unlp", "diff", "HEAD~1..HEAD"]).is_ok());
        assert!(Cli::try_parse_from(["unlp", "diff", "--staged", "HEAD~1..HEAD"]).is_err());
    }
}
