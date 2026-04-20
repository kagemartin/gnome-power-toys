pub mod proxy;

use crate::error::Result;
pub use proxy::{LayoutSummaryWire, LayoutWire, MonitorInfoWire, ZoneWire, ZonesProxy};

pub async fn connect() -> Result<ZonesProxy<'static>> {
    let conn = zbus::Connection::session().await?;
    let proxy = ZonesProxy::new(&conn).await?;
    Ok(proxy)
}
