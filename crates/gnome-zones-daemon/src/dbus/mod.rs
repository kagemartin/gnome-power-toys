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
