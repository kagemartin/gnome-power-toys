use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zbus::proxy;
use zbus::zvariant::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ZoneWire {
    pub zone_index: u32,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct LayoutSummaryWire {
    pub id: i64,
    pub name: String,
    pub is_preset: bool,
    pub zone_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct LayoutWire {
    pub id: i64,
    pub name: String,
    pub is_preset: bool,
    pub zones: Vec<ZoneWire>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct MonitorInfoWire {
    pub monitor_key: String,
    pub connector: String,
    pub name: String,
    pub width_px: u32,
    pub height_px: u32,
    pub is_primary: bool,
}

#[proxy(
    interface = "org.gnome.Zones",
    default_service = "org.gnome.Zones",
    default_path = "/org/gnome/Zones"
)]
pub trait Zones {
    async fn list_layouts(&self) -> zbus::Result<Vec<LayoutSummaryWire>>;
    async fn get_layout(&self, id: i64) -> zbus::Result<LayoutWire>;
    async fn create_layout(&self, name: &str, zones: Vec<ZoneWire>) -> zbus::Result<i64>;
    async fn update_layout(&self, id: i64, name: &str, zones: Vec<ZoneWire>) -> zbus::Result<()>;
    async fn delete_layout(&self, id: i64) -> zbus::Result<()>;

    async fn list_monitors(&self) -> zbus::Result<Vec<MonitorInfoWire>>;
    async fn assign_layout(&self, monitor_key: &str, layout_id: i64) -> zbus::Result<()>;
    async fn get_active_layout(&self, monitor_key: &str) -> zbus::Result<LayoutWire>;

    async fn get_settings(&self) -> zbus::Result<HashMap<String, String>>;
    async fn set_setting(&self, key: &str, value: &str) -> zbus::Result<()>;

    async fn snap_focused_to_zone(&self, zone_index: u32, span: bool) -> zbus::Result<()>;
    async fn show_activator(&self) -> zbus::Result<()>;
    async fn toggle_paused(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn layouts_changed(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn layout_assigned(&self, monitor_key: String, layout_id: i64) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn monitors_changed(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn paused_changed(&self, paused: bool) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn activator_requested(&self, monitor_key: String) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn editor_requested(&self, monitor_key: String) -> zbus::Result<()>;
}
