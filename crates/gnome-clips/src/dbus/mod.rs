pub mod proxy;

pub use proxy::{ClipDetail, ClipSummary, ClipsProxy};

use crate::error::Result;

pub async fn connect() -> Result<ClipsProxy<'static>> {
    let conn = zbus::Connection::session().await?;
    // Bind the proxy to 'static so it can be freely cloned into GLib closures.
    let proxy = ClipsProxy::new(&conn).await?;
    Ok(proxy)
}
