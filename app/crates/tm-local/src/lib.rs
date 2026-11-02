//! The local library: `local/<manga>/<chapter>.{cbz,zip}` or `local/<manga>/<chapter>/`.
//! Ported from `desktop/src/main/kotlin/tachimana/local/LocalImporter.kt`.

pub mod recognition;
#[cfg(test)]
mod recognition_cases;

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use tm_archive::{is_image, is_supported_archive, natural_cmp};
use tm_db::{Db, LOCAL_SOURCE};

#[derive(Debug, thiserror::Error)]
pub enum LocalError {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Db(#[from] tm_db::DbError),
}

pub type Result<T> = std::result::Result<T, LocalError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct ImportResult {
    pub manga_added: usize,
    pub chapters_added: usize,
    pub manga_total: usize,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// A file's modification time in epoch milliseconds, 0 when unknown.
pub fn mtime_ms(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn file_name(p: &Path) -> Option<String> {
    p.file_name().and_then(|n| n.to_str()).map(str::to_owned)
}

/// `nameWithoutExtension`: everything before the last dot (a leading-dot name keeps its dot).
fn stem(name: &str) -> &str {
    match name.rfind('.') {
        Some(0) | None => name,
        Some(i) => name.get(..i).unwrap_or(name),
    }
}

fn sorted_children(dir: &Path) -> Result<Vec<PathBuf>> {
    let rd = std::fs::read_dir(dir).map_err(|source| LocalError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    let mut v: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    v.sort_by(|a, b| {
        natural_cmp(
            &file_name(a).unwrap_or_default(),
            &file_name(b).unwrap_or_default(),
        )
    });
    Ok(v)
}

/// The `cover.<image>` file of a manga folder (case-insensitive), first by natural order.
pub fn find_cover(manga_dir: &Path) -> Option<PathBuf> {
    sorted_children(manga_dir).ok()?.into_iter().find(|p| {
        p.is_file()
            && file_name(p).is_some_and(|n| stem(&n).eq_ignore_ascii_case("cover") && is_image(&n))
    })
}

/// Scan `root` and add new manga and chapters. Idempotent.
pub fn scan(db: &Db, root: &Path) -> Result<ImportResult> {
    let mut res = ImportResult::default();
    if !root.is_dir() {
        return Ok(res);
    }
    for manga_dir in sorted_children(root)? {
        let Some(url) = file_name(&manga_dir) else {
            continue;
        };
        if !manga_dir.is_dir() || url.starts_with('.') {
            continue;
        }
        res.manga_total += 1;
        let now = now_ms();
        let cover = find_cover(&manga_dir);
        let cover_str = cover.as_ref().map(|c| c.to_string_lossy().into_owned());

        let chapter_files: Vec<PathBuf> = sorted_children(&manga_dir)?
            .into_iter()
            .filter(|p| {
                let Some(n) = file_name(p) else { return false };
                (p.is_dir() || is_supported_archive(p))
                    && !n.starts_with('.')
                    && !stem(&n).eq_ignore_ascii_case("cover")
            })
            .collect();

        let (added_manga, added_chapters) = db.write(|c| {
            let mut added_manga = false;
            let id = match tm_db::manga_by_url_tx(c, &url, LOCAL_SOURCE)? {
                Some(m) => m.id,
                None => {
                    added_manga = true;
                    tm_db::insert_local_manga(
                        c,
                        &url,
                        cover_str.as_deref(),
                        cover.as_deref().map_or(0, mtime_ms),
                        now,
                    )?
                }
            };
            let existing: std::collections::BTreeSet<String> = tm_db::chapters_tx(c, id)?
                .into_iter()
                .map(|ch| ch.url)
                .collect();
            let mut added = 0usize;
            for (index, file) in chapter_files.iter().enumerate() {
                let Some(fname) = file_name(file) else {
                    continue;
                };
                let chapter_url = format!("{url}/{fname}");
                if existing.contains(&chapter_url) {
                    continue;
                }
                // Kotlin uses `nameWithoutExtension` for folders too ("Vol.1" -> "Vol").
                let name = stem(&fname).to_owned();
                let number = recognition::parse_chapter_number(&name, &url);
                tm_db::insert_chapter_tx(
                    c,
                    id,
                    &tm_db::SourceChapter {
                        url: chapter_url,
                        name,
                        date_upload: mtime_ms(file),
                        chapter_number: f64::from(number),
                        scanlator: None,
                    },
                    i64::try_from(index).unwrap_or(i64::MAX),
                    now,
                )?;
                added += 1;
            }
            Ok((added_manga, added))
        })?;
        if added_manga {
            res.manga_added += 1;
        }
        res.chapters_added += added_chapters;
    }
    Ok(res)
}

/// The file or folder of a local chapter (`chapter.url` = `<manga>/<file>`), confined to `root`.
pub fn chapter_path(root: &Path, chapter_url: &str) -> Option<PathBuf> {
    let (manga, file) = chapter_url.split_once('/')?;
    let ok = |s: &str| !s.is_empty() && s != "." && s != ".." && !s.contains(['/', '\\']);
    if !ok(manga) || !ok(file) {
        return None;
    }
    Some(root.join(manga).join(file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_folders_and_cbz_idempotently() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let a = root.join("Alpha");
        std::fs::create_dir_all(a.join("Chapter 2")).unwrap();
        std::fs::write(a.join("Chapter 2").join("1.png"), b"x").unwrap();
        tm_archive::pack_cbz(&[("1.jpg".into(), vec![1])], &a.join("Chapter 10.cbz")).unwrap();
        std::fs::write(a.join("cover.jpg"), b"c").unwrap();
        std::fs::write(a.join("notes.txt"), b"n").unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::create_dir_all(root.join("Beta")).unwrap();

        let db = Db::open_in_memory().unwrap();
        let r = scan(&db, root).unwrap();
        assert_eq!(
            r,
            ImportResult {
                manga_added: 2,
                chapters_added: 2,
                manga_total: 2
            }
        );
        let lib = db.library().unwrap();
        assert_eq!(
            lib.iter().map(|m| m.title.as_str()).collect::<Vec<_>>(),
            ["Alpha", "Beta"]
        );
        let alpha = &lib[0];
        assert_eq!(alpha.source, 0);
        assert_eq!(alpha.url, "Alpha");
        assert!(alpha
            .thumbnail_url
            .as_deref()
            .unwrap()
            .ends_with("cover.jpg"));
        let chs = db.chapters(alpha.id).unwrap();
        // Natural order: "Chapter 2" (index 0) before "Chapter 10.cbz" (index 1).
        let c2 = chs.iter().find(|c| c.url == "Alpha/Chapter 2").unwrap();
        let c10 = chs
            .iter()
            .find(|c| c.url == "Alpha/Chapter 10.cbz")
            .unwrap();
        assert_eq!((c2.source_order, c10.source_order), (0, 1));
        assert_eq!(c10.name, "Chapter 10");
        assert!((c10.chapter_number - 10.0).abs() < 1e-9);
        assert!((c2.chapter_number - 2.0).abs() < 1e-9);

        assert_eq!(
            scan(&db, root).unwrap(),
            ImportResult {
                manga_added: 0,
                chapters_added: 0,
                manga_total: 2
            }
        );
        tm_archive::pack_cbz(&[("1.jpg".into(), vec![1])], &a.join("Chapter 11.cbz")).unwrap();
        assert_eq!(scan(&db, root).unwrap().chapters_added, 1);
    }

    #[test]
    fn chapter_path_is_confined() {
        let root = Path::new("/lib");
        assert_eq!(
            chapter_path(root, "A/b.cbz"),
            Some(PathBuf::from("/lib/A/b.cbz"))
        );
        assert_eq!(chapter_path(root, "../x"), None);
        assert_eq!(chapter_path(root, "A/../../x"), None);
        assert_eq!(chapter_path(root, "A"), None);
    }
}
