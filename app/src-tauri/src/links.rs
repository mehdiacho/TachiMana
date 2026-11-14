//! Linked devices: show this profile's link code (QR and text), link to another device by its
//! code, list and unlink (`docs/rust-port/profiles-and-remote-downloads.md` §2).
//!
//! Linking is symmetric: whichever side sends `POST /link`, both end up holding the same key, and
//! the other side then talks to this one with `X-Link-Key` instead of the six-digit code.

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tm_sync::protocol::{LinkInvite, LinkRequest};
use tm_sync::{Link, SyncClient, SyncError};
use ts_rs::TS;

use crate::dto::CmdError;
use crate::profiles::ProfileState;
use crate::state::AppState;

type R<T> = Result<T, CmdError>;

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LinkDto {
    pub peer_device_id: String,
    pub peer_name: String,
    /// `"android"` or `"desktop"`.
    pub platform: String,
    pub last_address: String,
    /// Unix milliseconds.
    #[ts(type = "number")]
    pub linked_at: i64,
    /// Last heard from or seen on the network, Unix milliseconds; 0 when not since linking.
    #[ts(type = "number")]
    pub last_seen: i64,
}

impl From<&Link> for LinkDto {
    fn from(l: &Link) -> Self {
        Self {
            peer_device_id: l.peer_device_id.clone(),
            peer_name: l.peer_name.clone(),
            platform: l.platform.clone(),
            last_address: l.last_address.clone(),
            linked_at: l.linked_at,
            last_seen: l.last_seen,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LinksDto {
    pub profile_name: Option<String>,
    /// `tachimana://link?…` for the first LAN address; `None` until the server is up, or when
    /// profiles are unavailable.
    pub uri: Option<String>,
    /// The same as a QR code, an SVG document.
    pub qr_svg: Option<String>,
    /// Every `host:port` the other device could type in.
    pub addresses: Vec<String>,
    pub pairing_code: String,
    pub links: Vec<LinkDto>,
    /// A device that tried to link from another profile and was turned away.
    pub refused: Option<RefusedDto>,
    /// Whether this profile's library is empty (taking on another profile then changes this one
    /// instead of making a new one).
    pub empty: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RefusedDto {
    pub peer_name: String,
    pub profile_name: String,
    pub address: String,
}

fn list(state: &AppState) -> Vec<LinkDto> {
    let mut v: Vec<LinkDto> = state.sync.links.list().iter().map(LinkDto::from).collect();
    v.sort_by_key(|l| std::cmp::Reverse(l.linked_at));
    v
}

fn qr_svg(text: &str) -> Option<String> {
    let code = qrcode::QrCode::with_error_correction_level(text, qrcode::EcLevel::M).ok()?;
    Some(
        code.render::<qrcode::render::svg::Color<'_>>()
            .min_dimensions(220, 220)
            .quiet_zone(true)
            .dark_color(qrcode::render::svg::Color("#000000"))
            .light_color(qrcode::render::svg::Color("#ffffff"))
            .build(),
    )
}

#[tauri::command]
pub fn links_info(state: State<'_, AppState>, profiles: State<'_, ProfileState>) -> LinksDto {
    let profile = profiles.open_profile();
    let port = state.sync.server_port();
    let addresses: Vec<String> = match port {
        Some(p) => tm_sync::discovery::local_ipv4s()
            .into_iter()
            .map(|ip| format!("{ip}:{p}"))
            .collect(),
        None => Vec::new(),
    };
    let uri = match (&profile, addresses.first()) {
        (Some((id, name)), Some(address)) => Some(
            LinkInvite {
                address: address.clone(),
                device_id: state.sync.identity.id.clone(),
                profile_id: id.clone(),
                profile_name: name.clone(),
                code: state.sync.identity.pairing_code.clone(),
            }
            .to_uri(),
        ),
        _ => None,
    };
    LinksDto {
        profile_name: profile.map(|(_, n)| n),
        qr_svg: uri.as_deref().and_then(qr_svg),
        uri,
        addresses,
        pairing_code: state.sync.identity.pairing_code.clone(),
        links: list(&state),
        refused: state
            .sync
            .refused
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map(|r| RefusedDto {
                peer_name: r.peer_name.clone(),
                profile_name: r.profile_name.clone(),
                address: r.address.clone(),
            }),
        empty: library_is_empty(&state).unwrap_or(false),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Where to link to: a pasted `tachimana://link?…` code, or `host:port` plus the code typed
/// separately.
fn target(address: &str, code: &str) -> R<(String, u16, String)> {
    let address = address.trim();
    if let Some(inv) = LinkInvite::parse(address) {
        let Some((host, port)) = inv.host_port() else {
            return Err(CmdError::new(
                "invalid",
                "That link code has no usable address.",
            ));
        };
        return Ok((host, port, inv.code));
    }
    let code = code.trim();
    if !tm_sync::identity::is_pairing_code(code) {
        return Err(CmdError::new("invalid", "The pairing code is six digits."));
    }
    let (host, port) = match address.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') || h.starts_with('[') => {
            let port: u16 = p
                .parse()
                .map_err(|_| CmdError::new("invalid", "The port after the colon is a number."))?;
            (
                h.trim_start_matches('[').trim_end_matches(']').to_owned(),
                port,
            )
        }
        _ => (address.to_owned(), tm_sync::protocol::DEFAULT_PORT),
    };
    if host.is_empty() {
        return Err(CmdError::new(
            "invalid",
            "Enter the other device's address.",
        ));
    }
    Ok((host, port, code.to_owned()))
}

/// What linking did. `restarting` when this PC took on the other device's profile: the app is
/// restarting into it and copies that device's library once it is back.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LinkOutcomeDto {
    pub link: LinkDto,
    pub restarting: bool,
}

/// Left in a profile's folder by a link that took on another device's profile; read once on the
/// next start, which then copies that device's library.
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct JoinMarker {
    peer_device_id: String,
    peer_name: String,
}

const JOIN_FILE: &str = "join.json";

fn unreachable_err(host: &str, port: u16) -> CmdError {
    CmdError::new(
        "unreachable",
        format!("No TachiMana answered at {host}:{port}. Check the address and that both devices are on the same network."),
    )
}

pub(crate) fn address_of(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

/// Whether this profile's library is empty, so it can take on another device's profile in place.
fn library_is_empty(state: &AppState) -> R<bool> {
    Ok(state.db.library()?.is_empty())
}

/// Links this profile to the device at `address` with its pairing code (or a pasted link code),
/// and keeps the key.
///
/// When the two are on different profiles the answer is an `otherProfile` error, unless `keep`
/// says what to do with the other device's library:
/// - `"merge"` (or the older `"theirs"`): this profile takes on the other device's profile and
///   its library is merged in. What is missing here is added, what both have is updated, and
///   nothing here is deleted.
/// - `"new"`: the other device's library goes into a profile of its own (made if needed), and
///   this one is left alone.
///
/// Either way this PC links, restarts into that profile and copies the other device's library.
/// Keeping this PC's library instead is chosen on the other device.
#[tauri::command]
pub async fn link_device(
    app: AppHandle,
    address: String,
    code: String,
    keep: Option<String>,
) -> R<LinkOutcomeDto> {
    let (host, port, code) = target(&address, &code)?;
    let mode = match keep.as_deref() {
        None => None,
        Some("merge" | "theirs") => Some(true),
        Some("new") => Some(false),
        Some(_) => return Err(CmdError::new("invalid", "Unknown choice.")),
    };
    let Some((profile_id, profile_name)) = app.state::<ProfileState>().open_profile() else {
        return Err(CmdError::new(
            "unavailable",
            "Linking needs profiles, and the profile list couldn't be read. Restart TachiMana and try again.",
        ));
    };
    let state = app.state::<AppState>();
    let mut req = LinkRequest {
        device_id: state.sync.identity.id.clone(),
        device_name: state.sync.identity.name.clone(),
        profile_id: profile_id.clone(),
        profile_name: profile_name.clone(),
        platform: "desktop".into(),
        port: state.sync.server_port(),
    };
    let client = SyncClient::new()?;
    if client.probe(&host, port).await.is_none() {
        return Err(unreachable_err(&host, port));
    }
    let mut joining: Option<(String, String)> = None;
    let answer = match client.link(&host, port, &code, &req).await {
        Ok(a) => a,
        Err(SyncError::OtherProfile(theirs)) if mode.is_some() => {
            // Ask again as a copy of their profile; the code is the same.
            req.profile_id.clone_from(&theirs.profile_id);
            req.profile_name.clone_from(&theirs.profile_name);
            joining = Some((theirs.profile_id.clone(), theirs.profile_name.clone()));
            client.link(&host, port, &code, &req).await?
        }
        Err(SyncError::OtherProfile(theirs)) => {
            return Err(CmdError::new(
                "otherProfile",
                format!(
                    "{} is on its own profile, \u{201c}{}\u{201d}, with its own library, and this PC is on \u{201c}{profile_name}\u{201d}. Choose how to bring its library in.",
                    theirs.device_name, theirs.profile_name
                ),
            ));
        }
        Err(e) => return Err(e.into()),
    };
    if answer.device_id == state.sync.identity.id {
        return Err(CmdError::new("invalid", "That address is this device."));
    }
    let Some(key) = answer.key else {
        return Err(CmdError::new(
            "sync",
            "The other device didn't send a link key.",
        ));
    };
    let link = Link {
        peer_device_id: answer.device_id,
        peer_name: answer.device_name,
        profile_id: req.profile_id.clone(),
        // The answer doesn't say; a phone is what links to a PC profile today.
        platform: "android".into(),
        key,
        last_address: address_of(&host, port),
        linked_at: now_ms(),
        last_seen: now_ms(),
    };
    let dto = LinkDto::from(&link);
    let Some((their_id, their_name)) = joining else {
        state
            .sync
            .links
            .put(link)
            .map_err(|e| CmdError::new("io", format!("Couldn't save the link: {e}")))?;
        tracing::info!(peer = %dto.peer_device_id, "linked to a device");
        return Ok(LinkOutcomeDto {
            link: dto,
            restarting: false,
        });
    };
    join_and_restart(&app, &their_id, &their_name, link, mode.unwrap_or(true))?;
    Ok(LinkOutcomeDto {
        link: dto,
        restarting: true,
    })
}

/// Takes on `their_id` (in this profile when `merge`, or when its library is empty), keeps
/// `link` in that profile with a note to copy the peer's library, and restarts into it.
fn join_and_restart(
    app: &AppHandle,
    their_id: &str,
    their_name: &str,
    link: Link,
    merge: bool,
) -> R<()> {
    let state = app.state::<AppState>();
    let profiles = app.state::<ProfileState>();
    let in_place = merge || library_is_empty(&state)?;
    let marker = JoinMarker {
        peer_device_id: link.peer_device_id.clone(),
        peer_name: link.peer_name.clone(),
    };
    let dir = crate::profiles::take_profile(&state, &profiles, their_id, their_name, in_place)?;
    let paths = tm_core::AppPaths::at(&dir).map_err(|e| CmdError::new("io", e.to_string()))?;
    tm_sync::LinkStore::open(&paths.links_file())
        .put(link)
        .map_err(|e| CmdError::new("io", format!("Couldn't save the link: {e}")))?;
    let bytes = serde_json::to_vec(&marker).map_err(|e| CmdError::new("io", e.to_string()))?;
    std::fs::write(dir.join(JOIN_FILE), bytes)
        .map_err(|e| CmdError::new("io", format!("Couldn't save the link: {e}")))?;
    tracing::info!(profile = %their_id, in_place, "restarting into the linked profile");
    app.restart()
}

/// On start-up: if this profile was just taken on from another device, copy that device's
/// library over the link, once. The note is removed first, so a failing copy never loops; the
/// Sync screen shows the error and Sync from another device still works by hand.
pub fn resume_join(app: &AppHandle) {
    let state = app.state::<AppState>();
    let file = state.paths.data_dir().join(JOIN_FILE);
    let Ok(bytes) = std::fs::read(&file) else {
        return;
    };
    let _ = std::fs::remove_file(&file);
    let Ok(marker) = serde_json::from_slice::<JoinMarker>(&bytes) else {
        tracing::warn!("ignoring a damaged {JOIN_FILE}");
        return;
    };
    let Some(link) = state.sync.links.by_peer(&marker.peer_device_id) else {
        tracing::warn!("no link to {} for the profile copy", marker.peer_name);
        return;
    };
    let Some((host, port)) = (LinkInvite {
        address: link.last_address.clone(),
        device_id: String::new(),
        profile_id: String::new(),
        profile_name: String::new(),
        code: String::new(),
    })
    .host_port() else {
        return;
    };
    let peer = tm_sync::PeerDevice {
        id: link.peer_device_id.clone(),
        name: link.peer_name.clone(),
        host,
        port,
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tracing::info!(peer = %peer.id, "copying the linked device's library");
        if let Err(e) = crate::sync::pull_from(&app, peer, link.key).await {
            tracing::warn!("copying the linked device's library failed: {}", e.message);
        }
    });
}

/// Takes on the profile of the device that last tried to link from another profile, after it
/// was refused, merging or in a profile of its own as `keep` says (see [`link_device`]). Needs
/// that device's own pairing code, since this side has no key for it yet.
#[tauri::command]
pub async fn link_refused(app: AppHandle, code: String, keep: Option<String>) -> R<LinkOutcomeDto> {
    let refused = app
        .state::<AppState>()
        .sync
        .refused
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
        .ok_or_else(|| CmdError::not_found("That device"))?;
    let keep = keep.unwrap_or_else(|| "merge".into());
    link_device(app, refused.address, code, Some(keep)).await
}

/// Forgets the refused attempt (the user chose to keep this PC's library, or closed the offer).
#[tauri::command]
pub fn dismiss_refused(state: State<'_, AppState>) {
    *state
        .sync
        .refused
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
}

/// Forgets the link to `peer_device_id`. The other device finds out the next time it calls.
#[tauri::command]
pub fn unlink_device(state: State<'_, AppState>, peer_device_id: String) -> R<Vec<LinkDto>> {
    state
        .sync
        .links
        .remove(&peer_device_id)
        .map_err(|e| CmdError::new("io", format!("Couldn't forget the link: {e}")))?;
    tracing::info!(peer = %peer_device_id, "unlinked a device");
    Ok(list(&state))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn targets_from_codes_and_addresses() {
        let inv = LinkInvite {
            address: "10.0.0.2:27195".into(),
            device_id: "d".into(),
            profile_id: "p".into(),
            profile_name: "Main".into(),
            code: "654321".into(),
        };
        assert_eq!(
            target(&inv.to_uri(), "").unwrap(),
            ("10.0.0.2".into(), 27195, "654321".into())
        );
        assert_eq!(
            target(" 10.0.0.3 ", "123456").unwrap(),
            ("10.0.0.3".into(), 27193, "123456".into())
        );
        assert_eq!(
            target("[fe80::1]:9", "123456").unwrap(),
            ("fe80::1".into(), 9, "123456".into())
        );
        assert!(target("10.0.0.3", "12").is_err());
        assert!(target("10.0.0.3:x", "123456").is_err());
        assert!(target("", "123456").is_err());
    }

    #[test]
    fn the_qr_code_is_an_svg() {
        let svg = qr_svg("tachimana://link?h=1.2.3.4%3A27193&d=a&p=b&n=Main&c=123456").unwrap();
        assert!(svg.contains("<svg"), "{}", &svg[..60.min(svg.len())]);
    }
}
