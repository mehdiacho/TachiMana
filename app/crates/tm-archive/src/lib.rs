//! Chapter pages from a folder of images or a ZIP/CBZ archive, and CBZ packing.
//! Ported from `desktop/src/main/kotlin/tachimana/local/ArchiveReader.kt`.

use std::cmp::Ordering;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Largest single page we will read into memory (a guard against hostile archives).
pub const MAX_PAGE_BYTES: u64 = 200 * 1024 * 1024;

pub const SUPPORTED_ARCHIVES: &[&str] = &["zip", "cbz", "cbr", "rar", "7z", "cb7", "tar", "cbt"];
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "avif", "jxl"];
const ZIP_EXTS: &[&str] = &["zip", "cbz"];

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("bad archive {path}: {source}")]
    Zip {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },
    #[error("page {0} not found")]
    NoSuchPage(String),
    #[error("page {name} is too large ({size} bytes)")]
    TooLarge { name: String, size: u64 },
    #[error("{0} is not a readable chapter")]
    Unsupported(PathBuf),
}

pub type Result<T> = std::result::Result<T, ArchiveError>;

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> ArchiveError + '_ {
    move |source| ArchiveError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn zip_err(path: &Path) -> impl FnOnce(zip::result::ZipError) -> ArchiveError + '_ {
    move |source| ArchiveError::Zip {
        path: path.to_path_buf(),
        source,
    }
}

fn ext_of(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_lowercase())
        .unwrap_or_default()
}

pub fn is_image(name: &str) -> bool {
    IMAGE_EXTS.contains(&ext_of(name).as_str())
}

pub fn is_supported_archive(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| SUPPORTED_ARCHIVES.contains(&ext_of(n).as_str()))
}

/// A folder, or a zip-family archive (RAR/7z are recognised but not yet readable).
pub fn is_readable(path: &Path) -> bool {
    path.is_dir()
        || path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| ZIP_EXTS.contains(&ext_of(n).as_str()))
}

/// Page names of a chapter in reading (natural) order.
pub fn page_names(chapter: &Path) -> Result<Vec<String>> {
    let mut names = if chapter.is_dir() {
        let mut v = Vec::new();
        for entry in std::fs::read_dir(chapter).map_err(io(chapter))? {
            let entry = entry.map_err(io(chapter))?;
            let is_file = entry.file_type().map(|t| t.is_file()).unwrap_or(false);
            if let Some(name) = entry.file_name().to_str() {
                if is_file && is_image(name) {
                    v.push(name.to_owned());
                }
            }
        }
        v
    } else if is_readable(chapter) {
        let file = File::open(chapter).map_err(io(chapter))?;
        let zip = zip::ZipArchive::new(file).map_err(zip_err(chapter))?;
        zip.file_names()
            .filter_map(|n| n.ok())
            .filter(|n| !n.ends_with('/') && is_image(n))
            .map(|n| n.into_owned())
            .collect()
    } else {
        return Err(ArchiveError::Unsupported(chapter.to_path_buf()));
    };
    names.sort_by(|a, b| natural_cmp(a, b));
    Ok(names)
}

/// Bytes of one page by name.
pub fn read_page(chapter: &Path, name: &str) -> Result<Vec<u8>> {
    if chapter.is_dir() {
        // Only names that `page_names` could have produced: a plain file name, no separators.
        if name.contains(['/', '\\']) || name == ".." || name == "." {
            return Err(ArchiveError::NoSuchPage(name.to_owned()));
        }
        let path = chapter.join(name);
        let size = std::fs::metadata(&path).map_err(io(&path))?.len();
        if size > MAX_PAGE_BYTES {
            return Err(ArchiveError::TooLarge {
                name: name.to_owned(),
                size,
            });
        }
        return std::fs::read(&path).map_err(io(&path));
    }
    let file = File::open(chapter).map_err(io(chapter))?;
    let mut zip = zip::ZipArchive::new(file).map_err(zip_err(chapter))?;
    let entry = zip.by_name(name).map_err(|e| match e {
        zip::result::ZipError::FileNotFound => ArchiveError::NoSuchPage(name.to_owned()),
        other => zip_err(chapter)(other),
    })?;
    let size = entry.size();
    if size > MAX_PAGE_BYTES {
        return Err(ArchiveError::TooLarge {
            name: name.to_owned(),
            size,
        });
    }
    let mut buf = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    // `take` guards against an entry whose real size exceeds its declared size.
    entry
        .take(MAX_PAGE_BYTES.saturating_add(1))
        .read_to_end(&mut buf)
        .map_err(io(chapter))?;
    if u64::try_from(buf.len()).unwrap_or(u64::MAX) > MAX_PAGE_BYTES {
        return Err(ArchiveError::TooLarge {
            name: name.to_owned(),
            size: MAX_PAGE_BYTES,
        });
    }
    Ok(buf)
}

/// How many leading bytes of a page are read to find its pixel size. JPEG headers can sit after
/// a large EXIF block, so this is generous; pages whose size is not found report `None`.
const HEADER_PREFIX: u64 = 512 * 1024;

/// Pixel size (width, height) of each named page, read from the image header only. One pass over
/// the archive; a page that cannot be read or parsed gives `None` rather than failing the list.
pub fn page_dimensions(chapter: &Path, names: &[String]) -> Result<Vec<Option<(u32, u32)>>> {
    let size_of = |bytes: &[u8]| {
        imagesize::blob_size(bytes).ok().and_then(|d| {
            let w = u32::try_from(d.width).ok()?;
            let h = u32::try_from(d.height).ok()?;
            (w > 0 && h > 0).then_some((w, h))
        })
    };
    if chapter.is_dir() {
        return Ok(names
            .iter()
            .map(|n| {
                if n.contains(['/', '\\']) || n == ".." || n == "." {
                    return None;
                }
                let f = File::open(chapter.join(n)).ok()?;
                let mut buf = Vec::new();
                f.take(HEADER_PREFIX).read_to_end(&mut buf).ok()?;
                size_of(&buf)
            })
            .collect());
    }
    let file = File::open(chapter).map_err(io(chapter))?;
    let mut zip = zip::ZipArchive::new(file).map_err(zip_err(chapter))?;
    let mut out = Vec::with_capacity(names.len().min(10_000));
    for n in names {
        let dims = match zip.by_name(n) {
            Ok(entry) => {
                let mut buf = Vec::new();
                match entry.take(HEADER_PREFIX).read_to_end(&mut buf) {
                    Ok(_) => size_of(&buf),
                    Err(_) => None,
                }
            }
            Err(_) => None,
        };
        out.push(dims);
    }
    Ok(out)
}

/// Write pages as an uncompressed (stored) CBZ at `out`, atomically (temp file, then rename).
/// Images are already compressed, so storing them is faster and the same size.
pub fn pack_cbz(pages: &[(String, Vec<u8>)], out: &Path) -> Result<()> {
    let tmp = out.with_extension("cbz.part");
    {
        let file = File::create(&tmp).map_err(io(&tmp))?;
        let mut w = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in pages {
            w.start_file(name.as_str(), opts).map_err(zip_err(&tmp))?;
            w.write_all(bytes).map_err(io(&tmp))?;
        }
        w.finish().map_err(zip_err(&tmp))?;
    }
    std::fs::rename(&tmp, out).map_err(io(out))
}

/// Natural (human) ordering so "2.jpg" sorts before "10.jpg". Same algorithm as the Kotlin
/// `NaturalOrder`: digit runs compare by value (leading zeros ignored), other characters
/// case-insensitively, and on a tie the shorter remainder sorts first.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (mut i, mut j) = (0usize, 0usize);
    while let (Some(&ca), Some(&cb)) = (a.get(i), b.get(j)) {
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let ei = i + a
                .get(i..)
                .map_or(0, |s| s.iter().take_while(|c| c.is_ascii_digit()).count());
            let ej = j + b
                .get(j..)
                .map_or(0, |s| s.iter().take_while(|c| c.is_ascii_digit()).count());
            let na = trim_zeros(a.get(i..ei).unwrap_or_default());
            let nb = trim_zeros(b.get(j..ej).unwrap_or_default());
            let cmp = na.len().cmp(&nb.len()).then_with(|| na.cmp(nb));
            if cmp != Ordering::Equal {
                return cmp;
            }
            i = ei;
            j = ej;
        } else {
            let cmp = lower(ca).cmp(&lower(cb));
            if cmp != Ordering::Equal {
                return cmp;
            }
            i += 1;
            j += 1;
        }
    }
    (a.len().saturating_sub(i)).cmp(&b.len().saturating_sub(j))
}

fn trim_zeros(s: &[char]) -> &[char] {
    let start = s.iter().position(|c| *c != '0').unwrap_or(s.len());
    match s.get(start..) {
        Some(rest) if !rest.is_empty() => rest,
        // All zeros (or empty) compares as "0".
        _ => &['0'],
    }
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_header(w: u32, h: u32) -> Vec<u8> {
        let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        v
    }

    #[test]
    fn page_dimensions_from_headers() {
        let dir = tempfile::tempdir().unwrap();
        let cbz = dir.path().join("c.cbz");
        pack_cbz(
            &[
                ("1.png".into(), png_header(800, 1200)),
                ("2.png".into(), b"not an image".to_vec()),
            ],
            &cbz,
        )
        .unwrap();
        let names = page_names(&cbz).unwrap();
        let dims = page_dimensions(&cbz, &names).unwrap();
        assert_eq!(dims, [Some((800, 1200)), None]);
        let missing = page_dimensions(&cbz, &["nope.png".into()]).unwrap();
        assert_eq!(missing, [None]);

        let folder = dir.path().join("f");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("a.png"), png_header(10, 20)).unwrap();
        let dims = page_dimensions(&folder, &["a.png".into(), "../c.cbz".into()]).unwrap();
        assert_eq!(dims, [Some((10, 20)), None]);
    }

    #[test]
    fn natural_order() {
        let mut v = vec![
            "10.jpg", "2.jpg", "1.jpg", "002b.jpg", "B.png", "a.png", "001.jpg",
        ];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            v,
            ["1.jpg", "001.jpg", "2.jpg", "002b.jpg", "10.jpg", "a.png", "B.png"]
        );
        assert_eq!(natural_cmp("page", "page2"), Ordering::Less);
        assert_eq!(natural_cmp("x00", "x0"), Ordering::Equal);
    }

    #[test]
    fn folder_and_cbz_pages() {
        let dir = tempfile::tempdir().unwrap();
        let ch = dir.path().join("Ch 1");
        std::fs::create_dir(&ch).unwrap();
        for n in ["10.png", "2.png", "notes.txt"] {
            std::fs::write(ch.join(n), n.as_bytes()).unwrap();
        }
        assert_eq!(page_names(&ch).unwrap(), ["2.png", "10.png"]);
        assert_eq!(read_page(&ch, "2.png").unwrap(), b"2.png");
        assert!(read_page(&ch, "../x").is_err());

        let cbz = dir.path().join("Ch 2.cbz");
        pack_cbz(
            &[("b/10.jpg".into(), vec![1]), ("b/9.jpg".into(), vec![2, 3])],
            &cbz,
        )
        .unwrap();
        assert_eq!(page_names(&cbz).unwrap(), ["b/9.jpg", "b/10.jpg"]);
        assert_eq!(read_page(&cbz, "b/9.jpg").unwrap(), [2, 3]);
        assert!(matches!(
            read_page(&cbz, "nope"),
            Err(ArchiveError::NoSuchPage(_))
        ));
        assert!(is_supported_archive(&cbz));
        assert!(!is_readable(&dir.path().join("x.cbr")));
    }

    #[test]
    fn garbage_archive_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("bad.cbz");
        std::fs::write(&bad, b"not a zip").unwrap();
        assert!(page_names(&bad).is_err());
        assert!(read_page(&bad, "1.jpg").is_err());
    }
}
