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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Divider runs vertically (left zone sits to the left of right zone).
    Vertical,
    /// Divider runs horizontally (top zone sits above bottom zone).
    Horizontal,
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

    /// Delete the selected zone. If a single neighbor shares the full edge
    /// where the deletion happens, extend it to cover the deleted area.
    pub fn delete_selected(&mut self) {
        let Some(idx) = self.selected else { return; };
        let Some(pos) = self.zones.iter().position(|z| z.zone_index == idx) else { return; };
        let deleted = self.zones[pos];

        let eps = 1e-6;
        let mut candidates: Vec<(usize, f64)> = Vec::new();

        for (i, n) in self.zones.iter().enumerate() {
            if i == pos { continue; }
            let right_matches = (n.x + n.w - deleted.x).abs() < eps
                && (n.y - deleted.y).abs() < eps
                && (n.h - deleted.h).abs() < eps;
            let left_matches  = (n.x - (deleted.x + deleted.w)).abs() < eps
                && (n.y - deleted.y).abs() < eps
                && (n.h - deleted.h).abs() < eps;
            let below_matches = (n.y - (deleted.y + deleted.h)).abs() < eps
                && (n.x - deleted.x).abs() < eps
                && (n.w - deleted.w).abs() < eps;
            let above_matches = (n.y + n.h - deleted.y).abs() < eps
                && (n.x - deleted.x).abs() < eps
                && (n.w - deleted.w).abs() < eps;

            if right_matches || left_matches || above_matches || below_matches {
                candidates.push((i, n.w * n.h));
            }
        }

        let chosen = candidates
            .into_iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        if let Some((ni, _)) = chosen {
            let n = self.zones[ni];
            let merged = merge_rects(&n, &deleted);
            self.zones[ni] = merged;
            self.zones.remove(pos);
        } else {
            self.zones.remove(pos);
        }

        if self.zones.is_empty() {
            self.selected = None;
        } else {
            self.renumber_row_major();
            self.selected = Some(self.zones[0].zone_index);
        }
    }

    /// Append a user-drawn zone. Fractional coords; renumbers row-major.
    pub fn add_zone(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.zones.push(Zone { zone_index: 0, x, y, w, h });
        self.renumber_row_major();
    }

    /// Return all divider pairs (first_idx, second_idx, axis) where the two zones share
    /// a full edge. Used by the view to place drag handles.
    pub fn shared_edges(&self) -> Vec<(u32, u32, Axis)> {
        let eps = 1e-6;
        let mut out = Vec::new();
        for (i, a) in self.zones.iter().enumerate() {
            for b in self.zones.iter().skip(i + 1) {
                if (a.x + a.w - b.x).abs() < eps
                    && (a.y - b.y).abs() < eps
                    && (a.h - b.h).abs() < eps {
                    out.push((a.zone_index, b.zone_index, Axis::Vertical));
                } else if (b.x + b.w - a.x).abs() < eps
                    && (a.y - b.y).abs() < eps
                    && (a.h - b.h).abs() < eps {
                    out.push((b.zone_index, a.zone_index, Axis::Vertical));
                } else if (a.y + a.h - b.y).abs() < eps
                    && (a.x - b.x).abs() < eps
                    && (a.w - b.w).abs() < eps {
                    out.push((a.zone_index, b.zone_index, Axis::Horizontal));
                } else if (b.y + b.h - a.y).abs() < eps
                    && (a.x - b.x).abs() < eps
                    && (a.w - b.w).abs() < eps {
                    out.push((b.zone_index, a.zone_index, Axis::Horizontal));
                }
            }
        }
        out
    }

    /// Move a shared divider between two zones by a fractional delta.
    pub fn move_divider(&mut self, first_idx: u32, second_idx: u32, axis: Axis, delta: f64) {
        const MIN_DIVIDER_GAP: f64 = 0.02;
        let Some(pa) = self.zones.iter().position(|z| z.zone_index == first_idx) else { return; };
        let Some(pb) = self.zones.iter().position(|z| z.zone_index == second_idx) else { return; };
        if pa == pb { return; }

        match axis {
            Axis::Vertical => {
                let a_w = self.zones[pa].w;
                let b_x = self.zones[pb].x;
                let b_w = self.zones[pb].w;
                let d = delta
                    .max(-a_w + MIN_DIVIDER_GAP)
                    .min( b_w - MIN_DIVIDER_GAP);
                self.zones[pa].w = a_w + d;
                self.zones[pb].x = b_x + d;
                self.zones[pb].w = b_w - d;
            }
            Axis::Horizontal => {
                let a_h = self.zones[pa].h;
                let b_y = self.zones[pb].y;
                let b_h = self.zones[pb].h;
                let d = delta
                    .max(-a_h + MIN_DIVIDER_GAP)
                    .min( b_h - MIN_DIVIDER_GAP);
                self.zones[pa].h = a_h + d;
                self.zones[pb].y = b_y + d;
                self.zones[pb].h = b_h - d;
            }
        }
    }
}

fn merge_rects(neighbor: &Zone, deleted: &Zone) -> Zone {
    let x0 = neighbor.x.min(deleted.x);
    let y0 = neighbor.y.min(deleted.y);
    let x1 = (neighbor.x + neighbor.w).max(deleted.x + deleted.w);
    let y1 = (neighbor.y + neighbor.h).max(deleted.y + deleted.h);
    Zone {
        zone_index: neighbor.zone_index,
        x: x0, y: y0,
        w: x1 - x0, h: y1 - y0,
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

    #[test]
    fn delete_extends_neighbor_when_edges_match() {
        let mut s = EditorState::from_layout(&two_col_layout());
        s.select(2);
        s.delete_selected();
        assert_eq!(s.zones.len(), 1);
        let z = &s.zones[0];
        assert!((z.x - 0.0).abs() < 1e-9);
        assert!((z.w - 1.0).abs() < 1e-9);
        assert!((z.h - 1.0).abs() < 1e-9);
        assert_eq!(z.zone_index, 1);
    }

    #[test]
    fn delete_picks_largest_neighbor_on_tie() {
        let mut s = EditorState::from_layout(&LayoutWire {
            id: 1, name: "t".into(), is_preset: false,
            zones: vec![
                zw(1, 0.0, 0.0, 1.0, 0.5),
                zw(2, 0.0, 0.5, 0.5, 0.5),
                zw(3, 0.5, 0.5, 0.5, 0.5),
            ],
        });
        s.select(1);
        s.delete_selected();
        assert_eq!(s.zones.len(), 2);
    }

    #[test]
    fn delete_last_zone_leaves_empty_layout() {
        let mut s = EditorState::from_layout(&LayoutWire {
            id: 1, name: "t".into(), is_preset: false,
            zones: vec![zw(1, 0.0, 0.0, 1.0, 1.0)],
        });
        s.select(1);
        s.delete_selected();
        assert!(s.zones.is_empty());
        assert_eq!(s.selected, None);
    }

    #[test]
    fn delete_without_selection_is_noop() {
        let mut s = EditorState::from_layout(&two_col_layout());
        s.selected = None;
        s.delete_selected();
        assert_eq!(s.zones.len(), 2);
    }

    #[test]
    fn add_zone_appends_and_renumbers() {
        let mut s = EditorState::from_layout(&two_col_layout());
        s.add_zone(0.1, 0.1, 0.2, 0.2);
        assert_eq!(s.zones.len(), 3);
        let new_zone = s.zones.iter().find(|z| (z.x - 0.1).abs() < 1e-9).unwrap();
        assert!((new_zone.w - 0.2).abs() < 1e-9);
        assert!(s.is_dirty());
    }

    #[test]
    fn move_divider_vertical_between_columns() {
        let mut s = EditorState::from_layout(&two_col_layout());
        s.move_divider(1, 2, Axis::Vertical, -0.1);
        let left  = s.zones.iter().find(|z| z.x == 0.0).unwrap();
        let right = s.zones.iter().find(|z| (z.x - 0.4).abs() < 1e-9).unwrap();
        assert!((left.w - 0.4).abs() < 1e-9);
        assert!((right.w - 0.6).abs() < 1e-9);
    }

    #[test]
    fn move_divider_clamps_at_edges() {
        let mut s = EditorState::from_layout(&two_col_layout());
        s.move_divider(1, 2, Axis::Vertical, 0.6);
        let left  = s.zones.iter().find(|z| z.x == 0.0).unwrap();
        let right = s.zones.iter().find(|z| z.x > 0.0).unwrap();
        assert!(left.w > 0.0);
        assert!(right.w > 0.0);
        assert!((left.w + right.w - 1.0).abs() < 1e-9);
    }

    #[test]
    fn shared_edges_for_two_columns() {
        let s = EditorState::from_layout(&two_col_layout());
        let edges = s.shared_edges();
        assert_eq!(edges.len(), 1);
        let (a, b, axis) = edges[0];
        assert_eq!((a, b), (1, 2));
        assert_eq!(axis, Axis::Vertical);
    }

    #[test]
    fn shared_edges_for_2x2() {
        let s = EditorState::from_layout(&LayoutWire {
            id: 1, name: "t".into(), is_preset: false,
            zones: vec![
                zw(1, 0.0, 0.0, 0.5, 0.5),
                zw(2, 0.5, 0.0, 0.5, 0.5),
                zw(3, 0.0, 0.5, 0.5, 0.5),
                zw(4, 0.5, 0.5, 0.5, 0.5),
            ],
        });
        let edges = s.shared_edges();
        assert_eq!(edges.len(), 4);
    }
}
