use crate::dbus::{LayoutWire, ZoneWire};

/// A zone in the working copy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zone {
    pub zone_index: u32,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl From<&ZoneWire> for Zone {
    fn from(z: &ZoneWire) -> Self {
        Self { zone_index: z.zone_index, x: z.x, y: z.y, w: z.w, h: z.h }
    }
}

impl From<&Zone> for ZoneWire {
    fn from(z: &Zone) -> Self {
        ZoneWire { zone_index: z.zone_index, x: z.x, y: z.y, w: z.w, h: z.h }
    }
}

/// Editor working copy.
#[derive(Debug, Clone)]
pub struct EditorState {
    pub layout_id: Option<i64>,
    pub name: String,
    pub is_preset: bool,
    pub zones: Vec<Zone>,
    pub selected: Option<u32>,
    original: Vec<Zone>,
    original_name: String,
}

impl EditorState {
    pub fn from_layout(layout: &LayoutWire) -> Self {
        let zones: Vec<Zone> = layout.zones.iter().map(Zone::from).collect();
        Self {
            layout_id: Some(layout.id),
            name: layout.name.clone(),
            is_preset: layout.is_preset,
            zones: zones.clone(),
            selected: zones.first().map(|z| z.zone_index),
            original: zones,
            original_name: layout.name.clone(),
        }
    }

    pub fn select(&mut self, zone_index: u32) {
        if self.zones.iter().any(|z| z.zone_index == zone_index) {
            self.selected = Some(zone_index);
        }
    }

    pub fn selected_zone(&self) -> Option<&Zone> {
        self.selected.and_then(|i| self.zones.iter().find(|z| z.zone_index == i))
    }

    pub fn reset(&mut self) {
        self.zones = self.original.clone();
        self.name = self.original_name.clone();
        self.selected = self.zones.first().map(|z| z.zone_index);
    }

    pub fn is_dirty(&self) -> bool {
        self.name != self.original_name || self.zones != self.original
    }

    /// Renumber zones in row-major reading order based on top-left corner.
    /// Preserves `selected` by tracking the zone identity through the resort.
    pub fn renumber_row_major(&mut self) {
        let selected_pos = self
            .selected
            .and_then(|i| self.zones.iter().position(|z| z.zone_index == i));

        let eps = 1e-6;
        let mut order: Vec<usize> = (0..self.zones.len()).collect();
        order.sort_by(|&a, &b| {
            let za = &self.zones[a];
            let zb = &self.zones[b];
            if (za.y - zb.y).abs() < eps {
                za.x.partial_cmp(&zb.x).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                za.y.partial_cmp(&zb.y).unwrap_or(std::cmp::Ordering::Equal)
            }
        });

        let reordered: Vec<Zone> = order
            .iter()
            .enumerate()
            .map(|(new_idx, &old_pos)| {
                let mut z = self.zones[old_pos];
                z.zone_index = (new_idx + 1) as u32;
                z
            })
            .collect();

        if let Some(old_pos) = selected_pos {
            let new_pos = order.iter().position(|&p| p == old_pos).unwrap();
            self.selected = Some((new_pos + 1) as u32);
        }
        self.zones = reordered;
    }

    /// Split the selected zone into top/bottom halves. No-op if nothing selected.
    pub fn split_horizontal(&mut self) {
        let Some(idx) = self.selected else { return; };
        let Some(pos) = self.zones.iter().position(|z| z.zone_index == idx) else { return; };

        let z = self.zones[pos];
        let half = z.h / 2.0;
        let top = Zone { zone_index: 0, x: z.x, y: z.y,          w: z.w, h: half };
        let bot = Zone { zone_index: 0, x: z.x, y: z.y + half,   w: z.w, h: half };

        self.zones.remove(pos);
        self.zones.push(top);
        self.zones.push(bot);
        self.renumber_row_major();
        if let Some(t) = self.zones.iter().find(|zz| (zz.x - top.x).abs() < 1e-9 && (zz.y - top.y).abs() < 1e-9) {
            self.selected = Some(t.zone_index);
        }
    }

    /// Split the selected zone into left/right halves. No-op if nothing selected.
    pub fn split_vertical(&mut self) {
        let Some(idx) = self.selected else { return; };
        let Some(pos) = self.zones.iter().position(|z| z.zone_index == idx) else { return; };

        let z = self.zones[pos];
        let half = z.w / 2.0;
        let left  = Zone { zone_index: 0, x: z.x,         y: z.y, w: half, h: z.h };
        let right = Zone { zone_index: 0, x: z.x + half,  y: z.y, w: half, h: z.h };

        self.zones.remove(pos);
        self.zones.push(left);
        self.zones.push(right);
        self.renumber_row_major();
        if let Some(l) = self.zones.iter().find(|zz| (zz.x - left.x).abs() < 1e-9 && (zz.y - left.y).abs() < 1e-9) {
            self.selected = Some(l.zone_index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbus::{LayoutWire, ZoneWire};

    fn zw(i: u32, x: f64, y: f64, w: f64, h: f64) -> ZoneWire {
        ZoneWire { zone_index: i, x, y, w, h }
    }

    fn two_col_layout() -> LayoutWire {
        LayoutWire {
            id: 7,
            name: "Two Columns".into(),
            is_preset: true,
            zones: vec![zw(1, 0.0, 0.0, 0.5, 1.0), zw(2, 0.5, 0.0, 0.5, 1.0)],
        }
    }

    #[test]
    fn from_layout_seeds_selection() {
        let s = EditorState::from_layout(&two_col_layout());
        assert_eq!(s.selected, Some(1));
        assert_eq!(s.zones.len(), 2);
        assert!(s.is_preset);
        assert!(!s.is_dirty());
    }

    #[test]
    fn select_existing_zone() {
        let mut s = EditorState::from_layout(&two_col_layout());
        s.select(2);
        assert_eq!(s.selected, Some(2));
    }

    #[test]
    fn select_ignores_unknown_zone() {
        let mut s = EditorState::from_layout(&two_col_layout());
        s.select(99);
        assert_eq!(s.selected, Some(1));
    }

    #[test]
    fn renumber_sorts_row_major() {
        let mut s = EditorState::from_layout(&LayoutWire {
            id: 1, name: "tmp".into(), is_preset: false,
            zones: vec![
                zw(1, 0.5, 0.5, 0.5, 0.5),
                zw(2, 0.0, 0.0, 0.5, 0.5),
            ],
        });
        s.select(2);
        s.renumber_row_major();
        assert_eq!(s.zones[0].zone_index, 1);
        assert!((s.zones[0].x - 0.0).abs() < 1e-9);
        assert!((s.zones[0].y - 0.0).abs() < 1e-9);
        assert_eq!(s.zones[1].zone_index, 2);
        assert_eq!(s.selected, Some(1));
    }

    #[test]
    fn renumber_groups_by_row() {
        let mut s = EditorState::from_layout(&LayoutWire {
            id: 1, name: "tmp".into(), is_preset: false,
            zones: vec![
                zw(1, 0.5, 0.5, 0.5, 0.5),
                zw(2, 0.0, 0.5, 0.5, 0.5),
                zw(3, 0.5, 0.0, 0.5, 0.5),
                zw(4, 0.0, 0.0, 0.5, 0.5),
            ],
        });
        s.renumber_row_major();
        assert_eq!(s.zones[0].x, 0.0); assert_eq!(s.zones[0].y, 0.0);
        assert_eq!(s.zones[1].x, 0.5); assert_eq!(s.zones[1].y, 0.0);
        assert_eq!(s.zones[2].x, 0.0); assert_eq!(s.zones[2].y, 0.5);
        assert_eq!(s.zones[3].x, 0.5); assert_eq!(s.zones[3].y, 0.5);
    }

    #[test]
    fn reset_restores_original() {
        let mut s = EditorState::from_layout(&two_col_layout());
        s.name = "garbage".into();
        s.zones[0].w = 0.9;
        assert!(s.is_dirty());
        s.reset();
        assert!(!s.is_dirty());
        assert_eq!(s.name, "Two Columns");
        assert_eq!(s.zones[0].w, 0.5);
    }

    #[test]
    fn split_horizontal_selected_zone() {
        let mut s = EditorState::from_layout(&two_col_layout());
        s.select(1);
        s.split_horizontal();
        assert_eq!(s.zones.len(), 3);
        let top = s.zones.iter().find(|z| z.x == 0.0 && z.y == 0.0).unwrap();
        let bot = s.zones.iter().find(|z| z.x == 0.0 && (z.y - 0.5).abs() < 1e-9).unwrap();
        assert!((top.h - 0.5).abs() < 1e-9);
        assert!((bot.h - 0.5).abs() < 1e-9);
        assert!((top.w - 0.5).abs() < 1e-9);
        assert_eq!(s.selected, Some(1));
    }

    #[test]
    fn split_vertical_selected_zone() {
        let mut s = EditorState::from_layout(&two_col_layout());
        s.select(1);
        s.split_vertical();
        assert_eq!(s.zones.len(), 3);
        let left = s.zones.iter().find(|z| z.x == 0.0).unwrap();
        let mid  = s.zones.iter().find(|z| (z.x - 0.25).abs() < 1e-9).unwrap();
        assert!((left.w - 0.25).abs() < 1e-9);
        assert!((mid.w  - 0.25).abs() < 1e-9);
        assert!((left.h - 1.0).abs()  < 1e-9);
    }

    #[test]
    fn split_without_selection_is_noop() {
        let mut s = EditorState::from_layout(&two_col_layout());
        s.selected = None;
        s.split_horizontal();
        assert_eq!(s.zones.len(), 2);
    }

    #[test]
    fn split_marks_dirty() {
        let mut s = EditorState::from_layout(&two_col_layout());
        assert!(!s.is_dirty());
        s.split_vertical();
        assert!(s.is_dirty());
    }
}
