//! The on-disk layout of downloads, byte for byte the Kotlin `DownloadManager`'s, so chapters the
//! Kotlin app downloaded keep working:
//! `<root>/<sourceId>/<hash(mangaUrl)>/<hash(chapterUrl)>/NNN.<ext>`, plus `meta.json` and a
//! `.done` marker written last.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Hex of the first 16 bytes of SHA-256 (32 characters), as `DownloadManager.hash`.
pub fn hash(s: &str) -> String {
    let digest = Sha256::digest(s.as_bytes());
    digest.iter().take(16).map(|b| format!("{b:02x}")).collect()
}

pub fn manga_dir(root: &Path, source: i64, manga_url: &str) -> PathBuf {
    root.join(source.to_string()).join(hash(manga_url))
}

pub fn chapter_dir(root: &Path, source: i64, manga_url: &str, chapter_url: &str) -> PathBuf {
    manga_dir(root, source, manga_url).join(hash(chapter_url))
}

pub fn done_marker(dir: &Path) -> PathBuf {
    dir.join(".done")
}

/// `NNN.<ext>` for page `index` (0-based), as Kotlin's `"%03d.%s".format(index + 1, ext)`.
pub fn page_name(index: usize, ext: &str) -> String {
    format!("{:03}.{ext}", index.saturating_add(1))
}

/// The Kotlin sniffer, quirks included (WebP is recognised by bytes 8 and 9 only; unknown is jpg).
pub fn sniff_extension(b: &[u8]) -> &'static str {
    let at = |i: usize| b.get(i).copied();
    if b.len() >= 3 && at(0) == Some(0xFF) && at(1) == Some(0xD8) {
        "jpg"
    } else if b.len() >= 8 && at(0) == Some(0x89) && at(1) == Some(b'P') {
        "png"
    } else if b.len() >= 12 && at(8) == Some(b'W') && at(9) == Some(b'E') {
        "webp"
    } else if b.len() >= 3 && at(0) == Some(b'G') && at(1) == Some(b'I') {
        "gif"
    } else {
        "jpg"
    }
}

/// `meta.json` beside each chapter, so the folder alone can rebuild the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterMeta {
    pub source_id: i64,
    pub manga_url: String,
    pub manga_title: String,
    pub chapter_url: String,
    pub chapter_name: String,
    pub page_count: u32,
}

/// Page files of a complete chapter in page order; empty when the chapter is not complete.
/// Only files whose name starts with a digit count (as Kotlin), ordered by the number before the dot.
pub fn downloaded_pages(dir: &Path) -> Vec<PathBuf> {
    if !done_marker(dir).is_file() {
        return Vec::new();
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut pages: Vec<(u32, PathBuf)> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?.to_owned();
            if !name.chars().next()?.is_ascii_digit() {
                return None;
            }
            let stem = p.file_stem()?.to_str()?;
            Some((stem.parse::<u32>().unwrap_or(0), p))
        })
        .collect();
    pages.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    pages.into_iter().map(|(_, p)| p).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_matches_kotlin() {
        // SHA-256("/manga/1") truncated to 16 bytes; the same value Kotlin's MessageDigest gives.
        let h = hash("/manga/1");
        assert_eq!(h.len(), 32);
        assert!(h
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(hash(""), "e3b0c44298fc1c149afbf4c8996fb924");
    }

    #[test]
    fn sniffs_like_kotlin() {
        assert_eq!(sniff_extension(&[0xFF, 0xD8, 0xFF]), "jpg");
        assert_eq!(sniff_extension(b"\x89PNG\r\n\x1a\n"), "png");
        assert_eq!(sniff_extension(b"RIFF\0\0\0\0WEBPVP8 "), "webp");
        assert_eq!(sniff_extension(b"GIF89a"), "gif");
        assert_eq!(sniff_extension(b"??"), "jpg");
        assert_eq!(sniff_extension(&[]), "jpg");
        assert_eq!(page_name(0, "png"), "001.png");
        assert_eq!(page_name(999, "jpg"), "1000.jpg");
    }

    #[test]
    fn meta_json_uses_kotlin_field_names() {
        let m = ChapterMeta {
            source_id: 1,
            manga_url: "/m".into(),
            manga_title: "T".into(),
            chapter_url: "/c".into(),
            chapter_name: "C".into(),
            page_count: 3,
        };
        let v = serde_json::to_value(&m).unwrap_or_default();
        for k in [
            "sourceId",
            "mangaUrl",
            "mangaTitle",
            "chapterUrl",
            "chapterName",
            "pageCount",
        ] {
            assert!(v.get(k).is_some(), "{k}");
        }
    }
}
