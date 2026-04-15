// crates/gnome-zones-daemon/src/dbus/interface.rs
use crate::db::{layouts, monitors as db_monitors, settings, Database};
use crate::dbus::types::*;
use crate::model::{IterateDir, ZoneRect};
use crate::monitors::MonitorService;
use crate::snap::SnapEngine;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use zbus::{fdo, interface, SignalContext};

pub struct ZonesInterface {
    pub db: Arc<Mutex<Database>>,
    pub snap: Arc<SnapEngine>,
    pub monitor_svc: Arc<dyn MonitorService>,
}

fn fdo_error(e: impl std::fmt::Display) -> fdo::Error {
    fdo::Error::Failed(e.to_string())
}

#[interface(name = "org.gnome.Zones")]
impl ZonesInterface {
    // ---- Layout methods ----

    async fn list_layouts(&self) -> fdo::Result<Vec<LayoutSummaryWire>> {
        let db = self.db.lock().await;
        let v = layouts::list_layouts(&db).map_err(fdo_error)?;
        Ok(v.iter().map(LayoutSummaryWire::from).collect())
    }

    async fn get_layout(&self, id: i64) -> fdo::Result<LayoutWire> {
        let db = self.db.lock().await;
        let layout = layouts::get_layout(&db, id)
            .map_err(fdo_error)?
            .ok_or_else(|| fdo::Error::Failed(format!("no layout id={id}")))?;
        Ok(LayoutWire::from(&layout))
    }

    async fn create_layout(
        &self,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
        name: &str,
        zones: Vec<ZoneWire>,
    ) -> fdo::Result<i64> {
        let rects: Vec<ZoneRect> = zones.into_iter().map(Into::into).collect();
        let id = {
            let mut db = self.db.lock().await;
            layouts::create_layout(&mut db, name, false, &rects).map_err(fdo_error)?
        };
        Self::layouts_changed(&ctx).await.ok();
        Ok(id)
    }

    async fn update_layout(
        &self,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
        id: i64,
        name: &str,
        zones: Vec<ZoneWire>,
    ) -> fdo::Result<()> {
        let rects: Vec<ZoneRect> = zones.into_iter().map(Into::into).collect();
        {
            let mut db = self.db.lock().await;
            layouts::update_layout(&mut db, id, name, &rects).map_err(fdo_error)?;
        }
        Self::layouts_changed(&ctx).await.ok();
        Ok(())
    }

    async fn delete_layout(
        &self,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
        id: i64,
    ) -> fdo::Result<()> {
        {
            let mut db = self.db.lock().await;
            layouts::delete_layout(&mut db, id).map_err(fdo_error)?;
        }
        Self::layouts_changed(&ctx).await.ok();
        Ok(())
    }

    // ---- Monitor methods ----

    async fn list_monitors(&self) -> fdo::Result<Vec<MonitorInfoWire>> {
        let v = self.monitor_svc.list_monitors().await.map_err(fdo_error)?;
        Ok(v.iter().map(MonitorInfoWire::from).collect())
    }

    async fn assign_layout(
        &self,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
        monitor_key: &str,
        layout_id: i64,
    ) -> fdo::Result<()> {
        {
            let db = self.db.lock().await;
            db_monitors::assign_layout(&db, monitor_key, layout_id).map_err(fdo_error)?;
        }
        Self::layout_assigned(&ctx, monitor_key.to_string(), layout_id).await.ok();
        Ok(())
    }

    async fn get_active_layout(&self, monitor_key: &str) -> fdo::Result<LayoutWire> {
        let layout = self.snap.active_layout_for(monitor_key).await.map_err(fdo_error)?;
        Ok(LayoutWire::from(&layout))
    }

    // ---- Settings methods ----

    async fn get_settings(&self) -> fdo::Result<HashMap<String, String>> {
        let db = self.db.lock().await;
        settings::get_all_settings(&db).map_err(fdo_error)
    }

    async fn set_setting(
        &self,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
        key: &str,
        value: &str,
    ) -> fdo::Result<()> {
        {
            let db = self.db.lock().await;
            settings::set_setting(&db, key, value).map_err(fdo_error)?;
        }
        if key == "paused" {
            let paused = value == "1" || value == "true";
            Self::paused_changed(&ctx, paused).await.ok();
        }
        Ok(())
    }

    // ---- Action methods ----

    async fn snap_focused_to_zone(&self, zone_index: u32, span: bool) -> fdo::Result<()> {
        self.snap.snap_focused_to_zone(zone_index, span).await.map_err(fdo_error)
    }

    async fn iterate_focused_zone(&self, direction: &str) -> fdo::Result<()> {
        let dir: IterateDir = direction.parse().map_err(|e: String| fdo::Error::Failed(e))?;
        self.snap.iterate_focused_zone(dir).await.map_err(fdo_error)
    }

    async fn cycle_focus_in_zone(&self, direction: i32) -> fdo::Result<()> {
        self.snap.cycle_focus_in_zone(direction).await.map_err(fdo_error)
    }

    async fn show_activator(
        &self,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
    ) -> fdo::Result<()> {
        // UI isn't implemented yet (Plan 2). Emit the signal anyway so the
        // wire surface is complete.
        let primary_key = self
            .monitor_svc.list_monitors().await.map_err(fdo_error)?
            .into_iter()
            .find(|m| m.is_primary)
            .map(|m| m.monitor_key)
            .unwrap_or_default();
        Self::activator_requested(&ctx, primary_key).await.ok();
        Ok(())
    }

    async fn toggle_paused(
        &self,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
    ) -> fdo::Result<()> {
        let new_value = {
            let db = self.db.lock().await;
            let current = settings::get_bool(&db, "paused", false).map_err(fdo_error)?;
            let next = !current;
            settings::set_setting(&db, "paused", if next { "true" } else { "false" })
                .map_err(fdo_error)?;
            next
        };
        Self::paused_changed(&ctx, new_value).await.ok();
        Ok(())
    }

    // ---- Signals ----

    #[zbus(signal)]
    async fn layouts_changed(ctx: &SignalContext<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn layout_assigned(ctx: &SignalContext<'_>, monitor_key: String, layout_id: i64) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn monitors_changed(ctx: &SignalContext<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn paused_changed(ctx: &SignalContext<'_>, paused: bool) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn activator_requested(ctx: &SignalContext<'_>, monitor_key: String) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn editor_requested(ctx: &SignalContext<'_>, monitor_key: String) -> zbus::Result<()>;
}
