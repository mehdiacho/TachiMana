//! File and folder names on the NAS, ported from `DiskFilenames.kt` (which ports Mihon's
//! `DiskUtil.buildValidFilename` and `DownloadProvider`). The names must match Mihon's exactly so a
//! NAS shared with Mihon finds the same chapters (plan invariant 5).

use md5::{Digest, Md5};

/// Longest file name in UTF-8 bytes.
pub const MAX_FILE_NAME_BYTES: usize = 240;

fn is_valid_fat_char(c: char) -> bool {
    !matches!(
        c,
        '\u{0}'..='\u{1f}' | '"' | '*' | '/' | ':' | '<' | '>' | '?' | '\\' | '|' | '\u{7f}'
    )
}

/// `buildValidFilename`: trims dots and spaces from both ends, replaces characters FAT does not
/// allow with `_`, optionally hex-encodes non-ASCII, and truncates to `max_bytes` UTF-8 bytes.
pub fn build_valid_filename(orig: &str, max_bytes: usize, disallow_non_ascii: bool) -> String {
    let name = orig.trim_matches(|c| c == '.' || c == ' ');
    if name.is_empty() {
        return "(invalid)".into();
    }
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if disallow_non_ascii && u32::from(c) >= 0x80 {
            if c.len_utf16() == 2 {
                // Kotlin walks UTF-16 units; a lone surrogate encodes as "?" (0x3f) each.
                out.push_str("3f3f");
            } else {
                let mut buf = [0u8; 4];
                for b in c.encode_utf8(&mut buf).bytes() {
                    out.push_str(&format!("{b:02x}"));
                }
            }
        } else if is_valid_fat_char(c) {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    truncate_to_length(&out, max_bytes)
}

/// At most `max_bytes` UTF-8 bytes, cut at a character boundary.
pub fn truncate_to_length(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_owned();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.get(..end).unwrap_or_default().to_owned()
}

/// Lower-case hex MD5 of the UTF-8 bytes.
pub fn md5_hex(s: &str) -> String {
    Md5::digest(s.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Folder of a source, e.g. `MangaDex (EN)`.
pub fn source_dir_name(source_name: &str) -> String {
    build_valid_filename(source_name, MAX_FILE_NAME_BYTES, false)
}

/// Folder of a manga.
pub fn manga_dir_name(title: &str) -> String {
    build_valid_filename(title, MAX_FILE_NAME_BYTES, false)
}

fn is_blank(s: &str) -> bool {
    s.chars().all(char::is_whitespace)
}

/// `<scanlator>_<chapter name>_<first 6 hex of md5(url)>`, the scanlator part only when set.
pub fn chapter_dir_name(
    name: &str,
    scanlator: Option<&str>,
    url: &str,
    disallow_non_ascii: bool,
) -> String {
    let base = if is_blank(name) { "Chapter" } else { name };
    let joined = match scanlator.filter(|s| !is_blank(s)) {
        Some(s) => format!("{s}_{base}"),
        None => base.to_owned(),
    };
    // 7 bytes for "_" and the hash, 4 for ".cbz".
    let mut dir = build_valid_filename(&joined, MAX_FILE_NAME_BYTES - 11, disallow_non_ascii);
    dir.push('_');
    dir.push_str(md5_hex(url).get(..6).unwrap_or_default());
    dir
}

/// Every `.cbz` name a chapter may have on the NAS, best first: the current name, then the older
/// names without the url hash, then the ASCII-only variant. Duplicates removed, order kept.
pub fn chapter_file_names(name: &str, scanlator: Option<&str>, url: &str) -> Vec<String> {
    let current = chapter_dir_name(name, scanlator, url, false);
    let base = if is_blank(name) { "Chapter" } else { name };
    let with_scanlator = match scanlator.filter(|s| !is_blank(s)) {
        Some(s) => build_valid_filename(&format!("{s}_{base}"), MAX_FILE_NAME_BYTES, false),
        None => build_valid_filename(base, MAX_FILE_NAME_BYTES, false),
    };
    let without_scanlator = build_valid_filename(base, MAX_FILE_NAME_BYTES, false);
    let ascii = chapter_dir_name(name, scanlator, url, true);
    let mut out: Vec<String> = Vec::with_capacity(4);
    for n in [current, with_scanlator, without_scanlator, ascii] {
        let file = format!("{n}.cbz");
        if !out.contains(&file) {
            out.push(file);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn names_from_remote_self_test() {
        assert_eq!(
            build_valid_filename("Manga: The / Special ? Edition", MAX_FILE_NAME_BYTES, false),
            "Manga_ The _ Special _ Edition"
        );
        let url = "https://example.com/chapter/123";
        let hash = &md5_hex(url)[..6];
        assert_eq!(
            chapter_dir_name("Chapter 1", None, url, false),
            format!("Chapter 1_{hash}")
        );
        let c = chapter_file_names("Chapter 1", Some("ScanTeam"), url);
        assert!(c.contains(&format!("ScanTeam_Chapter 1_{hash}.cbz")));
        assert!(c.contains(&"ScanTeam_Chapter 1.cbz".to_string()));
        assert_eq!(c[0], format!("ScanTeam_Chapter 1_{hash}.cbz"));
    }

    #[test]
    fn edge_cases() {
        assert_eq!(build_valid_filename(" .. ", 240, false), "(invalid)");
        assert_eq!(build_valid_filename("a\u{7}b|c", 240, false), "a_b_c");
        assert_eq!(build_valid_filename("é", 240, true), "c3a9");
        assert_eq!(build_valid_filename("😀", 240, true), "3f3f");
        assert_eq!(build_valid_filename("😀", 240, false), "😀");
        // Never cut inside a character.
        assert_eq!(truncate_to_length("aé", 2), "a");
        assert_eq!(truncate_to_length("abc", 0), "");
        assert_eq!(md5_hex(""), "d41d8cd98f00b204e9800998ecf8427e");
        assert!(chapter_dir_name("  ", Some(" "), "u", false).starts_with("Chapter_"));
        let long = "x".repeat(400);
        assert_eq!(chapter_dir_name(&long, None, "u", false).len(), 229 + 7);
        // No scanlator: the older names collapse into one.
        assert_eq!(chapter_file_names("Ch 1", None, "u").len(), 2);
    }
}
