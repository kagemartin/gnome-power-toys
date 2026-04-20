// crates/gnome-zones-daemon/src/presets.rs
use crate::db::{layouts, Database};
use crate::error::Result;
use crate::model::ZoneRect;
use rusqlite::params;

fn z(i: u32, x: f64, y: f64, w: f64, h: f64) -> ZoneRect {
    ZoneRect { zone_index: i, x, y, w, h }
}

/// Name + zone list for each built-in preset.
///
/// Numbering is row-major (top-to-bottom, left-to-right by top-left corner).
pub fn builtin_presets() -> Vec<(&'static str, Vec<ZoneRect>)> {
    vec![
        ("Two Columns (50/50)", vec![
            z(1, 0.0, 0.0, 0.5, 1.0),
            z(2, 0.5, 0.0, 0.5, 1.0),
        ]),
        ("Three Columns", vec![
            z(1, 0.0,         0.0, 1.0 / 3.0, 1.0),
            z(2, 1.0 / 3.0,   0.0, 1.0 / 3.0, 1.0),
            z(3, 2.0 / 3.0,   0.0, 1.0 / 3.0, 1.0),
        ]),
        ("2/3 | 1/3", vec![
            z(1, 0.0,         0.0, 2.0 / 3.0, 1.0),
            z(2, 2.0 / 3.0,   0.0, 1.0 / 3.0, 1.0),
        ]),
        ("1/3 | 2/3", vec![
            z(1, 0.0,         0.0, 1.0 / 3.0, 1.0),
            z(2, 1.0 / 3.0,   0.0, 2.0 / 3.0, 1.0),
        ]),
        ("2×2 Grid", vec![
            z(1, 0.0, 0.0, 0.5, 0.5),
            z(2, 0.5, 0.0, 0.5, 0.5),
            z(3, 0.0, 0.5, 0.5, 0.5),
            z(4, 0.5, 0.5, 0.5, 0.5),
        ]),
        ("1/3 | 1/3 | 1/3", vec![
            z(1, 0.0,         0.0, 1.0 / 3.0, 1.0),
            z(2, 1.0 / 3.0,   0.0, 1.0 / 3.0, 1.0),
            z(3, 2.0 / 3.0,   0.0, 1.0 / 3.0, 1.0),
        ]),
        ("Sidebar + Main (1/4 | 3/4)", vec![
            z(1, 0.0,  0.0, 0.25, 1.0),
            z(2, 0.25, 0.0, 0.75, 1.0),
        ]),
        ("Main + Sidebar (3/4 | 1/4)", vec![
            z(1, 0.0,  0.0, 0.75, 1.0),
            z(2, 0.75, 0.0, 0.25, 1.0),
        ]),
    ]
}

/// Idempotent — safe to call on every daemon start.
pub fn seed(db: &mut Database) -> Result<()> {
    for (name, zones) in builtin_presets() {
        let exists: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM layouts WHERE name = ?1 AND is_preset = 1",
            params![name],
            |r| r.get(0),
        )?;
        if exists == 0 {
            layouts::create_layout(db, name, true, &zones)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::temp_db;

    #[test]
    fn every_preset_covers_exactly_unit_area() {
        for (name, zones) in builtin_presets() {
            let total: f64 = zones.iter().map(|z| z.w * z.h).sum();
            assert!((total - 1.0).abs() < 1e-9, "{name} does not tile the unit square ({total})");
        }
    }

    #[test]
    fn every_preset_uses_sequential_indices() {
        for (name, zones) in builtin_presets() {
            for (i, z) in zones.iter().enumerate() {
                assert_eq!(z.zone_index as usize, i + 1, "{name} zone {i}");
            }
        }
    }

    #[test]
    fn seed_populates_all_presets() {
        let mut db = temp_db();
        seed(&mut db).unwrap();
        let count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM layouts WHERE is_preset = 1", [], |r: &rusqlite::Row| r.get(0),
        ).unwrap();
        assert_eq!(count as usize, builtin_presets().len());
    }

    #[test]
    fn seed_is_idempotent() {
        let mut db = temp_db();
        seed(&mut db).unwrap();
        seed(&mut db).unwrap();
        seed(&mut db).unwrap();
        let count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM layouts WHERE is_preset = 1", [], |r: &rusqlite::Row| r.get(0),
        ).unwrap();
        assert_eq!(count as usize, builtin_presets().len());
    }
}
