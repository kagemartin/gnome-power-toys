// crates/gnome-zones-daemon/src/model.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ZoneRect {
    pub zone_index: u32,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl ZoneRect {
    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn is_normalized(&self) -> bool {
        self.x >= 0.0 && self.y >= 0.0
            && self.w > 0.0 && self.h > 0.0
            && self.x + self.w <= 1.0 + f64::EPSILON
            && self.y + self.h <= 1.0 + f64::EPSILON
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutSummary {
    pub id: i64,
    pub name: String,
    pub is_preset: bool,
    pub zone_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub id: i64,
    pub name: String,
    pub is_preset: bool,
    pub zones: Vec<ZoneRect>,
}

impl Layout {
    pub fn zone(&self, zone_index: u32) -> Option<&ZoneRect> {
        self.zones.iter().find(|z| z.zone_index == zone_index)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub monitor_key: String,
    pub connector: String,
    pub name: String,
    pub width_px: u32,
    pub height_px: u32,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IterateDir {
    Prev,
    Next,
}

impl std::str::FromStr for IterateDir {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        match s {
            "prev" => Ok(Self::Prev),
            "next" => Ok(Self::Next),
            other  => Err(format!("unknown direction: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_center_is_midpoint() {
        let r = ZoneRect { zone_index: 1, x: 0.0, y: 0.0, w: 0.5, h: 1.0 };
        assert_eq!(r.center(), (0.25, 0.5));
    }

    #[test]
    fn normalized_accepts_full_rect() {
        let full = ZoneRect { zone_index: 1, x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
        assert!(full.is_normalized());
    }

    #[test]
    fn normalized_rejects_negative_origin() {
        let bad = ZoneRect { zone_index: 1, x: -0.1, y: 0.0, w: 0.5, h: 0.5 };
        assert!(!bad.is_normalized());
    }

    #[test]
    fn normalized_rejects_overflow() {
        let bad = ZoneRect { zone_index: 1, x: 0.5, y: 0.0, w: 0.6, h: 1.0 };
        assert!(!bad.is_normalized());
    }

    #[test]
    fn layout_zone_lookup_by_index() {
        let layout = Layout {
            id: 1, name: "t".into(), is_preset: false,
            zones: vec![
                ZoneRect { zone_index: 1, x: 0.0, y: 0.0, w: 0.5, h: 1.0 },
                ZoneRect { zone_index: 2, x: 0.5, y: 0.0, w: 0.5, h: 1.0 },
            ],
        };
        assert_eq!(layout.zone(2).unwrap().x, 0.5);
        assert!(layout.zone(99).is_none());
    }

    #[test]
    fn iterate_dir_parses() {
        assert_eq!("prev".parse::<IterateDir>().unwrap(), IterateDir::Prev);
        assert_eq!("next".parse::<IterateDir>().unwrap(), IterateDir::Next);
        assert!("sideways".parse::<IterateDir>().is_err());
    }
}
