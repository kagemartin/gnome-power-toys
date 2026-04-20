use crate::model::{IterateDir, ZoneRect};

/// Pixel rectangle. Same layout everywhere: top-left + size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Project a fractional zone onto a monitor of given pixel size.
pub fn project_rect(zone: &ZoneRect, monitor_w: i32, monitor_h: i32) -> PixelRect {
    PixelRect {
        x: (zone.x * monitor_w as f64).round() as i32,
        y: (zone.y * monitor_h as f64).round() as i32,
        w: (zone.w * monitor_w as f64).round() as i32,
        h: (zone.h * monitor_h as f64).round() as i32,
    }
}

/// Shrink all four sides by `gap` pixels. A rect smaller than `2*gap`
/// on either axis clamps to a single pixel so we never emit negative sizes.
pub fn deflate(rect: PixelRect, gap: i32) -> PixelRect {
    let w = (rect.w - 2 * gap).max(1);
    let h = (rect.h - 2 * gap).max(1);
    PixelRect { x: rect.x + gap, y: rect.y + gap, w, h }
}

/// Bounding rectangle (union) of the given zones, in fractional coords.
/// Panics if `zones` is empty — callers must ensure at least one zone.
pub fn bounding_rect(zones: &[&ZoneRect]) -> ZoneRect {
    assert!(!zones.is_empty(), "bounding_rect of empty set");
    let x0 = zones.iter().map(|z| z.x).fold(f64::INFINITY, f64::min);
    let y0 = zones.iter().map(|z| z.y).fold(f64::INFINITY, f64::min);
    let x1 = zones.iter().map(|z| z.x + z.w).fold(f64::NEG_INFINITY, f64::max);
    let y1 = zones.iter().map(|z| z.y + z.h).fold(f64::NEG_INFINITY, f64::max);
    ZoneRect {
        zone_index: zones[0].zone_index,  // caller picks one; irrelevant for layout
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

/// Compute the next zone index for indexed iteration.
///
/// `current_index` — 1-based current zone, or 0 if the window isn't snapped.
/// `zone_count`    — total zones on the active layout (must be > 0).
///
/// Returns the new 1-based index. Wraps.
pub fn iterate_index(current_index: u32, zone_count: u32, dir: IterateDir) -> u32 {
    assert!(zone_count > 0, "iterate_index needs at least one zone");
    match dir {
        IterateDir::Next => {
            // Unsnapped (0) → 1. Snapped → (current mod n) + 1.
            if current_index == 0 {
                1
            } else {
                (current_index % zone_count) + 1
            }
        }
        IterateDir::Prev => {
            // Unsnapped (0) → last. Snapped → ((c - 2 + n) mod n) + 1.
            if current_index == 0 {
                zone_count
            } else {
                ((current_index + zone_count - 2) % zone_count) + 1
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn z(i: u32, x: f64, y: f64, w: f64, h: f64) -> ZoneRect {
        ZoneRect { zone_index: i, x, y, w, h }
    }

    #[test]
    fn project_full_screen() {
        let r = project_rect(&z(1, 0.0, 0.0, 1.0, 1.0), 1920, 1080);
        assert_eq!(r, PixelRect { x: 0, y: 0, w: 1920, h: 1080 });
    }

    #[test]
    fn project_right_third() {
        let r = project_rect(&z(1, 2.0 / 3.0, 0.0, 1.0 / 3.0, 1.0), 1920, 1080);
        assert!((r.x - 1280).abs() <= 1);
        assert!((r.w - 640).abs()  <= 1);
    }

    #[test]
    fn deflate_shrinks_four_sides() {
        let r = deflate(PixelRect { x: 0, y: 0, w: 100, h: 100 }, 8);
        assert_eq!(r, PixelRect { x: 8, y: 8, w: 84, h: 84 });
    }

    #[test]
    fn deflate_clamps_when_too_small() {
        let r = deflate(PixelRect { x: 0, y: 0, w: 4, h: 100 }, 8);
        assert_eq!(r.w, 1);
        assert_eq!(r.h, 84);
    }

    #[test]
    fn bounding_rect_of_two_columns() {
        let a = z(1, 0.0, 0.0, 0.5, 1.0);
        let b = z(2, 0.5, 0.0, 0.5, 1.0);
        let u = bounding_rect(&[&a, &b]);
        assert!((u.x - 0.0).abs() < 1e-9);
        assert!((u.w - 1.0).abs() < 1e-9);
        assert!((u.h - 1.0).abs() < 1e-9);
    }

    #[test]
    fn bounding_rect_of_disjoint_zones() {
        let a = z(1, 0.0, 0.0, 0.3, 0.5);
        let b = z(2, 0.7, 0.5, 0.3, 0.5);
        let u = bounding_rect(&[&a, &b]);
        assert!((u.x - 0.0).abs() < 1e-9);
        assert!((u.y - 0.0).abs() < 1e-9);
        assert!((u.w - 1.0).abs() < 1e-9);
        assert!((u.h - 1.0).abs() < 1e-9);
    }

    #[test]
    fn iterate_next_wraps() {
        assert_eq!(iterate_index(1, 3, IterateDir::Next), 2);
        assert_eq!(iterate_index(2, 3, IterateDir::Next), 3);
        assert_eq!(iterate_index(3, 3, IterateDir::Next), 1);
    }

    #[test]
    fn iterate_prev_wraps() {
        assert_eq!(iterate_index(3, 3, IterateDir::Prev), 2);
        assert_eq!(iterate_index(2, 3, IterateDir::Prev), 1);
        assert_eq!(iterate_index(1, 3, IterateDir::Prev), 3);
    }

    #[test]
    fn iterate_unsnapped_next_lands_on_1() {
        assert_eq!(iterate_index(0, 5, IterateDir::Next), 1);
    }

    #[test]
    fn iterate_unsnapped_prev_lands_on_last() {
        assert_eq!(iterate_index(0, 5, IterateDir::Prev), 5);
    }

    #[test]
    fn iterate_single_zone_is_fixpoint() {
        assert_eq!(iterate_index(1, 1, IterateDir::Next), 1);
        assert_eq!(iterate_index(1, 1, IterateDir::Prev), 1);
    }
}
