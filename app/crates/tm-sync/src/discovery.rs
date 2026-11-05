//! mDNS advertise and browse on `_tachimana._tcp.local.` (`LanDiscovery.kt`), so peers find each
//! other with no setup. The TXT record carries `id` and `name`, like the Kotlin app's JmDNS
//! service. Where mDNS is blocked (some Windows and VPN setups) a peer can still be added by
//! address.

use std::collections::HashMap;
use std::sync::Arc;

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};

use crate::protocol::{PeerDevice, SERVICE_TYPE};
use crate::{Result, SyncError};

pub struct Discovery {
    daemon: ServiceDaemon,
    registered: Option<String>,
}

impl Discovery {
    pub fn new() -> Result<Self> {
        let daemon = ServiceDaemon::new().map_err(|e| SyncError::Bind(format!("mDNS: {e}")))?;
        Ok(Self {
            daemon,
            registered: None,
        })
    }

    /// Advertise this device on `port`. `profile_id` goes in the TXT record as `p`, so peers of
    /// the same profile can be listed first.
    pub fn advertise(
        &mut self,
        device_id: &str,
        device_name: &str,
        profile_id: Option<&str>,
        port: u16,
    ) -> Result<()> {
        let mut props: HashMap<String, String> = [
            ("id".to_owned(), device_id.to_owned()),
            ("name".to_owned(), device_name.to_owned()),
        ]
        .into_iter()
        .collect();
        if let Some(p) = profile_id {
            props.insert("p".to_owned(), p.to_owned());
        }
        let host: String = device_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        let info = ServiceInfo::new(
            SERVICE_TYPE,
            device_id,
            &format!("tachimana-{host}.local."),
            "",
            port,
            props,
        )
        .map_err(|e| SyncError::Bind(format!("mDNS: {e}")))?
        .enable_addr_auto();
        let full = info.get_fullname().to_owned();
        self.daemon
            .register(info)
            .map_err(|e| SyncError::Bind(format!("mDNS: {e}")))?;
        self.registered = Some(full);
        Ok(())
    }

    /// Browse for peers on a background thread; `on_peer` runs for each one resolved, except
    /// this device. Runs until the daemon shuts down.
    pub fn browse(
        &self,
        self_id: &str,
        on_peer: Arc<dyn Fn(PeerDevice) + Send + Sync>,
    ) -> Result<()> {
        let rx = self
            .daemon
            .browse(SERVICE_TYPE)
            .map_err(|e| SyncError::Bind(format!("mDNS: {e}")))?;
        let self_id = self_id.to_owned();
        std::thread::Builder::new()
            .name("mdns-browse".into())
            .spawn(move || {
                while let Ok(ev) = rx.recv() {
                    let ServiceEvent::ServiceResolved(s) = ev else {
                        continue;
                    };
                    let fallback = s
                        .get_fullname()
                        .split('.')
                        .next()
                        .unwrap_or_default()
                        .to_owned();
                    let id = s
                        .get_property_val_str("id")
                        .map(str::to_owned)
                        .unwrap_or(fallback);
                    if id.is_empty() || id == self_id {
                        continue;
                    }
                    let mut v4: Vec<_> = s.get_addresses_v4().into_iter().collect();
                    v4.sort();
                    let host = match v4.first() {
                        Some(ip) => ip.to_string(),
                        None => match s.get_addresses().iter().next() {
                            Some(ip) => ip.to_ip_addr().to_string(),
                            None => continue,
                        },
                    };
                    let name = s
                        .get_property_val_str("name")
                        .map(str::to_owned)
                        .unwrap_or_else(|| id.clone());
                    on_peer(PeerDevice {
                        id,
                        name,
                        host,
                        port: s.get_port(),
                    });
                }
            })
            .map_err(SyncError::Io)?;
        Ok(())
    }
}

impl Drop for Discovery {
    fn drop(&mut self) {
        if let Some(full) = self.registered.take() {
            let _ = self.daemon.unregister(&full);
        }
        let _ = self.daemon.shutdown();
    }
}

/// This machine's LAN IPv4 addresses (no loopback), for typing into another device by hand.
pub fn local_ipv4s() -> Vec<String> {
    let mut v: Vec<String> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.ip() {
            std::net::IpAddr::V4(ip) if !ip.is_link_local() => Some(ip.to_string()),
            _ => None,
        })
        .collect();
    v.sort();
    v.dedup();
    v
}
