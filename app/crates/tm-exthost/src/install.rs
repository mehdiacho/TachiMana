//! Installing the helper from its zip (PLAN.md §P5.1, §P8.4): `ext-host.jar`, `lib/` and a trimmed
//! Java `runtime/`, as `app/scripts/package-ext-host.sh` packs them. The zip is untrusted input:
//! entries must stay inside the folder, sizes and counts are capped, and the old helper is only
//! replaced once the new one is fully unpacked.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Largest unpacked helper (the real one is about 250 MB).
pub const MAX_UNPACKED: u64 = 2 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 50_000;

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("this isn't a TachiMana helper zip: {0}")]
    NotAHelper(String),
    #[error("the zip is damaged: {0}")]
    Zip(String),
    #[error("the zip is larger than a helper can be")]
    TooLarge,
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Unpacks `zip` into `dest` (for example `<data>/ext-host`), replacing what is there. The host
/// must not be running from `dest`. Returns the number of files written.
pub fn install_zip(zip: &Path, dest: &Path) -> Result<usize, InstallError> {
    let parent = dest
        .parent()
        .ok_or_else(|| InstallError::NotAHelper("no folder to install into".into()))?;
    fs::create_dir_all(parent)?;
    let staging = sibling(dest, "new");
    let old = sibling(dest, "old");
    let _ = fs::remove_dir_all(&staging);
    let n = match unpack(zip, &staging) {
        Ok(n) => n,
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
    };
    let _ = fs::remove_dir_all(&old);
    if dest.exists() {
        fs::rename(dest, &old)?;
    }
    if let Err(e) = fs::rename(&staging, dest) {
        // Put the previous helper back so the app keeps working.
        if old.exists() {
            let _ = fs::rename(&old, dest);
        }
        let _ = fs::remove_dir_all(&staging);
        return Err(e.into());
    }
    let _ = fs::remove_dir_all(&old);
    Ok(n)
}

fn sibling(dest: &Path, suffix: &str) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".");
    s.push(suffix);
    PathBuf::from(s)
}

fn unpack(zip: &Path, out: &Path) -> Result<usize, InstallError> {
    let file = fs::File::open(zip)?;
    let mut archive = zip::ZipArchive::new(io::BufReader::new(file))
        .map_err(|e| InstallError::Zip(e.to_string()))?;
    if archive.len() > MAX_ENTRIES {
        return Err(InstallError::TooLarge);
    }
    fs::create_dir_all(out)?;
    let mut total: u64 = 0;
    let mut files = 0usize;
    let mut has_jar = false;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| InstallError::Zip(e.to_string()))?;
        let rel = entry
            .enclosed_name()
            .ok_or_else(|| InstallError::NotAHelper(format!("unsafe path {:?}", entry.name())))?;
        let target = out.join(&rel);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if entry.is_symlink() {
            return Err(InstallError::NotAHelper(format!("link {:?}", entry.name())));
        }
        if let Some(p) = target.parent() {
            fs::create_dir_all(p)?;
        }
        let remaining = MAX_UNPACKED.saturating_sub(total);
        let mut w = fs::File::create(&target)?;
        // Read one byte past what is left so an entry that lies about its size is caught.
        let n = io::copy(&mut (&mut entry).take(remaining.saturating_add(1)), &mut w)?;
        if n > remaining {
            return Err(InstallError::TooLarge);
        }
        total = total.saturating_add(n);
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            // Keep the executable bits (runtime/bin/java); never setuid or world-writable.
            fs::set_permissions(&target, fs::Permissions::from_mode(mode & 0o755))?;
        }
        if rel == Path::new("ext-host.jar") {
            has_jar = true;
        }
        files = files.saturating_add(1);
    }
    if !has_jar {
        return Err(InstallError::NotAHelper(
            "no ext-host.jar at the top".into(),
        ));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
    use super::*;
    use std::io::Write;

    fn make(path: &Path, entries: &[(&str, &[u8])]) {
        let mut z = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let opts = zip::write::SimpleFileOptions::default().unix_permissions(0o4777);
        for (name, body) in entries {
            z.start_file(*name, opts).unwrap();
            z.write_all(body).unwrap();
        }
        z.finish().unwrap();
    }

    #[test]
    fn installs_and_replaces_a_helper() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("ext-host");
        let a = dir.path().join("a.zip");
        make(
            &a,
            &[
                ("ext-host.jar", b"v1"),
                ("lib/x.jar", b"x"),
                ("runtime/bin/java", b"#!"),
            ],
        );
        assert_eq!(install_zip(&a, &dest).unwrap(), 3);
        assert_eq!(fs::read(dest.join("ext-host.jar")).unwrap(), b"v1");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dest.join("runtime/bin/java"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(
                mode & 0o7777,
                0o755,
                "executable kept, setuid and world-write dropped"
            );
        }
        let b = dir.path().join("b.zip");
        make(&b, &[("ext-host.jar", b"v2")]);
        install_zip(&b, &dest).unwrap();
        assert_eq!(fs::read(dest.join("ext-host.jar")).unwrap(), b"v2");
        assert!(
            !dest.join("lib").exists(),
            "the old helper is replaced, not merged"
        );
        assert!(!sibling(&dest, "old").exists() && !sibling(&dest, "new").exists());
    }

    #[test]
    fn a_bad_zip_leaves_the_installed_helper_alone() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("ext-host");
        let good = dir.path().join("good.zip");
        make(&good, &[("ext-host.jar", b"v1")]);
        install_zip(&good, &dest).unwrap();

        type Case<'a> = (&'a str, Vec<(&'a str, &'a [u8])>);
        let cases: Vec<Case> = vec![
            ("nojar.zip", vec![("readme.txt", b"hi")]),
            ("nested.zip", vec![("ext-host/ext-host.jar", b"x")]),
            (
                "escape.zip",
                vec![("../evil.jar", b"x"), ("ext-host.jar", b"x")],
            ),
        ];
        for (name, entries) in &cases {
            let p = dir.path().join(name);
            make(&p, entries);
            assert!(install_zip(&p, &dest).is_err(), "{name}");
        }
        let junk = dir.path().join("junk.zip");
        fs::write(&junk, b"not a zip").unwrap();
        assert!(matches!(
            install_zip(&junk, &dest),
            Err(InstallError::Zip(_))
        ));
        assert!(install_zip(&dir.path().join("missing.zip"), &dest).is_err());

        assert_eq!(fs::read(dest.join("ext-host.jar")).unwrap(), b"v1");
        assert!(!dir.path().join("evil.jar").exists());
        assert!(!sibling(&dest, "new").exists());

        // An absolute name is kept inside the folder, never written at the root.
        let abs = dir.path().join("abs.zip");
        make(
            &abs,
            &[("/tm-install-test-evil", b"x"), ("ext-host.jar", b"v3")],
        );
        install_zip(&abs, &dest).unwrap();
        assert!(dest.join("tm-install-test-evil").exists());
        assert!(!Path::new("/tm-install-test-evil").exists());
    }
}
