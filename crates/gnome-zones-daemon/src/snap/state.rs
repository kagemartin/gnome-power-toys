// crates/gnome-zones-daemon/src/snap/state.rs
use crate::math::PixelRect;
use std::collections::HashMap;
use tokio::sync::Mutex;

#[derive(Debug, Default, Clone)]
pub struct WindowState {
    /// Rect the window had right before its first snap — used for v2 unsnap.
    pub pre_snap: Option<PixelRect>,
    /// Zone indices the window is currently snapped across (empty = not snapped).
    pub zones: Vec<u32>,
}

#[derive(Default)]
pub struct WindowStateMap(Mutex<HashMap<u64, WindowState>>);

impl WindowStateMap {
    pub fn new() -> Self { Self::default() }

    pub async fn get(&self, id: u64) -> WindowState {
        self.0.lock().await.get(&id).cloned().unwrap_or_default()
    }

    pub async fn set_zones(&self, id: u64, zones: Vec<u32>) {
        self.0.lock().await.entry(id).or_default().zones = zones;
    }

    pub async fn ensure_pre_snap(&self, id: u64, rect: PixelRect) {
        let mut map = self.0.lock().await;
        let entry = map.entry(id).or_default();
        if entry.pre_snap.is_none() {
            entry.pre_snap = Some(rect);
        }
    }

    pub async fn forget(&self, id: u64) {
        self.0.lock().await.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_on_unknown_id_is_default() {
        let m = WindowStateMap::new();
        let s = m.get(42).await;
        assert!(s.zones.is_empty());
        assert!(s.pre_snap.is_none());
    }

    #[tokio::test]
    async fn set_and_retrieve_zones() {
        let m = WindowStateMap::new();
        m.set_zones(7, vec![2, 3]).await;
        let s = m.get(7).await;
        assert_eq!(s.zones, vec![2, 3]);
    }

    #[tokio::test]
    async fn ensure_pre_snap_only_sets_once() {
        let m = WindowStateMap::new();
        let r1 = PixelRect { x: 0, y: 0, w: 100, h: 100 };
        let r2 = PixelRect { x: 50, y: 50, w: 100, h: 100 };
        m.ensure_pre_snap(7, r1).await;
        m.ensure_pre_snap(7, r2).await;
        assert_eq!(m.get(7).await.pre_snap, Some(r1));
    }

    #[tokio::test]
    async fn forget_removes_entry() {
        let m = WindowStateMap::new();
        m.set_zones(7, vec![1]).await;
        m.forget(7).await;
        assert!(m.get(7).await.zones.is_empty());
    }
}
