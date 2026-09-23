use std::io;
use std::path::{Path, PathBuf};
use std::{fs, result};

use thiserror::Error;

use crate::document::Document;
use crate::extract::{self, SourceLang};

/// 探索の対象から除くディレクトリの名前。
const EXCLUDED_DIRS: [&str; 3] = ["target", "node_modules", ".git"];

/// Markdown として抽出する拡張子。
const MARKDOWN_EXTENSIONS: [&str; 2] = ["md", "markdown"];

/// 全体を本文として抽出する拡張子。拡張子の無いファイルも本文にする。
const TEXT_EXTENSIONS: [&str; 1] = ["txt"];

/// 入力の読み込みで生じる誤り。
#[derive(Debug, Error)]
pub enum Error {
    #[error("{} を読み込めない", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} は UTF-8 で符号化されていない", path.display())]
    NotUtf8 { path: PathBuf },
}

pub type Result<T> = result::Result<T, Error>;

/// ファイルを抽出する書式。
enum Format {
    Markdown,
    Source(SourceLang),
    Text,
}

/// ファイルを読み込んだ結果。
#[derive(Debug)]
pub enum Reading {
    Document(Document),
    /// 日本語の文字を含む Segment が無い。
    NoJapanese,
    /// 抽出の書式を定めていない種類。
    Unsupported,
}

/// ファイルを 1 つの文書として読み込む。
pub fn read_document(path: &Path) -> Result<Reading> {
    let Some(format) = format(path) else {
        return Ok(Reading::Unsupported);
    };
    let bytes = fs::read(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let text = String::from_utf8(bytes).map_err(|_| Error::NotUtf8 {
        path: path.to_path_buf(),
    })?;
    let name = document_name(path);
    let document = match format {
        Format::Markdown => extract::markdown_document(name, &text),
        Format::Source(lang) => extract::source_document(name, &text, lang),
        Format::Text => extract::text_document(name, &text),
    };
    Ok(match extract::with_japanese(document) {
        Some(document) => Reading::Document(document),
        None => Reading::NoJapanese,
    })
}

/// 拡張子が決める書式。大小は区別しない。`.md` と `.markdown` は Markdown、構文木から
/// 取り出せる言語はソースコード、`.txt` と拡張子の無いファイルは本文。他は対象外。
fn format(path: &Path) -> Option<Format> {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase());
    match extension.as_deref() {
        None => Some(Format::Text),
        Some(extension) if MARKDOWN_EXTENSIONS.contains(&extension) => Some(Format::Markdown),
        Some(extension) if TEXT_EXTENSIONS.contains(&extension) => Some(Format::Text),
        Some(extension) => SourceLang::from_extension(extension).map(Format::Source),
    }
}

/// 文書の名前。パスの区切りは OS によらず `/` にする。
fn document_name(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// パスがディレクトリならその下のファイルを再帰的に列挙し、ファイルならそれ自身を返す。
/// 列挙は名前の順で、`target`、`node_modules`、`.git` のディレクトリには入らない。
pub fn collect_files(path: &Path) -> Result<Vec<PathBuf>> {
    let metadata = fs::metadata(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    let mut files = Vec::new();
    let mut dirs = vec![path.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let entries = fs::read_dir(&dir).map_err(|source| Error::Read {
            path: dir.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| Error::Read {
                path: dir.clone(),
                source,
            })?;
            let entry_path = entry.path();
            let file_type = entry.file_type().map_err(|source| Error::Read {
                path: entry_path.clone(),
                source,
            })?;
            if file_type.is_file() {
                files.push(entry_path);
            } else if file_type.is_dir() && !is_excluded(&entry_path) {
                dirs.push(entry_path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn is_excluded(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| EXCLUDED_DIRS.contains(&name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_path_yields_itself() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        fs::write(&file, "文だ。").unwrap();
        assert_eq!(collect_files(&file).unwrap(), [file]);
    }

    #[test]
    fn excluded_directories_are_not_walked() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "文だ。").unwrap();
        fs::create_dir(dir.path().join("node_modules")).unwrap();
        fs::write(dir.path().join("node_modules/b.txt"), "文だ。").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/c.txt"), "文だ。").unwrap();

        let files = collect_files(dir.path()).unwrap();
        assert_eq!(
            files,
            [dir.path().join("a.txt"), dir.path().join("sub/c.txt")]
        );
    }

    #[test]
    fn a_missing_path_is_a_read_error() {
        let dir = tempfile::tempdir().unwrap();
        let error = collect_files(&dir.path().join("missing")).unwrap_err();
        assert!(matches!(error, Error::Read { .. }));
    }

    /// 文書になった読み込みの中身。
    fn document_of(reading: Reading) -> Document {
        match reading {
            Reading::Document(document) => document,
            _ => panic!("文書になる読み込み"),
        }
    }

    #[test]
    fn only_files_with_japanese_become_documents() {
        let dir = tempfile::tempdir().unwrap();
        let japanese = dir.path().join("a.txt");
        let latin = dir.path().join("b.txt");
        fs::write(&japanese, "文だ。").unwrap();
        fs::write(&latin, "no japanese here\n").unwrap();

        assert!(matches!(
            read_document(&japanese).unwrap(),
            Reading::Document(_)
        ));
        assert!(matches!(
            read_document(&latin).unwrap(),
            Reading::NoJapanese
        ));
    }

    #[test]
    fn the_extension_chooses_how_the_file_is_extracted() {
        let dir = tempfile::tempdir().unwrap();
        let text = "# 見出しだ\n\n```\nコードの文だ。\n```\n";
        for (name, segments) in [("a.md", 1), ("a.MARKDOWN", 1), ("a.txt", 1), ("a", 1)] {
            let path = dir.path().join(name);
            fs::write(&path, text).unwrap();
            let document = document_of(read_document(&path).unwrap());
            assert_eq!(document.segments.len(), segments, "{name}");
        }
        let source = dir.path().join("a.rs");
        fs::write(
            &source,
            "// コメントだ。\nfn f() { let s = \"文字列だ。\"; }\n",
        )
        .unwrap();
        let document = document_of(read_document(&source).unwrap());
        assert_eq!(
            document
                .segments
                .iter()
                .map(|segment| segment.text.as_str())
                .collect::<Vec<&str>>(),
            ["コメントだ。", "文字列だ。"]
        );
        assert_eq!(
            document_of(read_document(&dir.path().join("a.md")).unwrap()).segments[0].text,
            "見出しだ"
        );
        assert_eq!(
            document_of(read_document(&dir.path().join("a.txt")).unwrap()).segments[0].text,
            text
        );
    }

    #[test]
    fn document_names_use_forward_slashes() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        let file = dir.path().join("sub").join("a.txt");
        fs::write(&file, "文だ。").unwrap();
        let name = document_of(read_document(&file).unwrap()).name;
        assert!(name.ends_with("sub/a.txt"), "{name}");
    }

    #[test]
    fn invalid_utf8_is_reported_as_such() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.txt");
        fs::write(&file, [0xff, 0xfe, 0x00]).unwrap();
        assert!(matches!(
            read_document(&file).unwrap_err(),
            Error::NotUtf8 { .. }
        ));
    }

    #[test]
    fn the_case_of_the_extension_does_not_matter() {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in [
            ("A.RS", "// 日本語だ。\n"),
            ("B.PY", "# 日本語だ。\n"),
            ("C.MARKDOWN", "日本語だ。\n"),
            ("D.TXT", "日本語だ。\n"),
        ] {
            let path = dir.path().join(name);
            fs::write(&path, text).unwrap();
            assert!(
                matches!(read_document(&path).unwrap(), Reading::Document(_)),
                "{name}"
            );
        }
    }

    #[test]
    fn an_extension_out_of_the_formats_is_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.toml", "a.json", "a.yml", "a.bin"] {
            let path = dir.path().join(name);
            fs::write(&path, "値 = \"日本語だ。\"\n").unwrap();
            assert!(
                matches!(read_document(&path).unwrap(), Reading::Unsupported),
                "{name}"
            );
        }
    }
}
