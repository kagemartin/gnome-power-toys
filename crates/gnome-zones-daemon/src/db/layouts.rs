// crates/gnome-zones-daemon/src/db/layouts.rs
use crate::db::Database;
use crate::error::Result;
use crate::model::{Layout, LayoutSummary, ZoneRect};
use rusqlite::{params, OptionalExtension};

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn create_layout(
    db: &mut Database,
    name: &str,
    is_preset: bool,
    zones: &[ZoneRect],
) -> Result<i64> {
    let tx = db.conn.transaction()?;
    tx.execute(
        "INSERT INTO layouts (name, is_preset, created_at) VALUES (?1, ?2, ?3)",
        params![name, is_preset as i32, now_unix()],
    )?;
    let layout_id = tx.last_insert_rowid();
    for z in zones {
        tx.execute(
            "INSERT INTO zones (layout_id, zone_index, x, y, w, h)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![layout_id, z.zone_index, z.x, z.y, z.w, z.h],
        )?;
    }
    tx.commit()?;
    Ok(layout_id)
}

pub fn update_layout(
    db: &mut Database,
    id: i64,
    name: &str,
    zones: &[ZoneRect],
) -> Result<()> {
    let tx = db.conn.transaction()?;
    let is_preset: i32 = tx.query_row(
        "SELECT is_preset FROM layouts WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    if is_preset == 1 {
        return Err(crate::error::Error::Config(
            "cannot modify a preset layout — fork with Save As first".into(),
        ));
    }
    tx.execute("UPDATE layouts SET name = ?1 WHERE id = ?2", params![name, id])?;
    tx.execute("DELETE FROM zones WHERE layout_id = ?1", params![id])?;
    for z in zones {
        tx.execute(
            "INSERT INTO zones (layout_id, zone_index, x, y, w, h)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, z.zone_index, z.x, z.y, z.w, z.h],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn delete_layout(db: &mut Database, id: i64) -> Result<()> {
    let is_preset: i32 = db.conn.query_row(
        "SELECT is_preset FROM layouts WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    if is_preset == 1 {
        return Err(crate::error::Error::Config("cannot delete a preset layout".into()));
    }
    db.conn.execute("DELETE FROM layouts WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn get_layout(db: &Database, id: i64) -> Result<Option<Layout>> {
    let row = db.conn.query_row(
        "SELECT id, name, is_preset FROM layouts WHERE id = ?1",
        params![id],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i32>(2)?)),
    ).optional()?;
    let Some((id, name, is_preset)) = row else { return Ok(None) };

    let mut stmt = db.conn.prepare(
        "SELECT zone_index, x, y, w, h FROM zones WHERE layout_id = ?1 ORDER BY zone_index",
    )?;
    let zones: Vec<ZoneRect> = stmt
        .query_map(params![id], |r| {
            Ok(ZoneRect {
                zone_index: r.get::<_, i64>(0)? as u32,
                x: r.get(1)?,
                y: r.get(2)?,
                w: r.get(3)?,
                h: r.get(4)?,
            })
        })?
        .map(|r| r.map_err(crate::error::Error::from))
        .collect::<Result<_>>()?;

    Ok(Some(Layout { id, name, is_preset: is_preset == 1, zones }))
}

pub fn list_layouts(db: &Database) -> Result<Vec<LayoutSummary>> {
    let mut stmt = db.conn.prepare(
        "SELECT l.id, l.name, l.is_preset,
                (SELECT COUNT(*) FROM zones z WHERE z.layout_id = l.id) AS zone_count
         FROM layouts l
         ORDER BY l.is_preset DESC, l.name ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(LayoutSummary {
            id: r.get(0)?,
            name: r.get(1)?,
            is_preset: r.get::<_, i32>(2)? == 1,
            zone_count: r.get::<_, i64>(3)? as u32,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::temp_db;

    fn z(i: u32, x: f64, y: f64, w: f64, h: f64) -> ZoneRect {
        ZoneRect { zone_index: i, x, y, w, h }
    }

    #[test]
    fn create_and_get_roundtrips() {
        let mut db = temp_db();
        let id = create_layout(
            &mut db, "Two Columns", false,
            &[
                z(1, 0.0, 0.0, 0.5, 1.0),
                z(2, 0.5, 0.0, 0.5, 1.0),
            ],
        ).unwrap();
        let layout = get_layout(&db, id).unwrap().unwrap();
        assert_eq!(layout.name, "Two Columns");
        assert_eq!(layout.zones.len(), 2);
        assert_eq!(layout.zones[0].zone_index, 1);
        assert_eq!(layout.zones[1].zone_index, 2);
    }

    #[test]
    fn update_replaces_zone_list_atomically() {
        let mut db = temp_db();
        let id = create_layout(&mut db, "X", false, &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap();
        update_layout(&mut db, id, "Y", &[
            z(1, 0.0, 0.0, 0.5, 0.5),
            z(2, 0.5, 0.0, 0.5, 0.5),
            z(3, 0.0, 0.5, 1.0, 0.5),
        ]).unwrap();
        let layout = get_layout(&db, id).unwrap().unwrap();
        assert_eq!(layout.name, "Y");
        assert_eq!(layout.zones.len(), 3);
    }

    #[test]
    fn delete_removes_zones_cascade() {
        let mut db = temp_db();
        let id = create_layout(&mut db, "doomed", false, &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap();
        delete_layout(&mut db, id).unwrap();
        assert!(get_layout(&db, id).unwrap().is_none());
        let count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM zones WHERE layout_id = ?1", params![id], |r| r.get(0)
        ).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn cannot_delete_preset() {
        let mut db = temp_db();
        let id = create_layout(&mut db, "preset", true, &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap();
        let err = delete_layout(&mut db, id).unwrap_err();
        assert!(err.to_string().contains("preset"));
    }

    #[test]
    fn cannot_update_preset() {
        let mut db = temp_db();
        let id = create_layout(&mut db, "preset", true, &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap();
        let err = update_layout(&mut db, id, "hacked", &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap_err();
        assert!(err.to_string().contains("preset"));
    }

    #[test]
    fn list_sorts_presets_first() {
        let mut db = temp_db();
        let _user  = create_layout(&mut db, "AAA user",  false, &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap();
        let _preset = create_layout(&mut db, "ZZZ preset", true, &[z(1, 0.0, 0.0, 1.0, 1.0)]).unwrap();
        let list = list_layouts(&db).unwrap();
        assert_eq!(list[0].name, "ZZZ preset");
        assert_eq!(list[1].name, "AAA user");
    }
}
