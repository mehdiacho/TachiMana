//! Profile commands: list, create, rename, switch, delete. A switch saves the choice and restarts
//! the app on the other profile, so nothing from the old one (downloads, uploads, the LAN server,
//! the extension host) keeps running against the new one's data.

use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tm_core::profiles::{ProfileError, RemoveError};
use tm_core::{Profile, Profiles};
use ts_rs::TS;

use crate::dto::CmdError;
use crate::state::AppState;

type R<T> = Result<T, CmdError>;

/// The profile list, held for the whole run. `id` is the profile this run has open; it does not
/// change until the restart.
pub struct ProfileState {
    pub open: String,
    list: Mutex<Profiles>,
}

impl ProfileState {
    pub fn new(open: String, list: Profiles) -> Self {
        Self {
            open,
            list: Mutex::new(list),
        }
    }

    /// Id and name of the profile this run has open; `None` when the profile list couldn't be
    /// read and the data folder was opened bare.
    pub fn open_profile(&self) -> Option<(String, String)> {
        if self.open.is_empty() {
            return None;
        }
        self.lock()
            .get(&self.open)
            .map(|p| (p.id.clone(), p.name.clone()))
    }

    fn lock(&self) -> MutexGuard<'_, Profiles> {
        // The list is only ever replaced whole after a successful save, so a panic elsewhere
        // can't leave it half-changed.
        self.list.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProfileDto {
    pub id: String,
    pub name: String,
    pub color: String,
    /// The profile this window has open.
    pub open: bool,
    /// False for the first profile, whose data is the data folder itself.
    pub removable: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProfilesDto {
    pub profiles: Vec<ProfileDto>,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProfileInput {
    pub name: String,
    pub color: String,
}

fn dto(state: &ProfileState, list: &Profiles) -> ProfilesDto {
    ProfilesDto {
        profiles: list
            .profiles
            .iter()
            .map(|p: &Profile| ProfileDto {
                id: p.id.clone(),
                name: p.name.clone(),
                color: p.color.clone(),
                open: p.id == state.open,
                removable: !p.folder.is_empty(),
            })
            .collect(),
    }
}

impl From<ProfileError> for CmdError {
    fn from(e: ProfileError) -> Self {
        CmdError::new("invalid", e.to_string())
    }
}

fn save_err(e: tm_core::CoreError) -> CmdError {
    CmdError::new("io", format!("Couldn't save the profile list: {e}"))
}

/// Changes a copy of the list and keeps it only once it is saved, so a failed save leaves both
/// the file and the running app as they were.
fn change<T>(
    app: &AppState,
    profiles: &ProfileState,
    f: impl FnOnce(&mut Profiles) -> R<T>,
) -> R<(T, ProfilesDto)> {
    let mut guard = profiles.lock();
    let mut next = guard.clone();
    let out = f(&mut next)?;
    next.save(app.paths.root()).map_err(save_err)?;
    *guard = next;
    Ok((out, dto(profiles, &guard)))
}

#[tauri::command]
pub fn profiles_list(profiles: State<'_, ProfileState>) -> ProfilesDto {
    dto(&profiles, &profiles.lock())
}

#[tauri::command]
pub fn profile_create(
    state: State<'_, AppState>,
    profiles: State<'_, ProfileState>,
    input: ProfileInput,
) -> R<ProfilesDto> {
    let (p, list) = change(&state, &profiles, |l| {
        l.create(&input.name, &input.color).map_err(Into::into)
    })?;
    tracing::info!(profile = %p.id, "profile created");
    Ok(list)
}

#[tauri::command]
pub fn profile_edit(
    state: State<'_, AppState>,
    profiles: State<'_, ProfileState>,
    id: String,
    input: ProfileInput,
) -> R<ProfilesDto> {
    change(&state, &profiles, |l| {
        l.edit(&id, &input.name, &input.color).map_err(Into::into)
    })
    .map(|(_, list)| list)
}

/// Saves `id` as the active profile and restarts into it. Returns only on failure.
#[tauri::command]
pub fn profile_switch(
    app: AppHandle,
    state: State<'_, AppState>,
    profiles: State<'_, ProfileState>,
    id: String,
) -> R<()> {
    if id == profiles.open {
        return Ok(());
    }
    change(&state, &profiles, |l| l.set_active(&id).map_err(Into::into))?;
    tracing::info!(profile = %id, "switching profile, restarting");
    app.restart()
}

/// Deletes a profile that isn't open. Its folder is moved to `profiles/.deleted/`, and the
/// answer says where, so nothing is lost by a slip.
#[tauri::command]
pub fn profile_delete(
    state: State<'_, AppState>,
    profiles: State<'_, ProfileState>,
    id: String,
) -> R<ProfilesDto> {
    if id == profiles.open {
        return Err(ProfileError::DeletingActive.into());
    }
    let root = state.paths.root().to_path_buf();
    let (moved, list) = change(&state, &profiles, |l| {
        l.remove(&root, &id).map_err(|e| match e {
            RemoveError::Refused(e) => e.into(),
            RemoveError::Io(e) => {
                CmdError::new("io", format!("Couldn't move the profile's folder: {e}"))
            }
        })
    })?;
    tracing::info!(profile = %id, moved = ?moved, "profile deleted");
    Ok(list)
}
