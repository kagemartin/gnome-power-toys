// crates/gnome-zones-daemon/src/window/mod.rs
use crate::error::Result;
use crate::math::PixelRect;
use async_trait::async_trait;

pub mod mutter;
pub mod shim;

/// The daemon uses this trait for every interaction with windows — move/resize,
/// focus resolution, window lookup by rect. Production wires the `ShimMover`
/// (our extension) as primary with `MutterMover` as fallback; tests inject a mock.
#[async_trait]
pub trait WindowMover: Send + Sync {
    async fn focused_window_id(&self) -> Result<u64>;
    async fn move_resize(&self, window_id: u64, rect: PixelRect) -> Result<()>;
    async fn windows_in_rect(&self, rect: PixelRect) -> Result<Vec<u64>>;
    async fn activate(&self, window_id: u64) -> Result<()>;
    /// Work area (monitor rect minus struts like the top bar or dock) for the
    /// monitor containing the focused window.
    async fn focused_work_area(&self) -> Result<PixelRect>;
    /// Frame rect (decorations-inclusive) of an arbitrary window. Used to
    /// capture the real pre-snap geometry before move/resize overwrites it.
    async fn frame_rect(&self, window_id: u64) -> Result<PixelRect>;
    /// Unmaximize a window via the compositor's native unmaximize path (no-op
    /// if the window isn't currently maximized). Used as the fallback branch
    /// of `SnapEngine::restore_focused_window` when there's no tracked snap.
    async fn unmaximize(&self, window_id: u64) -> Result<()>;
}
