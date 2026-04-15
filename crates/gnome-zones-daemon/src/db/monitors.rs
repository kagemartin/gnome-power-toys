// crates/gnome-zones-daemon/src/db/monitors.rs
use crate::db::Database;
use crate::error::Result;
use rusqlite::{params, OptionalExtension};

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn assign_layout(db: &Database, monitor_key: &str, layout_id: i64) -> Result<()> {
    db.conn.execute(
        "INSERT INTO monitor_assignments (monitor_key, layout_id, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(monitor_key) DO UPDATE SET layout_id = excluded.layout_id,
                                                updated_at = excluded.updated_at",
        params![monitor_key, layout_id, now_unix()],
    )?;
    Ok(())
}

pub fn get_assigned_layout_id(db: &Database, monitor_key: &str) -> Result<Option<i64>> {
    let id: Option<i64> = db.conn.query_row(
        "SELECT layout_id FROM monitor_assignments WHERE monitor_key = ?1",
        params![monitor_key],
        |r| r.get(0),
    ).optional()?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::layouts::create_layout;
    use crate::db::tests::temp_db;
    use crate::model::ZoneRect;

    fn z(i: u32, x: f64, y: f64, w: f64, h: f64) -> ZoneRect {
        ZoneRect { zone_index: i, x, y, w, h }
    }

    #[test]
    fn assign_and_retrieve() {
        let mut db = temp_db();
        let id = create_layout(&mut db, "L", false, &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap();
        assign_layout(&db, "DP-1:abc123", id).unwrap();
        assert_eq!(get_assigned_layout_id(&db, "DP-1:abc123").unwrap(), Some(id));
    }

    #[test]
    fn unassigned_monitor_returns_none() {
        let db = temp_db();
        assert_eq!(get_assigned_layout_id(&db, "never-plugged").unwrap(), None);
    }

    #[test]
    fn assigning_same_monitor_twice_overwrites() {
        let mut db = temp_db();
        let a = create_layout(&mut db, "A", false, &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap();
        let b = create_layout(&mut db, "B", false, &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap();
        assign_layout(&db, "DP-1", a).unwrap();
        assign_layout(&db, "DP-1", b).unwrap();
        assert_eq!(get_assigned_layout_id(&db, "DP-1").unwrap(), Some(b));
    }
}
