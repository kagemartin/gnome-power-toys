// crates/gnome-zones-daemon/src/window/shim.rs
use crate::error::{Error, Result};
use crate::math::PixelRect;
use crate::window::WindowMover;
use async_trait::async_trait;
use zbus::{proxy, Connection};

#[proxy(
    interface        = "org.gnome.Shell.Extensions.GnomeZonesMover",
    default_service  = "org.gnome.Shell",
    default_path     = "/org/gnome/Shell/Extensions/GnomeZonesMover"
)]
trait GnomeZonesMover {
    fn move_resize_window(&self, window_id: u64, x: i32, y: i32, w: i32, h: i32) -> zbus::Result<bool>;
    fn get_focused_window_id(&self) -> zbus::Result<u64>;
    fn list_windows_in_rect(&self, x: i32, y: i32, w: i32, h: i32) -> zbus::Result<Vec<u64>>;
    fn activate_window(&self, window_id: u64) -> zbus::Result<()>;
}

pub struct ShimMover {
    proxy: GnomeZonesMoverProxy<'static>,
}

impl ShimMover {
    pub async fn new(conn: &Connection) -> Result<Self> {
        Ok(Self { proxy: GnomeZonesMoverProxy::new(conn).await? })
    }
}

#[async_trait]
impl WindowMover for ShimMover {
    async fn focused_window_id(&self) -> Result<u64> {
        let id = self.proxy.get_focused_window_id().await?;
        if id == 0 {
            return Err(Error::NoFocusedWindow);
        }
        Ok(id)
    }

    async fn move_resize(&self, window_id: u64, rect: PixelRect) -> Result<()> {
        let ok = self.proxy.move_resize_window(window_id, rect.x, rect.y, rect.w, rect.h).await?;
        if !ok {
            return Err(Error::Compositor(format!("mover rejected move_resize for {window_id}")));
        }
        Ok(())
    }

    async fn windows_in_rect(&self, rect: PixelRect) -> Result<Vec<u64>> {
        Ok(self.proxy.list_windows_in_rect(rect.x, rect.y, rect.w, rect.h).await?)
    }

    async fn activate(&self, window_id: u64) -> Result<()> {
        Ok(self.proxy.activate_window(window_id).await?)
    }
}
