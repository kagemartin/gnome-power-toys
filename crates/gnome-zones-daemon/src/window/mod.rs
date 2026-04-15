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
}
