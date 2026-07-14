use rusqlite::{params, Connection, OpenFlags, TransactionBehavior};
use std::path::Path;
use std::time::Duration;
use viewer_of_5ch::config::Config;
use viewer_of_5ch::image_cache::{self, write_verified};

type AnyError = Box<dyn std::error::Error>;

#[derive(Debug, PartialEq)]
enum MigrationOutcome {
    Migrated,
    Audited,
}

fn has_column(conn: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    for value in stmt.query_map([], |r| r.get::<_, String>(1))? {
        if value? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn migrate_image_cache(conn: &mut Connection, root: &Path) -> Result<MigrationOutcome, AnyError> {
    if !has_column(conn, "image_cache", "image")? {
        if !has_column(conn, "image_cache", "file_size")? {
            return Err("image_cache table is absent or unsupported".into());
        }
        let mut stmt = conn.prepare("SELECT id, file_size FROM image_cache")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?))
        })?;
        for row in rows {
            let (id, size) = row?;
            if size.is_some() && !image_cache::audit_file(root, id, size)? {
                return Err(format!("image cache audit failed for id {id}").into());
            }
        }
        return Ok(MigrationOutcome::Audited);
    }

    conn.busy_timeout(Duration::from_secs(5))?;
    // Hold the write reservation from the first legacy read through the schema switch.
    // A mistakenly running old application can still read, but cannot add or replace rows.
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut stmt = tx.prepare(
        "SELECT rowid, url, path, image, mime, mosaic, created_at FROM image_cache ORDER BY rowid",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<Vec<u8>>>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, i64>(6)?,
        ))
    })?;
    let mut metadata = Vec::new();
    for row in rows {
        let (id, url, old_path, blob, mime, mosaic, created_at) = row?;
        let path = if old_path.is_empty() {
            viewer_of_5ch::fivech::images::normalize_image_path(&url).unwrap_or_default()
        } else {
            old_path
        };
        let size = match blob {
            Some(bytes) => {
                write_verified(root, id, &bytes)?;
                Some(bytes.len() as i64)
            }
            None => None,
        };
        metadata.push((id, url, path, mime, size, mosaic, created_at));
    }
    drop(stmt);

    tx.execute_batch(
        "CREATE TABLE image_cache_new (
            id INTEGER PRIMARY KEY,
            url TEXT NOT NULL UNIQUE,
            path TEXT NOT NULL DEFAULT '',
            mime TEXT NOT NULL DEFAULT '',
            file_size INTEGER,
            mosaic INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
        );",
    )?;
    for (id, url, path, mime, size, mosaic, created_at) in metadata {
        tx.execute(
            "INSERT INTO image_cache_new
             (id, url, path, mime, file_size, mosaic, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, url, path, mime, size, mosaic, created_at],
        )?;
    }
    tx.execute_batch(
        "DROP TABLE image_cache;
         ALTER TABLE image_cache_new RENAME TO image_cache;
         CREATE INDEX idx_image_cache_path ON image_cache(path);
         CREATE INDEX idx_image_cache_created ON image_cache(created_at);",
    )?;
    tx.commit()?;
    Ok(MigrationOutcome::Migrated)
}

fn main() -> Result<(), AnyError> {
    dotenvy::dotenv().ok();
    let config = Config::from_env();
    let mut conn = Connection::open_with_flags(&config.db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    match migrate_image_cache(&mut conn, Path::new(&config.image_cache_dir))? {
        MigrationOutcome::Migrated => println!(
            "image_cache migration completed: filesystem files verified and schema replaced"
        ),
        MigrationOutcome::Audited => {
            println!("image_cache already uses the filesystem schema; audit succeeded")
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

    fn fixture(name: &str) -> (Connection, std::path::PathBuf) {
        let unique = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "fivech-migration-{name}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE image_cache (
                url TEXT NOT NULL PRIMARY KEY,
                path TEXT NOT NULL DEFAULT '',
                image BLOB,
                mime TEXT NOT NULL DEFAULT '',
                mosaic INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT 0
            );",
        )
        .unwrap();
        (conn, root)
    }

    #[test]
    fn migrates_blob_and_null_rows_and_is_rerunnable() {
        let (mut conn, root) = fixture("success");
        conn.execute(
            "INSERT INTO image_cache VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                "https://example.com/a.png",
                "example.com/a.png",
                b"png",
                "image/png",
                1,
                123
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO image_cache VALUES (?1, ?2, NULL, '', 0, 124)",
            params!["https://example.com/b.jpg", "example.com/b.jpg"],
        )
        .unwrap();

        assert_eq!(
            migrate_image_cache(&mut conn, &root).unwrap(),
            MigrationOutcome::Migrated
        );
        assert!(!has_column(&conn, "image_cache", "image").unwrap());
        assert_eq!(
            conn.query_row(
                "SELECT path, mime, file_size, mosaic, created_at FROM image_cache WHERE id=1",
                [],
                |r| Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                )),
            )
            .unwrap(),
            ("example.com/a.png".into(), "image/png".into(), 3, 1, 123)
        );
        assert_eq!(image_cache::read_verified(&root, 1, 3).unwrap(), b"png");
        assert_eq!(
            migrate_image_cache(&mut conn, &root).unwrap(),
            MigrationOutcome::Audited
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn conflicting_existing_file_keeps_legacy_schema() {
        let (mut conn, root) = fixture("conflict");
        conn.execute(
            "INSERT INTO image_cache VALUES (?1, ?2, ?3, 'image/png', 0, 1)",
            params!["https://example.com/a.png", "example.com/a.png", b"new"],
        )
        .unwrap();
        write_verified(&root, 1, b"old").unwrap();

        assert!(migrate_image_cache(&mut conn, &root).is_err());
        assert!(has_column(&conn, "image_cache", "image").unwrap());
        let blob: Vec<u8> = conn
            .query_row("SELECT image FROM image_cache", [], |r| r.get(0))
            .unwrap();
        assert_eq!(blob, b"new");
        let _ = fs::remove_dir_all(root);
    }
}
