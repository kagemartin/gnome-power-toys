// crates/gnome-zones-daemon/src/dbus/types.rs
use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

/// Wire type for a zone. D-Bus signature: (uddddd).
///
/// Kept separate from crate::model::ZoneRect so that internal Rust types
/// can evolve without breaking the D-Bus ABI.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ZoneWire {
    pub zone_index: u32,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl From<&crate::model::ZoneRect> for ZoneWire {
    fn from(r: &crate::model::ZoneRect) -> Self {
        Self { zone_index: r.zone_index, x: r.x, y: r.y, w: r.w, h: r.h }
    }
}

impl From<ZoneWire> for crate::model::ZoneRect {
    fn from(w: ZoneWire) -> Self {
        Self { zone_index: w.zone_index, x: w.x, y: w.y, w: w.w, h: w.h }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct LayoutSummaryWire {
    pub id: i64,
    pub name: String,
    pub is_preset: bool,
    pub zone_count: u32,
}

impl From<&crate::model::LayoutSummary> for LayoutSummaryWire {
    fn from(s: &crate::model::LayoutSummary) -> Self {
        Self {
            id: s.id,
            name: s.name.clone(),
            is_preset: s.is_preset,
            zone_count: s.zone_count,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct LayoutWire {
    pub id: i64,
    pub name: String,
    pub is_preset: bool,
    pub zones: Vec<ZoneWire>,
}

impl From<&crate::model::Layout> for LayoutWire {
    fn from(l: &crate::model::Layout) -> Self {
        Self {
            id: l.id,
            name: l.name.clone(),
            is_preset: l.is_preset,
            zones: l.zones.iter().map(ZoneWire::from).collect(),
        }
    }
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

impl From<&crate::model::MonitorInfo> for MonitorInfoWire {
    fn from(m: &crate::model::MonitorInfo) -> Self {
        Self {
            monitor_key: m.monitor_key.clone(),
            connector: m.connector.clone(),
            name: m.name.clone(),
            width_px: m.width_px,
            height_px: m.height_px,
            is_primary: m.is_primary,
        }
    }
}
