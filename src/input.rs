use std::io;
use std::path::{Path, PathBuf};
use std::{fs, result};

use thiserror::Error;

use crate::document::Document;
use crate::extract;

/// 探索の対象から除くディレクトリの名前。
const EXCLUDED_DIRS: [&str; 3] = ["target", "node_modules", ".git"];

/// Markdown として抽出する拡張子。
const MARKDOWN_EXTENSIONS: [&str; 2] = ["md", "markdown"];

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

/// ファイルを 1 つの文書として読み込む。`.md` と `.markdown` は Markdown の本文を、
/// 他の拡張子は全体を抽出する。日本語の文字を含む Segment が無ければ `None`。
pub fn read_document(path: &Path) -> Result<Option<Document>> {
    let bytes = fs::read(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let text = String::from_utf8(bytes).map_err(|_| Error::NotUtf8 {
        path: path.to_path_buf(),
    })?;
    let name = document_name(path);
    let document = if is_markdown(path) {
        extract::markdown_document(name, &text)
    } else {
        extract::text_document(name, &text)
    };
    Ok(extract::with_japanese(document))
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            MARKDOWN_EXTENSIONS
                .iter()
                .any(|markdown| extension.eq_ignore_ascii_case(markdown))
        })
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

    #[test]
    fn only_files_with_japanese_become_documents() {
        let dir = tempfile::tempdir().unwrap();
        let japanese = dir.path().join("a.txt");
        let latin = dir.path().join("b.txt");
        fs::write(&japanese, "文だ。").unwrap();
        fs::write(&latin, "no japanese here\n").unwrap();

        assert!(read_document(&japanese).unwrap().is_some());
        assert!(read_document(&latin).unwrap().is_none());
    }

    #[test]
    fn the_extension_chooses_how_the_file_is_extracted() {
        let dir = tempfile::tempdir().unwrap();
        let text = "# 見出しだ\n\n```\nコードの文だ。\n```\n";
        for (name, segments) in [("a.md", 1), ("a.MARKDOWN", 1), ("a.txt", 1), ("a", 1)] {
            let path = dir.path().join(name);
            fs::write(&path, text).unwrap();
            let document = read_document(&path).unwrap().unwrap();
            assert_eq!(document.segments.len(), segments, "{name}");
        }
        assert_eq!(
            read_document(&dir.path().join("a.md"))
                .unwrap()
                .unwrap()
                .segments[0]
                .text,
            "見出しだ"
        );
        assert_eq!(
            read_document(&dir.path().join("a.txt"))
                .unwrap()
                .unwrap()
                .segments[0]
                .text,
            text
        );
    }

    #[test]
    fn document_names_use_forward_slashes() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        let file = dir.path().join("sub").join("a.txt");
        fs::write(&file, "文だ。").unwrap();
        let name = read_document(&file).unwrap().unwrap().name;
        assert!(name.ends_with("sub/a.txt"), "{name}");
    }

    #[test]
    fn invalid_utf8_is_reported_as_such() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.bin");
        fs::write(&file, [0xff, 0xfe, 0x00]).unwrap();
        assert!(matches!(
            read_document(&file).unwrap_err(),
            Error::NotUtf8 { .. }
        ));
    }
}
