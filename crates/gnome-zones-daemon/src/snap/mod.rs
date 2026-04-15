// crates/gnome-zones-daemon/src/snap/mod.rs
pub mod state;

use crate::db::{layouts, monitors, Database};
use crate::error::{Error, Result};
use crate::math::{self, PixelRect};
use crate::model::{Layout, MonitorInfo};
use crate::monitors::MonitorService;
use crate::snap::state::WindowStateMap;
use crate::window::WindowMover;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct SnapEngine {
    pub(crate) db: Arc<Mutex<Database>>,
    pub(crate) monitor_svc: Arc<dyn MonitorService>,
    pub(crate) mover: Arc<dyn WindowMover>,
    pub(crate) states: Arc<WindowStateMap>,
}

impl SnapEngine {
    pub fn new(
        db: Arc<Mutex<Database>>,
        monitor_svc: Arc<dyn MonitorService>,
        mover: Arc<dyn WindowMover>,
        states: Arc<WindowStateMap>,
    ) -> Self {
        Self { db, monitor_svc, mover, states }
    }

    /// Pick the monitor to act on. Prefers primary, falls back to the first in
    /// the list. v1 keyboard-snap scope: always pick one monitor regardless of
    /// window position (spec §5 "Multi-monitor in v1").
    pub(crate) async fn target_monitor(&self) -> Result<MonitorInfo> {
        let mut monitors = self.monitor_svc.list_monitors().await?;
        monitors.sort_by_key(|m| !m.is_primary);  // primary first
        monitors.into_iter().next()
            .ok_or_else(|| Error::Compositor("no monitors enumerated".into()))
    }

    /// Fetch the layout assigned to a monitor. If none assigned, falls back to
    /// "Two Columns (50/50)" and persists the assignment for next time.
    pub(crate) async fn active_layout_for(&self, monitor_key: &str) -> Result<Layout> {
        let db = self.db.lock().await;
        let layout_id = match monitors::get_assigned_layout_id(&db, monitor_key)? {
            Some(id) => id,
            None => {
                let fallback: i64 = db.conn.query_row(
                    "SELECT id FROM layouts WHERE name = 'Two Columns (50/50)' AND is_preset = 1",
                    [],
                    |r| r.get(0),
                )?;
                monitors::assign_layout(&db, monitor_key, fallback)?;
                fallback
            }
        };
        layouts::get_layout(&db, layout_id)?
            .ok_or_else(|| Error::NoLayoutForMonitor(monitor_key.into()))
    }

    /// Snap the focused window to zone `zone_index`.
    /// `span = true` adds the zone to the window's current span set;
    /// `span = false` replaces whatever set was there.
    pub async fn snap_focused_to_zone(&self, zone_index: u32, span: bool) -> Result<()> {
        // Respect pause.
        let paused = {
            let db = self.db.lock().await;
            crate::db::settings::get_bool(&db, "paused", false)?
        };
        if paused {
            tracing::debug!(zone_index, "paused — ignoring snap");
            return Ok(());
        }

        let win_id = self.mover.focused_window_id().await?;
        let monitor = self.target_monitor().await?;

        let layout = self.active_layout_for(&monitor.monitor_key).await?;
        let zone_count = layout.zones.len() as u32;
        if zone_index == 0 || zone_index > zone_count {
            return Err(Error::InvalidZoneIndex(zone_index, zone_count));
        }

        // Compute the target set of zone indices.
        let current_state = self.states.get(win_id).await;
        let mut zone_set: Vec<u32> = if span {
            current_state.zones.clone()
        } else {
            Vec::new()
        };
        if !zone_set.contains(&zone_index) {
            zone_set.push(zone_index);
        }
        zone_set.sort_unstable();
        zone_set.dedup();

        // Resolve the target pixel rect (union of the set, deflated by gap).
        let zones: Vec<&_> = zone_set
            .iter()
            .filter_map(|i| layout.zone(*i))
            .collect();
        let union_frac = math::bounding_rect(&zones);

        let gap = {
            let db = self.db.lock().await;
            crate::db::settings::get_int(&db, "gap_px", 8)? as i32
        };
        let target_px = math::deflate(
            math::project_rect(&union_frac, monitor.width_px as i32, monitor.height_px as i32),
            gap,
        );

        // Persist pre-snap rect on first snap.
        if current_state.zones.is_empty() {
            self.states.ensure_pre_snap(win_id, target_px).await;
        }

        // Execute the move-resize.
        self.mover.move_resize(win_id, target_px).await?;
        self.states.set_zones(win_id, zone_set).await;
        Ok(())
    }

    pub async fn iterate_focused_zone(&self, dir: crate::model::IterateDir) -> Result<()> {
        let paused = {
            let db = self.db.lock().await;
            crate::db::settings::get_bool(&db, "paused", false)?
        };
        if paused { return Ok(()); }

        let win_id = self.mover.focused_window_id().await?;
        let monitor = self.target_monitor().await?;

        let layout = self.active_layout_for(&monitor.monitor_key).await?;
        let zone_count = layout.zones.len() as u32;
        if zone_count == 0 { return Ok(()); }

        let state = self.states.get(win_id).await;
        // Treat unsnapped or multi-zone-spanning windows as index 0.
        let current_index = if state.zones.len() == 1 { state.zones[0] } else { 0 };
        let target = math::iterate_index(current_index, zone_count, dir);

        self.snap_focused_to_zone(target, false).await
    }

    pub async fn cycle_focus_in_zone(&self, direction: i32) -> Result<()> {
        let paused = {
            let db = self.db.lock().await;
            crate::db::settings::get_bool(&db, "paused", false)?
        };
        if paused { return Ok(()); }

        let focused_id = self.mover.focused_window_id().await?;
        let state = self.states.get(focused_id).await;
        if state.zones.is_empty() {
            return Ok(());  // window not snapped — nothing to cycle.
        }

        let monitor = self.target_monitor().await?;
        let layout = self.active_layout_for(&monitor.monitor_key).await?;

        // Use union of the window's zones as the cycling rect.
        let zones_refs: Vec<&_> = state.zones.iter()
            .filter_map(|i| layout.zone(*i))
            .collect();
        if zones_refs.is_empty() { return Ok(()); }
        let union_frac = math::bounding_rect(&zones_refs);
        let rect_px = math::project_rect(
            &union_frac,
            monitor.width_px as i32,
            monitor.height_px as i32,
        );

        let ids = self.mover.windows_in_rect(rect_px).await?;
        if ids.len() < 2 { return Ok(()); }

        let pos = ids.iter().position(|&w| w == focused_id).unwrap_or(0);
        let next_pos = if direction >= 0 {
            (pos + 1) % ids.len()
        } else {
            (pos + ids.len() - 1) % ids.len()
        };
        self.mover.activate(ids[next_pos]).await?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;
    use crate::db::Database;
    use crate::model::MonitorInfo;
    use async_trait::async_trait;
    use std::sync::Mutex as StdMutex;
    use tempfile::NamedTempFile;

    pub struct MockMonitor {
        pub monitors: Vec<MonitorInfo>,
    }

    #[async_trait]
    impl MonitorService for MockMonitor {
        async fn list_monitors(&self) -> Result<Vec<MonitorInfo>> {
            Ok(self.monitors.clone())
        }
    }

    #[derive(Default)]
    pub struct MockMover {
        pub focused: StdMutex<u64>,
        pub moves: StdMutex<Vec<(u64, PixelRect)>>,
        pub activations: StdMutex<Vec<u64>>,
        pub windows_in_rect_result: StdMutex<Vec<u64>>,
    }

    #[async_trait]
    impl WindowMover for MockMover {
        async fn focused_window_id(&self) -> Result<u64> {
            let id = *self.focused.lock().unwrap();
            if id == 0 { Err(Error::NoFocusedWindow) } else { Ok(id) }
        }
        async fn move_resize(&self, window_id: u64, rect: PixelRect) -> Result<()> {
            self.moves.lock().unwrap().push((window_id, rect));
            Ok(())
        }
        async fn windows_in_rect(&self, _rect: PixelRect) -> Result<Vec<u64>> {
            Ok(self.windows_in_rect_result.lock().unwrap().clone())
        }
        async fn activate(&self, window_id: u64) -> Result<()> {
            self.activations.lock().unwrap().push(window_id);
            Ok(())
        }
    }

    pub fn primary_monitor(key: &str, w: u32, h: u32) -> MonitorInfo {
        MonitorInfo {
            monitor_key: key.into(),
            connector: "DP-1".into(),
            name: "Test".into(),
            width_px: w,
            height_px: h,
            is_primary: true,
        }
    }

    /// Build an engine around a pre-wrapped mover so tests can assert on the
    /// mover's internal state directly.
    pub fn temp_engine_with_mover(
        mover: Arc<MockMover>,
        monitors_vec: Vec<MonitorInfo>,
    ) -> SnapEngine {
        let f = NamedTempFile::new().unwrap();
        let mut db = Database::open(f.path()).unwrap();
        crate::presets::seed(&mut db).unwrap();
        let db = Arc::new(Mutex::new(db));
        SnapEngine::new(
            db,
            Arc::new(MockMonitor { monitors: monitors_vec }),
            mover,  // Arc<MockMover> coerces to Arc<dyn WindowMover>
            Arc::new(WindowStateMap::new()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::testutil::*;

    #[tokio::test]
    async fn snap_focused_moves_window_to_zone_1() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 42;
        let engine = temp_engine_with_mover(
            mover.clone(),
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        engine.snap_focused_to_zone(1, false).await.unwrap();

        let moves = mover.moves.lock().unwrap().clone();
        assert_eq!(moves.len(), 1);
        let (id, rect) = moves[0];
        assert_eq!(id, 42);
        // "Two Columns (50/50)" zone 1 on 1920×1080 with 8px gap → x=8,y=8,w≈944,h≈1064
        assert_eq!(rect.x, 8);
        assert_eq!(rect.y, 8);
        assert!((rect.w - 944).abs() <= 1);
        assert!((rect.h - 1064).abs() <= 1);
    }

    #[tokio::test]
    async fn snap_respects_paused_setting() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 42;
        let engine = temp_engine_with_mover(
            mover.clone(),
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        {
            let db = engine.db.lock().await;
            crate::db::settings::set_setting(&db, "paused", "true").unwrap();
        }
        engine.snap_focused_to_zone(1, false).await.unwrap();
        assert!(mover.moves.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn snap_invalid_zone_index_errors() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 42;
        let engine = temp_engine_with_mover(
            mover,
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        let err = engine.snap_focused_to_zone(99, false).await.unwrap_err();
        assert!(matches!(err, Error::InvalidZoneIndex(99, 2)));
    }

    #[tokio::test]
    async fn span_adds_zone_to_existing_set() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 42;
        let engine = temp_engine_with_mover(
            mover.clone(),
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        // First, snap to zone 1 only.
        engine.snap_focused_to_zone(1, false).await.unwrap();
        // Then span into zone 2.
        engine.snap_focused_to_zone(2, true).await.unwrap();

        let state = engine.states.get(42).await;
        assert_eq!(state.zones, vec![1, 2]);

        let moves = mover.moves.lock().unwrap().clone();
        assert_eq!(moves.len(), 2);
        // Second move's rect should span both halves.
        let (_, rect) = moves[1];
        assert!((rect.w - (1920 - 16)).abs() <= 1);  // full width minus 8px gap on each side
    }

    #[tokio::test]
    async fn non_span_replaces_set() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 42;
        let engine = temp_engine_with_mover(
            mover,
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        engine.snap_focused_to_zone(1, false).await.unwrap();
        engine.snap_focused_to_zone(2, false).await.unwrap();
        let state = engine.states.get(42).await;
        assert_eq!(state.zones, vec![2]);
    }

    #[tokio::test]
    async fn iterate_next_advances_to_zone_2() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 42;
        let engine = temp_engine_with_mover(
            mover,
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        engine.snap_focused_to_zone(1, false).await.unwrap();
        engine.iterate_focused_zone(crate::model::IterateDir::Next).await.unwrap();
        let state = engine.states.get(42).await;
        assert_eq!(state.zones, vec![2]);
    }

    #[tokio::test]
    async fn iterate_next_on_unsnapped_lands_on_1() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 42;
        let engine = temp_engine_with_mover(
            mover,
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        engine.iterate_focused_zone(crate::model::IterateDir::Next).await.unwrap();
        let state = engine.states.get(42).await;
        assert_eq!(state.zones, vec![1]);
    }

    #[tokio::test]
    async fn iterate_prev_on_unsnapped_lands_on_last() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 42;
        let engine = temp_engine_with_mover(
            mover,
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        engine.iterate_focused_zone(crate::model::IterateDir::Prev).await.unwrap();
        let state = engine.states.get(42).await;
        assert_eq!(state.zones, vec![2]);  // "Two Columns (50/50)" has 2 zones
    }

    #[tokio::test]
    async fn iterate_next_wraps_around() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 42;
        let engine = temp_engine_with_mover(
            mover,
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        engine.snap_focused_to_zone(2, false).await.unwrap();
        engine.iterate_focused_zone(crate::model::IterateDir::Next).await.unwrap();
        let state = engine.states.get(42).await;
        assert_eq!(state.zones, vec![1]);
    }

    #[tokio::test]
    async fn cycle_activates_next_in_rect() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 100;
        *mover.windows_in_rect_result.lock().unwrap() = vec![100, 200, 300];
        let engine = temp_engine_with_mover(
            mover.clone(),
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        engine.snap_focused_to_zone(1, false).await.unwrap();
        engine.cycle_focus_in_zone(1).await.unwrap();
        assert_eq!(mover.activations.lock().unwrap().clone(), vec![200]);
    }

    #[tokio::test]
    async fn cycle_prev_wraps_to_last() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 100;
        *mover.windows_in_rect_result.lock().unwrap() = vec![100, 200, 300];
        let engine = temp_engine_with_mover(
            mover.clone(),
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        engine.snap_focused_to_zone(1, false).await.unwrap();
        engine.cycle_focus_in_zone(-1).await.unwrap();
        assert_eq!(mover.activations.lock().unwrap().clone(), vec![300]);
    }

    #[tokio::test]
    async fn cycle_unsnapped_is_noop() {
        let mover = Arc::new(MockMover::default());
        *mover.focused.lock().unwrap() = 100;
        *mover.windows_in_rect_result.lock().unwrap() = vec![100, 200];
        let engine = temp_engine_with_mover(
            mover.clone(),
            vec![primary_monitor("DP-1:test", 1920, 1080)],
        );
        engine.cycle_focus_in_zone(1).await.unwrap();
        assert!(mover.activations.lock().unwrap().is_empty());
    }
}
