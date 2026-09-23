use std::path::{Path, PathBuf};
use std::{fs, io, result};

use serde::{Deserialize, Serialize};
use thiserror::Error;

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

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

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

    #[test]
    fn a_missing_file_is_a_read_error() {
        let dir = tempfile::tempdir().unwrap();
        let error = Manifest::load(&dir.path().join("manifest.toml")).unwrap_err();
        assert!(matches!(error, Error::Read { .. }), "{error}");
    }
}
