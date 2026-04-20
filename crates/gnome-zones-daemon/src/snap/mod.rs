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
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// After the activator overlay is shown, `SnapFocusedToZone` calls within
/// this window target the window that was focused *before* the overlay came
/// up. Matches the activator's auto-dismiss timeout (3 s) with a small
/// safety margin so clicks that land close to the timeout still work.
const ACTIVATOR_FOCUS_TTL: Duration = Duration::from_millis(3500);

pub struct SnapEngine {
    pub(crate) db: Arc<Mutex<Database>>,
    pub(crate) monitor_svc: Arc<dyn MonitorService>,
    pub(crate) mover: Arc<dyn WindowMover>,
    pub(crate) states: Arc<WindowStateMap>,
    /// Pre-overlay focused-window id captured on `ShowActivator`.
    /// Consumed by the next `snap_focused_to_zone` call within
    /// [`ACTIVATOR_FOCUS_TTL`] so the UI overlay (which steals focus on X11)
    /// doesn't become the snap target.
    activator_focus: Arc<Mutex<Option<(u64, Instant)>>>,
}

impl SnapEngine {
    pub fn new(
        db: Arc<Mutex<Database>>,
        monitor_svc: Arc<dyn MonitorService>,
        mover: Arc<dyn WindowMover>,
        states: Arc<WindowStateMap>,
    ) -> Self {
        Self {
            db,
            monitor_svc,
            mover,
            states,
            activator_focus: Arc::new(Mutex::new(None)),
        }
    }

    /// Capture the currently-focused window id so that the next
    /// `snap_focused_to_zone` call (invoked from the activator overlay)
    /// targets *that* window instead of the overlay itself.
    pub async fn stash_focus_for_activator(&self) -> Result<()> {
        let win_id = self.mover.focused_window_id().await?;
        *self.activator_focus.lock().await = Some((win_id, Instant::now()));
        Ok(())
    }

    /// Pop the cached pre-overlay focused window id if it's still fresh.
    /// Always clears the cache on read (single-use; subsequent snaps
    /// without a fresh `ShowActivator` fall back to the live focused id).
    async fn take_activator_focus(&self) -> Option<u64> {
        let mut guard = self.activator_focus.lock().await;
        let Some((id, ts)) = *guard else { return None; };
        *guard = None;
        if ts.elapsed() <= ACTIVATOR_FOCUS_TTL {
            Some(id)
        } else {
            None
        }
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

    /// Fetch the layout assigned to a monitor. If none assigned, falls back
    /// to the `Duet` preset (50/50 split) and persists the assignment for
    /// next time.
    pub(crate) async fn active_layout_for(&self, monitor_key: &str) -> Result<Layout> {
        let db = self.db.lock().await;
        let layout_id = match monitors::get_assigned_layout_id(&db, monitor_key)? {
            Some(id) => id,
            None => {
                let fallback: i64 = db.conn.query_row(
                    "SELECT id FROM layouts WHERE name = 'Duet' AND is_preset = 1",
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

        // Prefer the pre-activator-overlay focused window if one was stashed
        // by `ShowActivator` within the TTL; otherwise query live focus.
        let win_id = match self.take_activator_focus().await {
            Some(id) => id,
            None => self.mover.focused_window_id().await?,
        };
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
            crate::db::settings::get_int(&db, "gap_px", 0)? as i32
        };
        // Project onto the work area (monitor minus panels/docks) and shift by
        // its origin so snapping respects struts like the top bar.
        let wa = self.mover.focused_work_area().await?;
        let projected = math::project_rect(&union_frac, wa.w, wa.h);
        let target_px = math::deflate(
            PixelRect {
                x: wa.x + projected.x,
                y: wa.y + projected.y,
                w: projected.w,
                h: projected.h,
            },
            gap,
        );

        // Persist the window's real pre-snap frame rect so Super+Down
        // (RestoreFocusedWindow) can put it back later. Only on the first
        // snap — subsequent snaps/spans shouldn't overwrite it.
        if current_state.zones.is_empty() {
            match self.mover.frame_rect(win_id).await {
                Ok(pre) => self.states.ensure_pre_snap(win_id, pre).await,
                Err(e) => tracing::warn!(
                    error = %e,
                    win_id,
                    "snap: failed to capture pre-snap frame rect (restore-to-original will be unavailable)"
                ),
            }
        }

        // Execute the move-resize.
        self.mover.move_resize(win_id, target_px).await?;
        self.states.set_zones(win_id, zone_set).await;
        Ok(())
    }

    /// Restore the focused window to its pre-snap rect (if the daemon is
    /// tracking one for it), otherwise fall back to the compositor's native
    /// unmaximize behaviour. Called by the `RestoreFocusedWindow` D-Bus
    /// method, which the `gnome-zones-mover` extension binds to `Super+Down`.
    pub async fn restore_focused_window(&self) -> Result<()> {
        let paused = {
            let db = self.db.lock().await;
            crate::db::settings::get_bool(&db, "paused", false)?
        };
        if paused {
            tracing::debug!("paused — ignoring restore");
            return Ok(());
        }
        let win_id = self.mover.focused_window_id().await?;
        let state = self.states.get(win_id).await;

        if !state.zones.is_empty() {
            if let Some(pre) = state.pre_snap {
                // Snapped and we have a real pre-snap rect — put the
                // window back and clear tracked zone state.
                self.mover.move_resize(win_id, pre).await?;
                self.states.forget(win_id).await;
                return Ok(());
            }
            // Snapped but no pre-snap captured (e.g. pre-migration window):
            // fall through to native unmaximize so something useful happens.
        }
        self.mover.unmaximize(win_id).await?;
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

        // Use union of the window's zones as the cycling rect, projected onto
        // the focused window's work area.
        let zones_refs: Vec<&_> = state.zones.iter()
            .filter_map(|i| layout.zone(*i))
            .collect();
        if zones_refs.is_empty() { return Ok(()); }
        let union_frac = math::bounding_rect(&zones_refs);
        let wa = self.mover.focused_work_area().await?;
        let projected = math::project_rect(&union_frac, wa.w, wa.h);
        let rect_px = PixelRect {
            x: wa.x + projected.x,
            y: wa.y + projected.y,
            w: projected.w,
            h: projected.h,
        };

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

    pub struct MockMover {
        pub focused: StdMutex<u64>,
        pub moves: StdMutex<Vec<(u64, PixelRect)>>,
        pub activations: StdMutex<Vec<u64>>,
        pub windows_in_rect_result: StdMutex<Vec<u64>>,
        pub work_area: StdMutex<PixelRect>,
        pub frame_rect_result: StdMutex<PixelRect>,
        pub unmaximized: StdMutex<Vec<u64>>,
    }

    impl Default for MockMover {
        fn default() -> Self {
            Self {
                focused: StdMutex::new(0),
                moves: StdMutex::new(Vec::new()),
                activations: StdMutex::new(Vec::new()),
                windows_in_rect_result: StdMutex::new(Vec::new()),
                // Matches the tests that assume a 1920×1080 full-monitor work area.
                work_area: StdMutex::new(PixelRect { x: 0, y: 0, w: 1920, h: 1080 }),
                // Default pre-snap frame: an off-center 800×600 window.
                frame_rect_result: StdMutex::new(PixelRect { x: 200, y: 150, w: 800, h: 600 }),
                unmaximized: StdMutex::new(Vec::new()),
            }
        }
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
        async fn focused_work_area(&self) -> Result<PixelRect> {
            Ok(*self.work_area.lock().unwrap())
        }
        async fn frame_rect(&self, _window_id: u64) -> Result<PixelRect> {
            Ok(*self.frame_rect_result.lock().unwrap())
        }
        async fn unmaximize(&self, window_id: u64) -> Result<()> {
            self.unmaximized.lock().unwrap().push(window_id);
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
        // Duet (50/50) zone 1 on 1920×1080 work area with 0px gap → (0, 0, 960, 1080)
        assert_eq!(rect.x, 0);
        assert_eq!(rect.y, 0);
        assert!((rect.w - 960).abs() <= 1);
        assert!((rect.h - 1080).abs() <= 1);
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
        assert!((rect.w - 1920).abs() <= 1);  // full work-area width, 0 gap default
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
        assert_eq!(state.zones, vec![2]);  // Duet has 2 zones
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
