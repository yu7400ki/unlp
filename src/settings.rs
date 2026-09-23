use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;
use std::{fs, io, path, result};

use globset::{GlobBuilder, GlobMatcher};
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
    #[error("{} の {rule} は規則 ID でない", path.display())]
    UnknownRule { path: PathBuf, rule: String },
    #[error("{} の {rule} は語リストを持たない", path.display())]
    NoList { path: PathBuf, rule: RuleId },
    #[error("{} の {rule} に {group} の欄は無い", path.display())]
    UnknownGroup {
        path: PathBuf,
        rule: RuleId,
        group: String,
    },
    #[error("{} の {glob} はグロブとして解釈できない", path.display())]
    BadGlob {
        path: PathBuf,
        glob: String,
        #[source]
        source: globset::Error,
    },
}

pub type Result<T> = result::Result<T, Error>;

/// 設定ファイルの中身。欄を省いた設定は既定のままになる。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    threshold: Option<f64>,
    floor: Option<usize>,
    #[serde(default)]
    exclude: Vec<String>,
    #[serde(default)]
    weights: BTreeMap<String, f64>,
    #[serde(default)]
    lists: BTreeMap<String, BTreeMap<String, Words>>,
}

/// 語リストの 1 つの欄に対する、語の追加と除外。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Words {
    #[serde(default)]
    add: Vec<String>,
    #[serde(default)]
    remove: Vec<String>,
}

/// 採点の対象から外すパス。設定ファイルのあるディレクトリを根として、そこからの相対パスに
/// グロブを照合する。
#[derive(Debug, Default)]
struct Exclude {
    root: PathBuf,
    globs: Vec<GlobMatcher>,
}

impl Exclude {
    /// 設定ファイルの `exclude` からグロブを組む。`*` は区切りをまたがず、`**` はまたぐ。
    fn new(path: &Path, globs: &[String]) -> Result<Self> {
        let globs = globs
            .iter()
            .map(|glob| {
                GlobBuilder::new(glob)
                    .literal_separator(true)
                    .build()
                    .map(|built| built.compile_matcher())
                    .map_err(|source| Error::BadGlob {
                        path: path.to_path_buf(),
                        glob: glob.clone(),
                        source,
                    })
            })
            .collect::<Result<Vec<GlobMatcher>>>()?;
        Ok(Self {
            root: path.parent().unwrap_or(path).to_path_buf(),
            globs,
        })
    }

    /// 根の下にあり、グロブのいずれかに一致するパスか。
    fn matches(&self, path: &Path) -> bool {
        if self.globs.is_empty() {
            return false;
        }
        let Ok(absolute) = absolute(path) else {
            return false;
        };
        let Ok(relative) = absolute.strip_prefix(&self.root) else {
            return false;
        };
        self.globs.iter().any(|glob| glob.is_match(relative))
    }
}

/// 採点の設定。しきい値、下限、規則ごとの重み、規則ごとの語リスト、除外するパスを持つ。
#[derive(Debug)]
pub struct Settings {
    threshold: f64,
    floor: usize,
    weights: BTreeMap<RuleId, f64>,
    lists: BTreeMap<RuleId, WordList>,
    exclude: Exclude,
}

impl Default for Settings {
    /// 同梱した重みと語リストによる設定。
    fn default() -> Self {
        Self {
            threshold: THRESHOLD,
            floor: FLOOR,
            weights: DEFAULT_WEIGHTS.clone(),
            lists: DEFAULT_LISTS.clone(),
            exclude: Exclude::default(),
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
        self.exclude = Exclude::new(path, &file.exclude)?;
        for (key, weight) in file.weights {
            self.weights.insert(rule_id(&key, path)?, weight);
        }
        for (key, groups) in file.lists {
            let rule = rule_id(&key, path)?;
            let list = self.lists.get_mut(&rule).ok_or(Error::NoList {
                path: path.to_path_buf(),
                rule,
            })?;
            for (group, words) in groups {
                let listed = list.group_mut(&group).ok_or_else(|| Error::UnknownGroup {
                    path: path.to_path_buf(),
                    rule,
                    group: group.clone(),
                })?;
                listed.extend(words.add);
                listed.retain(|word| !words.remove.contains(word));
            }
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

    /// 採点の対象から外すパスか。
    pub fn excludes(&self, path: &Path) -> bool {
        self.exclude.matches(path)
    }
}

/// 設定が指す規則 ID。登録していない規則の ID は誤りにする。
fn rule_id(key: &str, path: &Path) -> Result<RuleId> {
    key.parse()
        .ok()
        .filter(|rule| DEFAULT_WEIGHTS.contains_key(rule))
        .ok_or_else(|| Error::UnknownRule {
            path: path.to_path_buf(),
            rule: key.to_string(),
        })
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

    /// S01 の語リスト。
    fn s01(settings: &Settings) -> &WordList {
        &settings.lists()[&RuleId::new(Layer::Structure, 1)]
    }

    #[test]
    fn the_weights_of_the_file_override_the_defaults() {
        let dir = dir_with("[weights]\nS01 = 6.0\nG01 = 0.0\n");
        let settings = found_in(dir.path()).unwrap();
        assert_eq!(settings.weights()[&RuleId::new(Layer::Structure, 1)], 6.0);
        assert_eq!(settings.weights()[&RuleId::new(Layer::Goshu, 1)], 0.0);
        assert_eq!(
            settings.weights().len(),
            Settings::default().weights().len(),
            "上書きは規則を増やさない"
        );
    }

    #[test]
    fn the_words_of_the_file_join_the_defaults() {
        let dir = dir_with("[lists.S01.person]\nadd = [\"型\"]\nremove = [\"利用者\"]\n");
        let settings = found_in(dir.path()).unwrap();
        assert!(s01(&settings).contains("person", "型"));
        assert!(!s01(&settings).contains("person", "利用者"));
        assert!(
            s01(&settings).contains("speech", "述べる"),
            "触れていない欄は既定のまま"
        );
    }

    #[test]
    fn a_word_in_both_the_add_and_the_remove_is_removed() {
        let dir = dir_with("[lists.S01.person]\nadd = [\"型\"]\nremove = [\"型\"]\n");
        assert!(!s01(&found_in(dir.path()).unwrap()).contains("person", "型"));
    }

    #[test]
    fn an_unknown_rule_id_is_an_error() {
        for key in ["S99", "ZZ", "s01"] {
            for config in [
                format!("[weights]\n{key} = 1.0\n"),
                format!("[lists.{key}.person]\nadd = [\"型\"]\n"),
            ] {
                let dir = dir_with(&config);
                let error = found_in(dir.path()).unwrap_err();
                assert!(
                    matches!(&error, Error::UnknownRule { rule, .. } if rule == key),
                    "{config}: {error}"
                );
            }
        }
    }

    #[test]
    fn a_rule_without_a_list_cannot_take_words() {
        let dir = dir_with("[lists.G01.person]\nadd = [\"型\"]\n");
        let error = found_in(dir.path()).unwrap_err();
        assert!(matches!(error, Error::NoList { .. }), "{error}");
        assert!(error.to_string().contains("G01"), "{error}");
    }

    #[test]
    fn an_unknown_group_of_a_list_is_an_error() {
        let dir = dir_with("[lists.S01.people]\nadd = [\"型\"]\n");
        let error = found_in(dir.path()).unwrap_err();
        assert!(matches!(error, Error::UnknownGroup { .. }), "{error}");
        assert!(error.to_string().contains("people"), "{error}");
    }

    #[test]
    fn the_globs_are_relative_to_the_directory_of_the_file() {
        let dir = dir_with("exclude = [\"tests/**\", \"src/a.rs\", \"*.md\"]\n");
        let settings = found_in(dir.path()).unwrap();
        for excluded in ["tests/a.rs", "tests/a/b.rs", "src/a.rs", "a.md"] {
            assert!(settings.excludes(&dir.path().join(excluded)), "{excluded}");
        }
        for kept in ["tests", "src/b.rs", "a/b.md", "src/a.rs.bak"] {
            assert!(!settings.excludes(&dir.path().join(kept)), "{kept}");
        }
    }

    #[test]
    fn a_path_outside_the_root_is_not_excluded() {
        let dir = dir_with("exclude = [\"**\"]\n");
        let outside = tempfile::tempdir().unwrap();
        assert!(
            !found_in(dir.path())
                .unwrap()
                .excludes(&outside.path().join("a.md"))
        );
    }

    #[test]
    fn a_file_without_globs_excludes_nothing() {
        let dir = dir_with("threshold = 20\n");
        assert!(
            !found_in(dir.path())
                .unwrap()
                .excludes(&dir.path().join("a.md"))
        );
    }

    #[test]
    fn a_glob_that_cannot_be_read_is_an_error() {
        let dir = dir_with("exclude = [\"a[\"]\n");
        let error = found_in(dir.path()).unwrap_err();
        assert!(matches!(error, Error::BadGlob { .. }), "{error}");
        assert!(error.to_string().contains("a["), "{error}");
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
