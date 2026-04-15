// crates/gnome-zones-daemon/src/window/mutter.rs
use crate::error::{Error, Result};
use crate::math::PixelRect;
use crate::window::WindowMover;
use async_trait::async_trait;
use zbus::{proxy, Connection};

#[proxy(
    interface       = "org.gnome.Shell.Introspect",
    default_service = "org.gnome.Shell",
    default_path    = "/org/gnome/Shell/Introspect"
)]
trait ShellIntrospect {
    fn get_windows(&self) -> zbus::Result<
        std::collections::HashMap<u64, std::collections::HashMap<String, zbus::zvariant::OwnedValue>>
    >;
}

pub struct MutterMover {
    introspect: ShellIntrospectProxy<'static>,
}

impl MutterMover {
    pub async fn new(conn: &Connection) -> Result<Self> {
        Ok(Self { introspect: ShellIntrospectProxy::new(conn).await? })
    }
}

#[async_trait]
impl WindowMover for MutterMover {
    async fn focused_window_id(&self) -> Result<u64> {
        let windows = self.introspect.get_windows().await?;
        for (id, props) in windows {
            if let Some(v) = props.get("has-focus") {
                if let Ok(b) = bool::try_from(v.try_clone()?) {
                    if b {
                        return Ok(id);
                    }
                }
            }
        }
        Err(Error::NoFocusedWindow)
    }

    async fn move_resize(&self, _window_id: u64, _rect: PixelRect) -> Result<()> {
        // No mainline D-Bus API exists to move other apps' windows without a
        // Shell extension. Surface a clear error so callers can fall back.
        Err(Error::Compositor(
            "move_resize unavailable without gnome-zones-mover shell extension".into()
        ))
    }

    async fn windows_in_rect(&self, rect: PixelRect) -> Result<Vec<u64>> {
        let windows = self.introspect.get_windows().await?;
        let mut out = Vec::new();
        let x1 = rect.x + rect.w;
        let y1 = rect.y + rect.h;
        for (id, props) in windows {
            let Some(v) = props.get("frame-rect") else { continue };
            // frame-rect is an (iiii) tuple
            let Ok(tuple) = <(i32, i32, i32, i32)>::try_from(v.try_clone()?) else { continue };
            let cx = tuple.0 + tuple.2 / 2;
            let cy = tuple.1 + tuple.3 / 2;
            if cx >= rect.x && cx < x1 && cy >= rect.y && cy < y1 {
                out.push(id);
            }
        }
        Ok(out)
    }

    async fn activate(&self, _window_id: u64) -> Result<()> {
        Err(Error::Compositor(
            "activate unavailable without gnome-zones-mover shell extension".into()
        ))
    }
}
