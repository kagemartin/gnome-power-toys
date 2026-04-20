// crates/gnome-zones-daemon/src/monitors.rs
use crate::error::Result;
use crate::model::MonitorInfo;
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use zbus::{proxy, Connection};

/// Trait over the source of monitor information. Real implementation hits
/// `org.gnome.Mutter.DisplayConfig`; tests inject a mock.
#[async_trait]
pub trait MonitorService: Send + Sync {
    async fn list_monitors(&self) -> Result<Vec<MonitorInfo>>;
}

pub fn compute_monitor_key(connector: &str, edid: &[u8]) -> String {
    if edid.is_empty() {
        format!("{connector}:no-edid")
    } else {
        let digest = Sha256::digest(edid);
        let short = hex::encode(&digest[..4]);  // 8 hex chars
        format!("{connector}:{short}")
    }
}

// ---- Real Mutter-backed implementation ----

#[proxy(
    interface    = "org.gnome.Mutter.DisplayConfig",
    default_service = "org.gnome.Mutter.DisplayConfig",
    default_path    = "/org/gnome/Mutter/DisplayConfig"
)]
trait MutterDisplayConfig {
    /// Returns (serial, monitors, logical_monitors, properties).
    /// We only use `monitors` and `logical_monitors`.
    fn get_current_state(&self) -> zbus::Result<(
        u32,
        Vec<MutterMonitor>,
        Vec<MutterLogicalMonitor>,
        std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
    )>;
}

type MutterMonitor = (
    (String, String, String, String),   // connector, vendor, product, serial
    Vec<(String, i32, i32, f64, f64, Vec<f64>, std::collections::HashMap<String, zbus::zvariant::OwnedValue>)>,
    std::collections::HashMap<String, zbus::zvariant::OwnedValue>,  // props (edid is here as "edid" Vec<u8>)
);

type MutterLogicalMonitor = (
    i32, i32,   // x, y
    f64,        // scale
    u32,        // transform
    bool,       // primary
    Vec<(String, String, String, String)>,  // monitors assigned
    std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
);

pub struct MutterMonitorService {
    proxy: MutterDisplayConfigProxy<'static>,
}

impl MutterMonitorService {
    pub async fn new(conn: &Connection) -> Result<Self> {
        Ok(Self {
            proxy: MutterDisplayConfigProxy::new(conn).await?,
        })
    }
}

#[async_trait]
impl MonitorService for MutterMonitorService {
    async fn list_monitors(&self) -> Result<Vec<MonitorInfo>> {
        let (_serial, monitors, logical_monitors, _props) =
            self.proxy.get_current_state().await?;

        let mut out = Vec::with_capacity(monitors.len());
        for ((connector, _vendor, _product, _serial), modes, props) in monitors.iter() {
            let edid: Vec<u8> = props
                .get("edid")
                .and_then(|v| v.try_clone().ok())
                .and_then(|v| Vec::<u8>::try_from(v).ok())
                .unwrap_or_default();
            let monitor_key = compute_monitor_key(connector, &edid);

            // Pick the current (active) mode — the one flagged "is-current".
            let (width_px, height_px) = modes
                .iter()
                .find_map(|(_id, w, h, _rate, _scale, _supported, flags)| {
                    flags.get("is-current")
                        .and_then(|v| bool::try_from(v.try_clone().ok()?).ok())
                        .filter(|b| *b)
                        .map(|_| (*w as u32, *h as u32))
                })
                .unwrap_or((0, 0));

            let is_primary = logical_monitors.iter().any(|(_x, _y, _s, _t, primary, assigned, _)| {
                *primary && assigned.iter().any(|(c, _, _, _)| c == connector)
            });

            let name = props.get("display-name")
                .and_then(|v| v.try_clone().ok())
                .and_then(|v| String::try_from(v).ok())
                .unwrap_or_else(|| connector.clone());

            out.push(MonitorInfo {
                monitor_key,
                connector: connector.clone(),
                name,
                width_px,
                height_px,
                is_primary,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_with_edid_is_connector_plus_hex8() {
        let edid = b"monitor edid payload";
        let k = compute_monitor_key("DP-1", edid);
        assert!(k.starts_with("DP-1:"));
        assert_eq!(k.split(':').nth(1).unwrap().len(), 8);
    }

    #[test]
    fn key_without_edid_falls_back() {
        assert_eq!(compute_monitor_key("HDMI-2", &[]), "HDMI-2:no-edid");
    }

    #[test]
    fn identical_edid_produces_identical_key() {
        let edid = b"AAAA";
        assert_eq!(compute_monitor_key("DP-1", edid), compute_monitor_key("DP-1", edid));
    }

    #[test]
    fn different_edid_produces_different_key() {
        assert_ne!(compute_monitor_key("DP-1", b"A"), compute_monitor_key("DP-1", b"B"));
    }
}

// ---- Hot-plug watcher ----

use crate::db::{monitors as db_monitors, Database};
use futures_util::StreamExt;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use zbus::{MatchRule, MessageStream, MessageType};

/// Watch for monitor reconfigurations. Auto-assigns the default layout to
/// newly-seen monitors and fires `notify_tx` on each reconcile so the D-Bus
/// interface can emit the user-facing `MonitorsChanged` signal.
pub async fn spawn_hotplug_watcher(
    conn: Connection,
    db: Arc<Mutex<Database>>,
    monitor_svc: Arc<dyn MonitorService>,
    notify_tx: mpsc::UnboundedSender<()>,
) -> Result<tokio::task::JoinHandle<()>> {
    let rule = MatchRule::builder()
        .msg_type(MessageType::Signal)
        .interface("org.gnome.Mutter.DisplayConfig")?
        .member("MonitorsChanged")?
        .build();

    let dbus_proxy = zbus::fdo::DBusProxy::new(&conn).await?;
    dbus_proxy.add_match_rule(rule).await
        .map_err(|e| zbus::Error::FDO(Box::new(e)))?;

    let mut stream = MessageStream::from(&conn);
    let handle = tokio::spawn(async move {
        while let Some(Ok(msg)) = stream.next().await {
            let hdr = msg.header();
            let Some(member) = hdr.member() else { continue };
            if member.as_str() != "MonitorsChanged" {
                continue;
            }

            if let Err(e) = reconcile_monitors(&db, &monitor_svc).await {
                tracing::warn!("monitor reconcile failed: {e}");
                continue;
            }
            // Best-effort — if nobody's listening, just drop the event.
            let _ = notify_tx.send(());
        }
    });
    Ok(handle)
}

async fn reconcile_monitors(
    db: &Arc<Mutex<Database>>,
    monitor_svc: &Arc<dyn MonitorService>,
) -> Result<()> {
    let monitors = monitor_svc.list_monitors().await?;
    let db = db.lock().await;
    let default_id: i64 = db.conn.query_row(
        "SELECT id FROM layouts WHERE name = 'Two Columns (50/50)' AND is_preset = 1",
        [],
        |r| r.get(0),
    )?;
    for m in monitors {
        if db_monitors::get_assigned_layout_id(&db, &m.monitor_key)?.is_none() {
            db_monitors::assign_layout(&db, &m.monitor_key, default_id)?;
            tracing::info!(monitor_key = %m.monitor_key, "new monitor → assigned default layout");
        }
    }
    Ok(())
}
