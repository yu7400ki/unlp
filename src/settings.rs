use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;
use std::{fs, io, path, result};

use serde::Deserialize;
use thiserror::Error;

use crate::rule::{Layer, RuleId, WordList};

/// 設定ファイルの名前。
const FILE_NAME: &str = "unlp.toml";

const WEIGHTS: &str = include_str!("../data/weights.toml");

const LISTS: [(RuleId, &str); 9] = [
    (
        RuleId::new(Layer::Structure, 1),
        include_str!("../data/lists/S01.toml"),
    ),
    (
        RuleId::new(Layer::Structure, 3),
        include_str!("../data/lists/S03.toml"),
    ),
    (
        RuleId::new(Layer::Structure, 6),
        include_str!("../data/lists/S06.toml"),
    ),
    (
        RuleId::new(Layer::Lexical, 1),
        include_str!("../data/lists/L01.toml"),
    ),
    (
        RuleId::new(Layer::Lexical, 2),
        include_str!("../data/lists/L02.toml"),
    ),
    (
        RuleId::new(Layer::Register, 1),
        include_str!("../data/lists/R01.toml"),
    ),
    (
        RuleId::new(Layer::Register, 2),
        include_str!("../data/lists/R02.toml"),
    ),
    (
        RuleId::new(Layer::Formulaic, 1),
        include_str!("../data/lists/F01.toml"),
    ),
    (
        RuleId::new(Layer::Formulaic, 5),
        include_str!("../data/lists/F05.toml"),
    ),
];

/// 超過と判断する、1000 字あたりの点。
const THRESHOLD: f64 = 10.0;

/// 正規化した点で採点する日本語文字数の下限。
const FLOOR: usize = 300;

static DEFAULT_WEIGHTS: LazyLock<BTreeMap<RuleId, f64>> = LazyLock::new(|| {
    let table: BTreeMap<String, f64> =
        toml::from_str(WEIGHTS).expect("同梱した重みは規則 ID と数の表である");
    table
        .into_iter()
        .map(|(rule, weight)| {
            let rule = rule.parse().expect("同梱した重みの鍵は規則 ID である");
            (rule, weight)
        })
        .collect()
});

static DEFAULT_LISTS: LazyLock<BTreeMap<RuleId, WordList>> = LazyLock::new(|| {
    LISTS
        .into_iter()
        .map(|(rule, source)| {
            let list = toml::from_str(source).expect("同梱した語リストは欄の名前と語の表である");
            (rule, list)
        })
        .collect()
});

/// 設定の読み込みで生じる誤り。
#[derive(Debug, Error)]
pub enum Error {
    #[error("{} を読み込めない", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} を設定として解釈できない", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

pub type Result<T> = result::Result<T, Error>;

/// 設定ファイルの中身。欄を省いた設定は既定のままになる。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    threshold: Option<f64>,
    floor: Option<usize>,
}

/// 採点の設定。しきい値、下限、規則ごとの重み、規則ごとの語リストを持つ。
#[derive(Debug, Clone)]
pub struct Settings {
    threshold: f64,
    floor: usize,
    weights: BTreeMap<RuleId, f64>,
    lists: BTreeMap<RuleId, WordList>,
}

impl Default for Settings {
    /// 同梱した重みと語リストによる設定。
    fn default() -> Self {
        Self {
            threshold: THRESHOLD,
            floor: FLOOR,
            weights: DEFAULT_WEIGHTS.clone(),
            lists: DEFAULT_LISTS.clone(),
        }
    }
}

impl Settings {
    /// 設定ファイルを同梱した既定に重ねる。`config` があればそのファイルを、無ければ `start`
    /// から上位に走査して最初に見つかった `unlp.toml` を読む。どちらも無ければ既定になる。
    pub fn load(config: Option<&Path>, start: &Path) -> Result<Self> {
        let path = match config {
            Some(path) => Some(absolute(path)?),
            None => find(start)?,
        };
        match path {
            Some(path) => Self::default().with_file(&path),
            None => Ok(Self::default()),
        }
    }

    fn with_file(mut self, path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).map_err(|source| Error::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let file: File = toml::from_str(&text).map_err(|source| Error::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        if let Some(threshold) = file.threshold {
            self.threshold = threshold;
        }
        if let Some(floor) = file.floor {
            self.floor = floor;
        }
        Ok(self)
    }

    /// 超過と判断する、1000 字あたりの点。
    pub fn threshold(&self) -> f64 {
        self.threshold
    }

    /// 正規化した点で採点する日本語文字数の下限。
    pub fn floor(&self) -> usize {
        self.floor
    }

    /// 規則 ID ごとの指摘 1 件の重み。
    pub fn weights(&self) -> &BTreeMap<RuleId, f64> {
        &self.weights
    }

    /// 規則 ID ごとの、規則が照合する語。
    pub fn lists(&self) -> &BTreeMap<RuleId, WordList> {
        &self.lists
    }
}

/// 起点から上位に走査して最初に見つかった設定ファイル。
fn find(start: &Path) -> Result<Option<PathBuf>> {
    let start = absolute(start)?;
    Ok(start
        .ancestors()
        .map(|dir| dir.join(FILE_NAME))
        .find(|path| path.is_file()))
}

/// 現在のディレクトリを基準にして、`.` と `..` を畳んだパス。
fn absolute(path: &Path) -> Result<PathBuf> {
    let absolute = path::absolute(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let mut folded = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                folded.pop();
            }
            component => folded.push(component),
        }
    }
    Ok(folded)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::rule::registered;

    /// 設定ファイルを置いた一時ディレクトリ。
    fn dir_with(config: &str) -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), config).unwrap();
        dir
    }

    /// ディレクトリを起点に探索した設定。
    fn found_in(dir: &Path) -> Result<Settings> {
        Settings::load(None, dir)
    }

    #[test]
    fn the_defaults_weigh_every_registered_rule() {
        let settings = Settings::default();
        assert_eq!(settings.weights().len(), 22);
        for (rule, _) in registered() {
            assert!(settings.weights().contains_key(&rule), "{rule}");
        }
        assert_eq!(settings.weights()[&RuleId::new(Layer::Structure, 1)], 3.0);
    }

    #[test]
    fn the_defaults_carry_the_words_of_the_rules() {
        let settings = Settings::default();
        let list = &settings.lists()[&RuleId::new(Layer::Structure, 1)];
        assert!(list.contains("person", "利用者"));
        assert!(list.contains("speech", "述べる"));
        assert!(!list.contains("speech", "言う"));
    }

    #[test]
    fn the_defaults_bound_the_point_and_the_ja_chars() {
        let settings = Settings::default();
        assert_eq!(settings.threshold(), 10.0);
        assert_eq!(settings.floor(), 300);
    }

    #[test]
    fn a_start_without_a_file_keeps_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let settings = found_in(dir.path()).unwrap();
        assert_eq!(settings.threshold(), Settings::default().threshold());
        assert_eq!(settings.floor(), Settings::default().floor());
    }

    #[test]
    fn the_threshold_and_the_floor_are_read_as_integers() {
        let dir = dir_with("threshold = 20\nfloor = 100\n");
        let settings = found_in(dir.path()).unwrap();
        assert_eq!(settings.threshold(), 20.0);
        assert_eq!(settings.floor(), 100);
    }

    #[test]
    fn a_file_above_the_start_is_found() {
        let dir = dir_with("threshold = 20\n");
        let sub = dir.path().join("a").join("b");
        fs::create_dir_all(&sub).unwrap();
        assert_eq!(found_in(&sub).unwrap().threshold(), 20.0);
        assert_eq!(
            found_in(&sub.join("c.md")).unwrap().threshold(),
            20.0,
            "起点がファイルでも上位の設定を見つける"
        );
    }

    #[test]
    fn the_nearest_file_wins() {
        let dir = dir_with("threshold = 20\n");
        let sub = dir.path().join("a");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join(FILE_NAME), "threshold = 30\n").unwrap();
        assert_eq!(found_in(&sub).unwrap().threshold(), 30.0);
    }

    #[test]
    fn the_given_file_replaces_the_search() {
        let dir = dir_with("threshold = 20\n");
        let other = dir_with("threshold = 30\n");
        let settings = Settings::load(Some(&other.path().join(FILE_NAME)), dir.path()).unwrap();
        assert_eq!(settings.threshold(), 30.0);
    }

    #[test]
    fn a_missing_given_file_is_a_read_error() {
        let dir = tempfile::tempdir().unwrap();
        let error = Settings::load(Some(&dir.path().join(FILE_NAME)), dir.path()).unwrap_err();
        assert!(matches!(error, Error::Read { .. }), "{error}");
    }

    #[test]
    fn a_broken_file_is_a_parse_error_with_its_path() {
        for config in ["threshold =\n", "floor = \"300\"\n", "thresold = 10\n"] {
            let dir = dir_with(config);
            let error = found_in(dir.path()).unwrap_err();
            assert!(matches!(error, Error::Parse { .. }), "{config}: {error}");
            assert!(error.to_string().contains(FILE_NAME), "{error}");
        }
    }
}
