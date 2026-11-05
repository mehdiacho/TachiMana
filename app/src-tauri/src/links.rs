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
}

impl From<&Link> for LinkDto {
    fn from(l: &Link) -> Self {
        Self {
            peer_device_id: l.peer_device_id.clone(),
            peer_name: l.peer_name.clone(),
            platform: l.platform.clone(),
            last_address: l.last_address.clone(),
            linked_at: l.linked_at,
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

/// Links this profile to the device at `address` with its pairing code (or a pasted link code),
/// and keeps the key.
#[tauri::command]
pub async fn link_device(app: AppHandle, address: String, code: String) -> R<LinkDto> {
    let (host, port, code) = target(&address, &code)?;
    let Some((profile_id, profile_name)) = app.state::<ProfileState>().open_profile() else {
        return Err(CmdError::new(
            "unavailable",
            "Linking needs profiles, and the profile list couldn't be read. Restart TachiMana and try again.",
        ));
    };
    let state = app.state::<AppState>();
    let req = LinkRequest {
        device_id: state.sync.identity.id.clone(),
        device_name: state.sync.identity.name.clone(),
        profile_id: profile_id.clone(),
        profile_name: profile_name.clone(),
        platform: "desktop".into(),
        port: state.sync.server_port(),
    };
    let client = SyncClient::new()?;
    if client.probe(&host, port).await.is_none() {
        return Err(CmdError::new(
            "unreachable",
            format!("No TachiMana answered at {host}:{port}. Check the address and that both devices are on the same network."),
        ));
    }
    let answer = match client.link(&host, port, &code, &req).await {
        Ok(a) => a,
        Err(SyncError::OtherProfile(theirs)) => {
            return Err(CmdError::new(
                "otherProfile",
                format!(
                    "That device is on the profile \u{201c}{}\u{201d}, and this one is \u{201c}{profile_name}\u{201d}. Link from that device instead, or switch here to the matching profile first.",
                    theirs.profile_name
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
        profile_id,
        // The answer doesn't say; a phone is what links to a PC profile today.
        platform: "android".into(),
        key,
        last_address: if host.contains(':') {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        },
        linked_at: now_ms(),
    };
    let dto = LinkDto::from(&link);
    state
        .sync
        .links
        .put(link)
        .map_err(|e| CmdError::new("io", format!("Couldn't save the link: {e}")))?;
    tracing::info!(peer = %dto.peer_device_id, "linked to a device");
    Ok(dto)
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
