//! Profiles: whole separate libraries on one machine, like two Mihon installs on one phone.
//!
//! `<root>/profiles.json` lists them and names the active one. The first profile ("Main") lives
//! in the data folder itself, so upgrading moves nothing and the Kotlin app keeps reading it.
//! Others live in `<root>/profiles/<id>/`. A deleted profile's folder is moved aside to
//! `<root>/profiles/.deleted/`, never deleted outright.
//!
//! Contract: `docs/rust-port/profiles-and-remote-downloads.md` §1.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{io_err, AppPaths, CoreError, Result};

pub const FILE: &str = "profiles.json";
const DIR: &str = "profiles";
const DELETED: &str = ".deleted";
pub const MAX_PROFILES: usize = 20;
pub const MAX_NAME_CHARS: usize = 40;
pub const DEFAULT_COLOR: &str = "#4f8cff";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// A UUID v4, made once and shared with the devices linked to this profile.
    pub id: String,
    pub name: String,
    /// `#rrggbb`.
    pub color: String,
    /// Relative to the root: `""` for the first profile, `profiles/<id>` for the rest.
    pub folder: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profiles {
    pub active: String,
    pub profiles: Vec<Profile>,
}

/// Why a profile change was refused, worded for the user.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileError {
    #[error("Give the profile a name.")]
    NoName,
    #[error("Profile names are at most {MAX_NAME_CHARS} characters.")]
    NameTooLong,
    #[error("There is already a profile called {0}.")]
    NameTaken(String),
    #[error("That profile no longer exists.")]
    Unknown,
    #[error("You can have at most {MAX_PROFILES} profiles.")]
    TooMany,
    #[error("Switch to another profile before deleting this one.")]
    DeletingActive,
    #[error("The last profile can't be deleted.")]
    Last,
    #[error("The first profile lives in the data folder itself, so it can't be deleted. Rename it instead.")]
    First,
    #[error("The other device's profile id isn't one TachiMana can use.")]
    BadId,
    #[error("Another profile on this computer is already that profile. Switch to it and link from there.")]
    IdTaken,
}

impl Profiles {
    /// Reads `<root>/profiles.json`, making the "Main" profile for the data folder when there is
    /// none. A file that can't be read is kept as `profiles.json.bad` and replaced, so a damaged
    /// file never stops the app from starting; the libraries themselves are untouched.
    pub fn load_or_init(root: &Path) -> Result<Self> {
        let file = root.join(FILE);
        match std::fs::read(&file) {
            Ok(bytes) => {
                if let Some(p) = serde_json::from_slice::<Profiles>(&bytes)
                    .ok()
                    .and_then(Self::cleaned)
                {
                    return Ok(p);
                }
                let _ = std::fs::rename(&file, root.join(format!("{FILE}.bad")));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_err(&file)(e)),
        }
        let main = Profile {
            id: new_id(),
            name: "Main".into(),
            color: DEFAULT_COLOR.into(),
            folder: String::new(),
        };
        let p = Self {
            active: main.id.clone(),
            profiles: vec![main],
        };
        p.save(root)?;
        Ok(p)
    }

    /// Drops entries whose id or folder is malformed (a folder must never point outside the
    /// root), and fixes up names, colours and the active id. `None` when nothing usable is left.
    fn cleaned(mut self) -> Option<Self> {
        let mut seen = std::collections::HashSet::new();
        let mut folders = std::collections::HashSet::new();
        // A folder is `profiles/<uuid>`. It starts as the profile's id, and stays put when the
        // profile later takes on another device's id (`adopt_id`).
        self.profiles.retain(|p| {
            is_id(&p.id)
                && (p.folder.is_empty()
                    || p.folder.strip_prefix(&format!("{DIR}/")).is_some_and(is_id))
                && seen.insert(p.id.clone())
                && (p.folder.is_empty() || folders.insert(p.folder.clone()))
        });
        // Only one profile may own the root folder.
        let mut root_owned = false;
        self.profiles.retain(|p| {
            if p.folder.is_empty() {
                let first = !root_owned;
                root_owned = true;
                first
            } else {
                true
            }
        });
        self.profiles.truncate(MAX_PROFILES);
        for p in &mut self.profiles {
            p.name = clean_name(&p.name).unwrap_or_else(|_| "Profile".into());
            if !is_color(&p.color) {
                p.color = DEFAULT_COLOR.into();
            }
        }
        let first = self.profiles.first()?.id.clone();
        if !self.profiles.iter().any(|p| p.id == self.active) {
            self.active = first;
        }
        Some(self)
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let file = root.join(FILE);
        let tmp = root.join(format!("{FILE}.tmp"));
        let bytes = serde_json::to_vec_pretty(self)?;
        std::fs::write(&tmp, bytes).map_err(io_err(&tmp))?;
        std::fs::rename(&tmp, &file).map_err(io_err(&file))
    }

    pub fn get(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    /// The active profile. `cleaned` guarantees there is one, but this never panics either way.
    pub fn active(&self) -> Option<&Profile> {
        self.get(&self.active).or_else(|| self.profiles.first())
    }

    /// Paths for profile `id` under `root`, creating its folder.
    pub fn paths(&self, root: &Path, id: &str) -> Result<AppPaths> {
        let p = self
            .get(id)
            .ok_or(CoreError::Profile(ProfileError::Unknown))?;
        AppPaths::for_profile(root, &p.folder)
    }

    pub fn create(
        &mut self,
        name: &str,
        color: &str,
    ) -> std::result::Result<Profile, ProfileError> {
        if self.profiles.len() >= MAX_PROFILES {
            return Err(ProfileError::TooMany);
        }
        let name = self.unique_name(name, None)?;
        let id = new_id();
        let p = Profile {
            folder: format!("{DIR}/{id}"),
            id,
            name,
            color: if is_color(color) {
                color.into()
            } else {
                DEFAULT_COLOR.into()
            },
        };
        self.profiles.push(p.clone());
        Ok(p)
    }

    pub fn edit(
        &mut self,
        id: &str,
        name: &str,
        color: &str,
    ) -> std::result::Result<Profile, ProfileError> {
        let name = self.unique_name(name, Some(id))?;
        let p = self
            .profiles
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or(ProfileError::Unknown)?;
        p.name = name;
        if is_color(color) {
            color.clone_into(&mut p.color);
        }
        Ok(p.clone())
    }

    /// Gives profile `id` the id `new_id`, keeping its folder, so it becomes the same profile as
    /// another device's (`docs/rust-port/profiles-and-remote-downloads.md` §2). The other
    /// device's library is then merged into this one's by the caller.
    pub fn adopt_id(&mut self, id: &str, new_id: &str) -> std::result::Result<(), ProfileError> {
        if !is_id(new_id) {
            return Err(ProfileError::BadId);
        }
        if id == new_id {
            return Ok(());
        }
        if self.get(new_id).is_some() {
            return Err(ProfileError::IdTaken);
        }
        let p = self
            .profiles
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or(ProfileError::Unknown)?;
        new_id.clone_into(&mut p.id);
        if self.active == id {
            new_id.clone_into(&mut self.active);
        }
        Ok(())
    }

    pub fn set_active(&mut self, id: &str) -> std::result::Result<(), ProfileError> {
        if self.get(id).is_none() {
            return Err(ProfileError::Unknown);
        }
        id.clone_into(&mut self.active);
        Ok(())
    }

    /// Removes `id` from the list and moves its folder aside. Returns where the folder went
    /// (`None` when it had no folder yet). The first profile can't be removed: its data folder is
    /// the root.
    pub fn remove(
        &mut self,
        root: &Path,
        id: &str,
    ) -> std::result::Result<Option<PathBuf>, RemoveError> {
        if self.active == id {
            return Err(ProfileError::DeletingActive.into());
        }
        if self.profiles.len() <= 1 {
            return Err(ProfileError::Last.into());
        }
        let at = self
            .profiles
            .iter()
            .position(|p| p.id == id)
            .ok_or(ProfileError::Unknown)?;
        let folder = self
            .profiles
            .get(at)
            .map(|p| p.folder.clone())
            .unwrap_or_default();
        if folder.is_empty() {
            // Its data is the root itself, which also holds every other profile.
            return Err(ProfileError::First.into());
        }
        let moved = {
            let from = root.join(&folder);
            if from.is_dir() {
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let aside = root.join(DIR).join(DELETED);
                std::fs::create_dir_all(&aside).map_err(io_err(&aside))?;
                let to = aside.join(format!("{id}-{stamp}"));
                std::fs::rename(&from, &to).map_err(io_err(&from))?;
                Some(to)
            } else {
                None
            }
        };
        self.profiles.remove(at);
        Ok(moved)
    }

    fn unique_name(
        &self,
        raw: &str,
        except: Option<&str>,
    ) -> std::result::Result<String, ProfileError> {
        let name = clean_name(raw)?;
        let taken = self
            .profiles
            .iter()
            .any(|p| Some(p.id.as_str()) != except && p.name.eq_ignore_ascii_case(&name));
        if taken {
            return Err(ProfileError::NameTaken(name));
        }
        Ok(name)
    }
}

/// A [`Profiles::remove`] failure: refused, or the folder couldn't be moved.
#[derive(Debug, thiserror::Error)]
pub enum RemoveError {
    #[error(transparent)]
    Refused(#[from] ProfileError),
    #[error(transparent)]
    Io(#[from] CoreError),
}

fn clean_name(raw: &str) -> std::result::Result<String, ProfileError> {
    let name: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let name: String = name.chars().filter(|c| !c.is_control()).collect();
    if name.is_empty() {
        return Err(ProfileError::NoName);
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(ProfileError::NameTooLong);
    }
    Ok(name)
}

pub fn is_color(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c.chars().skip(1).all(|ch| ch.is_ascii_hexdigit())
}

/// A lowercase UUID v4: `8-4-4-4-12` hex digits.
pub fn is_id(id: &str) -> bool {
    let groups: Vec<&str> = id.split('-').collect();
    groups.len() == 5
        && groups.iter().map(|g| g.len()).eq([8, 4, 4, 4, 12])
        && groups.iter().all(|g| {
            g.chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        })
}

/// A random UUID v4. Falls back to the clock and process id when the OS has no randomness,
/// which is still unique enough for an id that only has to differ between this user's profiles.
pub fn new_id() -> String {
    let mut b = [0u8; 16];
    if getrandom::fill(&mut b).is_err() {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        b = (t ^ (u128::from(std::process::id()) << 64)).to_le_bytes();
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    let part = |r: std::ops::Range<usize>| h.get(r).unwrap_or_default().to_owned();
    format!(
        "{}-{}-{}-{}-{}",
        part(0..8),
        part(8..12),
        part(12..16),
        part(16..20),
        part(20..32)
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn ids_are_uuid_v4() {
        let a = new_id();
        assert!(is_id(&a), "{a}");
        assert_eq!(a.as_bytes()[14], b'4');
        assert_ne!(a, new_id());
        assert!(!is_id("../../etc"));
        assert!(!is_id("ABCDEF00-0000-4000-8000-000000000000"));
    }

    #[test]
    fn a_profile_can_take_another_devices_id_and_keeps_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut p = Profiles::load_or_init(root).unwrap();
        let made = p.create("Phone", DEFAULT_COLOR).unwrap();
        p.set_active(&made.id).unwrap();
        let before = p.paths(root, &made.id).unwrap().data_dir().to_path_buf();
        let phone = new_id();
        assert_eq!(p.adopt_id(&made.id, "nope"), Err(ProfileError::BadId));
        let main = p.profiles[0].id.clone();
        assert_eq!(p.adopt_id(&made.id, &main), Err(ProfileError::IdTaken));
        p.adopt_id(&made.id, &phone).unwrap();
        assert_eq!(p.active, phone);
        assert_eq!(p.paths(root, &phone).unwrap().data_dir(), before);
        p.save(root).unwrap();
        let back = Profiles::load_or_init(root).unwrap();
        assert_eq!(back, p, "the moved id survives a reload");
        // Two profiles may not share a folder, whatever their ids.
        let mut twice = back.clone();
        let mut copy = twice.profiles[1].clone();
        copy.id = new_id();
        twice.profiles.push(copy);
        assert_eq!(twice.cleaned().unwrap().profiles.len(), 2);
    }

    #[test]
    fn first_start_makes_main_in_the_root_and_keeps_it() {
        let dir = tempfile::tempdir().unwrap();
        let p = Profiles::load_or_init(dir.path()).unwrap();
        assert_eq!(p.profiles.len(), 1);
        let main = p.active().unwrap();
        assert_eq!((main.name.as_str(), main.folder.as_str()), ("Main", ""));
        assert_eq!(
            p.paths(dir.path(), &main.id).unwrap().data_dir(),
            dir.path()
        );
        assert_eq!(
            Profiles::load_or_init(dir.path()).unwrap(),
            p,
            "read back, not remade"
        );
    }

    #[test]
    fn create_switch_rename_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut p = Profiles::load_or_init(root).unwrap();
        let main = p.active.clone();
        let second = p.create("  Second   phone ", "#ff0000").unwrap();
        assert_eq!(second.name, "Second phone");
        assert_eq!(second.folder, format!("profiles/{}", second.id));
        assert_eq!(
            p.create("second PHONE", "").unwrap_err(),
            ProfileError::NameTaken("second PHONE".into())
        );
        assert_eq!(p.create("   ", "").unwrap_err(), ProfileError::NoName);

        let paths = p.paths(root, &second.id).unwrap();
        assert_eq!(paths.data_dir(), root.join("profiles").join(&second.id));
        assert_eq!(
            paths.lock_file(),
            root.join("app.lock"),
            "one lock per machine"
        );
        assert_eq!(paths.logs_dir(), root.join("logs"));
        assert_eq!(paths.identity_file(), root.join("device.json"));
        assert_eq!(paths.database_file(), paths.data_dir().join("tachimana.db"));
        std::fs::write(paths.database_file(), b"x").unwrap();

        p.set_active(&second.id).unwrap();
        p.save(root).unwrap();
        assert_eq!(Profiles::load_or_init(root).unwrap().active, second.id);
        assert_eq!(
            p.remove(root, &second.id).unwrap_err().to_string(),
            ProfileError::DeletingActive.to_string()
        );

        p.set_active(&main).unwrap();
        let moved = p.remove(root, &second.id).unwrap().unwrap();
        assert!(
            moved.join("tachimana.db").is_file(),
            "moved aside, not deleted"
        );
        assert!(!root.join("profiles").join(&second.id).exists());
        assert_eq!(
            p.remove(root, &main).unwrap_err().to_string(),
            ProfileError::DeletingActive.to_string()
        );
        let third = p.create("Third", "").unwrap();
        p.set_active(&third.id).unwrap();
        assert_eq!(
            p.remove(root, &main).unwrap_err().to_string(),
            ProfileError::First.to_string()
        );
        assert!(p.get(&main).is_some());
        assert!(p.edit(&main, "Mehdi", "nope").is_ok());
        assert_eq!(p.get(&main).unwrap().color, DEFAULT_COLOR);
    }

    #[test]
    fn a_damaged_or_hostile_file_never_escapes_the_root_or_stops_start_up() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join(FILE), b"{not json").unwrap();
        let p = Profiles::load_or_init(root).unwrap();
        assert_eq!(p.profiles.len(), 1);
        assert!(root.join("profiles.json.bad").is_file());

        let good = new_id();
        let hostile = serde_json::json!({
            "active": "missing",
            "profiles": [
                {"id": "../../x", "name": "a", "color": "#000000", "folder": "../../x"},
                {"id": new_id(), "name": "b", "color": "#000000", "folder": "../outside"},
                {"id": good, "name": "\u{0007}ok", "color": "red", "folder": format!("profiles/{good}")},
            ]
        });
        std::fs::write(root.join(FILE), serde_json::to_vec(&hostile).unwrap()).unwrap();
        let p = Profiles::load_or_init(root).unwrap();
        assert_eq!(p.profiles.len(), 1);
        assert_eq!(p.active, good);
        assert_eq!(p.profiles[0].name, "ok");
        assert_eq!(p.profiles[0].color, DEFAULT_COLOR);
    }
}
