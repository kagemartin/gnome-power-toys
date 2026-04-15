// crates/gnome-zones-daemon/src/dbus/mod.rs
pub mod interface;
pub mod types;

use crate::db::Database;
use crate::error::Result;
use crate::monitors::MonitorService;
use crate::snap::SnapEngine;
use std::sync::Arc;
use tokio::sync::Mutex;
use zbus::ConnectionBuilder;

pub struct ServiceHandle {
    pub connection: zbus::Connection,
}

pub async fn run_service(
    db: Arc<Mutex<Database>>,
    snap: Arc<SnapEngine>,
    monitor_svc: Arc<dyn MonitorService>,
) -> Result<ServiceHandle> {
    let iface = interface::ZonesInterface {
        db: db.clone(),
        snap: snap.clone(),
        monitor_svc: monitor_svc.clone(),
    };
    let connection = ConnectionBuilder::session()?
        .name("org.gnome.Zones")?
        .serve_at("/org/gnome/Zones", iface)?
        .build()
        .await?;
    Ok(ServiceHandle { connection })
}

use crate::dbus::interface::{emit_monitors_changed, ZonesInterface};

impl ServiceHandle {
    /// Emit `org.gnome.Zones.MonitorsChanged` from outside the interface.
    /// Used by the hot-plug watcher.
    pub async fn emit_monitors_changed(&self) -> Result<()> {
        let iface_ref: zbus::InterfaceRef<ZonesInterface> = self
            .connection
            .object_server()
            .interface("/org/gnome/Zones")
            .await?;
        emit_monitors_changed(iface_ref.signal_context()).await?;
        Ok(())
    }
}
