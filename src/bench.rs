use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io, result};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::rule::{RuleId, registered};
use crate::score::{ScoreMode, Total};

/// 人間側の集合が超えない、1000 字あたりの点。
const HUMAN_MAX: f64 = 5.0;

/// Claude 側の集合が下回らない、1000 字あたりの点。
const CLAUDE_MIN: f64 = 6.0;

/// 較正の集合の書き手。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Human,
    Claude,
}

impl Side {
    /// 側の名前。
    pub fn name(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Claude => "claude",
        }
    }
}

/// 集合の文書の在り処。
#[derive(Debug, Clone)]
pub enum Source {
    /// ファイルとディレクトリ。
    Paths(Vec<PathBuf>),
    /// `git log --format=%H%x00%B%x00` の出力を保存したファイル。
    Commits(PathBuf),
}

/// 較正の 1 つの集合。
#[derive(Debug, Clone)]
pub struct Set {
    pub name: String,
    pub side: Side,
    pub source: Source,
    /// 受け入れ基準の判定に含めるか。含めない集合は点を参考として出すだけになる。
    pub judge: bool,
}

/// 較正のコーパスの一覧。
#[derive(Debug)]
pub struct Manifest {
    sets: Vec<Set>,
}

/// 一覧の読み込みで生じる誤り。
#[derive(Debug, Error)]
pub enum Error {
    #[error("{} を読み込めない", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} を較正の一覧として解釈できない", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("{} の {name} は paths と commits のどちらか一方を持つ", path.display())]
    Source { path: PathBuf, name: String },
}

pub type Result<T> = result::Result<T, Error>;

/// 一覧のファイルの中身。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default, rename = "set")]
    sets: Vec<SetFile>,
}

/// 一覧に並ぶ 1 つの集合。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SetFile {
    name: String,
    side: Side,
    paths: Option<Vec<PathBuf>>,
    commits: Option<PathBuf>,
    judge: Option<bool>,
}

impl SetFile {
    /// 相対パスを一覧のあるディレクトリから解決した集合。
    fn resolve(self, root: &Path, path: &Path) -> Result<Set> {
        let source = match (self.paths, self.commits) {
            (Some(paths), None) => {
                Source::Paths(paths.iter().map(|target| root.join(target)).collect())
            }
            (None, Some(log)) => Source::Commits(root.join(log)),
            _ => {
                return Err(Error::Source {
                    path: path.to_path_buf(),
                    name: self.name,
                });
            }
        };
        Ok(Set {
            name: self.name,
            side: self.side,
            source,
            judge: self.judge.unwrap_or(true),
        })
    }
}

impl Manifest {
    /// 一覧のファイルを読む。集合のパスは一覧のあるディレクトリを基準にする。
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).map_err(|source| Error::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let file: File = toml::from_str(&text).map_err(|source| Error::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        let root = path.parent().unwrap_or(Path::new(""));
        let sets = file
            .sets
            .into_iter()
            .map(|set| set.resolve(root, path))
            .collect::<Result<Vec<Set>>>()?;
        Ok(Self { sets })
    }

    pub fn sets(&self) -> &[Set] {
        &self.sets
    }
}

/// 集合が受け入れ基準を満たすか。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Met,
    Violated,
    /// 日本語の文字数が下限未満で正規化した点を持たず、判定の対象にならない。
    BelowFloor,
    /// 日本語の文書を 1 つも持たない。
    Empty,
    /// 一覧で判定の対象から外した参考の集合。
    Reference,
}

/// 集合 1 つの採点結果。
#[derive(Debug, Clone, Serialize)]
pub struct SetScore {
    name: String,
    side: Side,
    total: Total,
    /// 規則ごとの 1000 字あたりの件数。一覧にあるすべての規則を持つ。
    by_rule_per_1000: BTreeMap<RuleId, f64>,
    verdict: Verdict,
}

/// 較正の全体の結果。基準を外れた集合が 1 つも無ければ受け入れ基準を満たす。
#[derive(Debug, Clone, Serialize)]
pub struct BenchReport {
    sets: Vec<SetScore>,
    met: bool,
}

impl SetScore {
    /// 集合の文書を合算した集計から、集合の結果を作る。
    pub fn new(set: &Set, total: Total) -> Self {
        Self {
            name: set.name.clone(),
            side: set.side,
            by_rule_per_1000: per_1000(&total),
            verdict: verdict(set, &total),
            total,
        }
    }

    pub fn verdict(&self) -> Verdict {
        self.verdict
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn side(&self) -> Side {
        self.side
    }

    pub fn total(&self) -> &Total {
        &self.total
    }

    pub fn by_rule_per_1000(&self) -> &BTreeMap<RuleId, f64> {
        &self.by_rule_per_1000
    }

    /// 正規化した点。日本語の文字数が下限未満の集合は持たない。
    pub fn point(&self) -> Option<f64> {
        match self.total.mode() {
            ScoreMode::Normalized { per_1000, .. } => Some(*per_1000),
            ScoreMode::CountOnly => None,
        }
    }
}

impl BenchReport {
    pub fn new(sets: Vec<SetScore>) -> Self {
        let met = !sets.iter().any(|set| set.verdict() == Verdict::Violated);
        Self { sets, met }
    }

    pub fn sets(&self) -> &[SetScore] {
        &self.sets
    }

    /// 受け入れ基準を満たすか。
    pub fn met(&self) -> bool {
        self.met
    }

    /// 基準を外れた集合。
    pub fn violations(&self) -> impl Iterator<Item = &SetScore> {
        self.sets
            .iter()
            .filter(|set| set.verdict() == Verdict::Violated)
    }
}

/// 側ごとの基準に照らした判定。日本語の文書を持たない集合と、正規化した点を持たない集合は
/// 判定しない。
fn verdict(set: &Set, total: &Total) -> Verdict {
    if total.ja_chars() == 0 {
        return Verdict::Empty;
    }
    let ScoreMode::Normalized { per_1000, .. } = total.mode() else {
        return Verdict::BelowFloor;
    };
    if !set.judge {
        return Verdict::Reference;
    }
    let met = match set.side {
        Side::Human => *per_1000 <= HUMAN_MAX,
        Side::Claude => *per_1000 >= CLAUDE_MIN,
    };
    if met { Verdict::Met } else { Verdict::Violated }
}

/// 一覧にあるすべての規則の、1000 字あたりの件数。
fn per_1000(total: &Total) -> BTreeMap<RuleId, f64> {
    registered()
        .into_iter()
        .map(|(rule, _)| {
            let count = total.by_rule().get(&rule).copied().unwrap_or_default();
            let rate = match total.ja_chars() {
                0 => 0.0,
                ja_chars => count as f64 * 1000.0 / ja_chars as f64,
            };
            (rule, rate)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use tempfile::TempDir;

    use super::*;
    use crate::document::{LineRange, Origin, Segment, SegmentKind};
    use crate::measure::Measures;
    use crate::rule::{Finding, Layer};
    use crate::score::Score;
    use crate::sentence::split_sentences;
    use crate::settings::Settings;

    /// 正規化した点で採点する日本語文字数の下限。
    fn floor() -> usize {
        Settings::default().floor()
    }

    /// 一覧のファイルを置いた一時ディレクトリ。
    fn dir_with(manifest: &str) -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("manifest.toml"), manifest).unwrap();
        dir
    }

    /// 一時ディレクトリに置いた一覧を読んだ結果。
    fn loaded(dir: &TempDir) -> Result<Manifest> {
        Manifest::load(&dir.path().join("manifest.toml"))
    }

    fn paths(set: &Set) -> Vec<PathBuf> {
        match &set.source {
            Source::Paths(paths) => paths.clone(),
            Source::Commits(path) => panic!("{}", path.display()),
        }
    }

    fn commits(set: &Set) -> PathBuf {
        match &set.source {
            Source::Commits(path) => path.clone(),
            Source::Paths(paths) => panic!("{paths:?}"),
        }
    }

    #[test]
    fn the_sets_keep_the_order_of_the_file() {
        let dir = dir_with(
            "[[set]]\nname = \"zenn\"\nside = \"human\"\npaths = [\"human/zenn\"]\n\n\
             [[set]]\nname = \"sonnet-prose\"\nside = \"claude\"\npaths = [\"claude/prose\"]\n",
        );
        let manifest = loaded(&dir).unwrap();
        let names: Vec<&str> = manifest
            .sets()
            .iter()
            .map(|set| set.name.as_str())
            .collect();
        assert_eq!(names, ["zenn", "sonnet-prose"]);
        assert_eq!(manifest.sets()[0].side, Side::Human);
        assert_eq!(manifest.sets()[1].side, Side::Claude);
    }

    #[test]
    fn the_paths_are_relative_to_the_directory_of_the_file() {
        let dir = dir_with(
            "[[set]]\nname = \"a\"\nside = \"human\"\npaths = [\"human/zenn\", \"human/a.md\"]\n",
        );
        let manifest = loaded(&dir).unwrap();
        assert_eq!(
            paths(&manifest.sets()[0]),
            [
                dir.path().join("human").join("zenn"),
                dir.path().join("human").join("a.md")
            ]
        );
    }

    #[test]
    fn an_absolute_path_stays_as_it_is() {
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("a.md");
        let dir = dir_with(&format!(
            "[[set]]\nname = \"a\"\nside = \"human\"\npaths = [\"{}\"]\n",
            target.display().to_string().replace('\\', "\\\\")
        ));
        assert_eq!(paths(&loaded(&dir).unwrap().sets()[0]), [target]);
    }

    #[test]
    fn a_commits_log_is_a_file_under_the_directory_of_the_manifest() {
        let dir = dir_with(
            "[[set]]\nname = \"a\"\nside = \"human\"\ncommits = \"human/a/commits.log\"\n",
        );
        assert_eq!(
            commits(&loaded(&dir).unwrap().sets()[0]),
            dir.path().join("human").join("a").join("commits.log")
        );
    }

    #[test]
    fn a_set_holds_either_paths_or_a_commits_log() {
        for source in [
            "",
            "paths = [\"human/a\"]\ncommits = \"human/a/commits.log\"\n",
        ] {
            let dir = dir_with(&format!(
                "[[set]]\nname = \"a\"\nside = \"human\"\n{source}"
            ));
            let error = loaded(&dir).unwrap_err();
            assert!(
                matches!(&error, Error::Source { name, .. } if name == "a"),
                "{source}: {error}"
            );
        }
    }

    #[test]
    fn a_file_that_cannot_be_read_as_a_manifest_is_an_error() {
        for manifest in [
            "[[set]]\nname = \"a\"\nside = \"both\"\npaths = [\"a\"]\n",
            "[[set]]\nname = \"a\"\nside = \"human\"\npath = [\"a\"]\n",
            "[[set]]\nside = \"human\"\npaths = [\"a\"]\n",
            "[[sets]]\nname = \"a\"\nside = \"human\"\npaths = [\"a\"]\n",
        ] {
            let dir = dir_with(manifest);
            let error = loaded(&dir).unwrap_err();
            assert!(matches!(error, Error::Parse { .. }), "{manifest}: {error}");
            assert!(error.to_string().contains("manifest.toml"), "{error}");
        }
    }

    /// 集合の名前と側だけを持つ集合。
    fn set(side: Side) -> Set {
        Set {
            name: "a".to_string(),
            side,
            source: Source::Paths(Vec::new()),
            judge: true,
        }
    }

    #[test]
    fn the_judge_field_is_read_and_defaults_to_true() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("manifest.toml"),
            "[[set]]\nname = \"a\"\nside = \"claude\"\npaths = [\"a\"]\njudge = false\n\n[[set]]\nname = \"b\"\nside = \"human\"\npaths = [\"b\"]\n",
        )
        .unwrap();
        let manifest = Manifest::load(&dir.path().join("manifest.toml")).unwrap();
        let judges: Vec<bool> = manifest.sets().iter().map(|set| set.judge).collect();
        assert_eq!(judges, [false, true]);
    }

    #[test]
    fn a_set_out_of_the_judgment_is_a_reference() {
        let set = Set {
            judge: false,
            ..set(Side::Claude)
        };
        assert_eq!(verdict(&set, &total(1.0)), Verdict::Reference);
        assert!(BenchReport::new(vec![SetScore::new(&set, total(1.0))]).met());
    }

    /// S01 の指摘 1 件を数え、その重みを点にした 1000 字の集計。
    fn total(point: f64) -> Total {
        total_of(1000, point)
    }

    /// 日本語文字数と S01 の重みを指定した集計。
    fn total_of(ja_chars: usize, weight: f64) -> Total {
        let rule = RuleId::new(Layer::Structure, 1);
        let origin = Origin {
            path: "t".to_string(),
            lines: LineRange::new(NonZeroU32::MIN, NonZeroU32::MIN).unwrap(),
            commit: None,
        };
        let segment = Segment {
            text: "あ".repeat(ja_chars),
            origin: origin.clone(),
            kind: SegmentKind::Prose,
        };
        Score::new(
            &split_sentences(&segment),
            vec![Finding::new(rule, origin, "x".to_string(), "h")],
            Measures::default(),
            floor(),
            &BTreeMap::from([(rule, weight)]),
        )
        .total()
    }

    #[test]
    fn the_human_side_meets_the_criteria_up_to_five_points() {
        assert_eq!(
            SetScore::new(&set(Side::Human), total(HUMAN_MAX)).verdict(),
            Verdict::Met
        );
        assert_eq!(
            SetScore::new(&set(Side::Human), total(HUMAN_MAX + 0.1)).verdict(),
            Verdict::Violated
        );
    }

    #[test]
    fn the_claude_side_meets_the_criteria_from_six_points() {
        assert_eq!(
            SetScore::new(&set(Side::Claude), total(CLAUDE_MIN)).verdict(),
            Verdict::Met
        );
        assert_eq!(
            SetScore::new(&set(Side::Claude), total(CLAUDE_MIN - 0.1)).verdict(),
            Verdict::Violated
        );
    }

    /// 日本語の文書が 1 つも無い集合の集計。
    fn empty_total() -> Total {
        Score::new(
            &[],
            Vec::new(),
            Measures::default(),
            floor(),
            &BTreeMap::new(),
        )
        .total()
    }

    #[test]
    fn a_set_without_japanese_is_empty() {
        let score = SetScore::new(&set(Side::Claude), empty_total());
        assert_eq!(score.verdict(), Verdict::Empty);
        assert_eq!(score.point(), None);
        assert_eq!(score.total().ja_chars(), 0);
        assert!(
            BenchReport::new(vec![score]).met(),
            "空の集合は判定を落とさない"
        );
    }

    #[test]
    fn a_set_below_the_floor_has_no_point_and_no_judgment() {
        for side in [Side::Human, Side::Claude] {
            let score = SetScore::new(&set(side), total_of(floor() - 1, 100.0));
            assert_eq!(score.verdict(), Verdict::BelowFloor);
            assert_eq!(score.point(), None);
        }
    }

    #[test]
    fn the_rates_cover_every_rule() {
        let score = SetScore::new(&set(Side::Human), total(1.0));
        assert_eq!(score.point(), Some(1.0));
        assert_eq!(score.by_rule_per_1000().len(), registered().len());
        assert_eq!(
            score.by_rule_per_1000()[&RuleId::new(Layer::Structure, 1)],
            1.0
        );
        assert_eq!(score.by_rule_per_1000()[&RuleId::new(Layer::Goshu, 1)], 0.0);
    }

    #[test]
    fn one_set_outside_the_criteria_fails_the_whole_bench() {
        let met = BenchReport::new(vec![
            SetScore::new(&set(Side::Human), total(1.0)),
            SetScore::new(&set(Side::Claude), total(20.0)),
            SetScore::new(&set(Side::Claude), total_of(floor() - 1, 100.0)),
        ]);
        assert!(met.met());
        assert_eq!(met.violations().count(), 0);

        let violated = BenchReport::new(vec![
            SetScore::new(&set(Side::Human), total(1.0)),
            SetScore::new(&set(Side::Claude), total(1.0)),
        ]);
        assert!(!violated.met());
        assert_eq!(violated.violations().count(), 1);
        assert_eq!(violated.violations().next().unwrap().point(), Some(1.0));
    }

    #[test]
    fn a_missing_file_is_a_read_error() {
        let dir = tempfile::tempdir().unwrap();
        let error = Manifest::load(&dir.path().join("manifest.toml")).unwrap_err();
        assert!(matches!(error, Error::Read { .. }), "{error}");
    }
}
