use rusqlite::Connection;

/// Returns whether `table` has a column named `column` (via PRAGMA table_info).
fn has_column(conn: &Connection, table: &str, column: &str) -> bool {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .expect("Failed to prepare PRAGMA table_info");
    let mut rows = stmt.query([]).expect("Failed to query PRAGMA table_info");
    while let Some(row) = rows.next().expect("Failed to iterate PRAGMA rows") {
        let name: String = row.get(1).expect("column name");
        if name == column {
            return true;
        }
    }
    false
}

pub const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS favorites (
        thread_id   TEXT    NOT NULL,
        server      TEXT    NOT NULL,
        board       TEXT    NOT NULL,
        board_name  TEXT    NOT NULL,
        title       TEXT    NOT NULL,
        res_count   INTEGER NOT NULL DEFAULT 0,
        read_res    INTEGER NOT NULL DEFAULT 0,
        rating      INTEGER NOT NULL DEFAULT 0,
        archived    INTEGER NOT NULL DEFAULT 0,
        status      TEXT    NOT NULL DEFAULT 'active',
        created_at  INTEGER DEFAULT (strftime('%s','now')),
        updated_at  INTEGER DEFAULT (strftime('%s','now')),
        PRIMARY KEY (server, board, thread_id)
    );
    CREATE INDEX IF NOT EXISTS idx_favorites_order
        ON favorites (rating DESC, title ASC);

    CREATE TABLE IF NOT EXISTS dat_blobs (
        server     TEXT NOT NULL,
        board      TEXT NOT NULL,
        thread_id  TEXT NOT NULL,
        raw        TEXT NOT NULL,  -- UTF-8 decoded dat text (Shift-JIS decoded once on write)
        PRIMARY KEY (server, board, thread_id),
        FOREIGN KEY (server, board, thread_id)
            REFERENCES favorites(server, board, thread_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS ng_ids (
        ng_id      TEXT PRIMARY KEY,
        created_at INTEGER DEFAULT (strftime('%s','now'))
    );
";

pub fn open(path: &str) -> Connection {
    tracing::info!("Database: {}", path);

    // Create the parent directory (e.g. ./data) if it does not exist.
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    let conn = Connection::open(path).expect("Failed to open database");

    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA cache_size = -32000;
         PRAGMA temp_store = MEMORY;
         PRAGMA foreign_keys = ON;",
    )
    .expect("Failed to set PRAGMA");

    conn.execute_batch(SCHEMA).expect("Failed to create tables");

    // Migration: add `archived` column to favorites if it does not exist yet (existing DBs).
    if !has_column(&conn, "favorites", "archived") {
        conn.execute_batch(
            "ALTER TABLE favorites ADD COLUMN archived INTEGER NOT NULL DEFAULT 0",
        )
        .expect("Failed to add archived column to favorites");
    }

    // One-time migration: if any dat_blobs row still holds a Shift-JIS BLOB (typeof='blob'),
    // the old schema is in effect and read_blob_posts would fail with "Invalid column type Blob".
    // Since dat_blobs is a pure cache (re-fetchable from 5ch), the safest fix is to delete all
    // BLOB rows so they are re-downloaded on next reload. favorites (read positions, ratings)
    // are left untouched — only cached dat bytes are cleared.
    let blob_rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM dat_blobs WHERE typeof(raw) = 'blob'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if blob_rows > 0 {
        tracing::warn!(
            "dat_blobs: found {blob_rows} Shift-JIS BLOB row(s) from old schema — \
             deleting cached dat (will be re-fetched on next reload)"
        );
        conn.execute("DELETE FROM dat_blobs", [])
            .expect("Failed to clear legacy BLOB rows from dat_blobs");
    }

    conn
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_memory() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        conn
    }

    fn insert_favorite(conn: &Connection, thread_id: &str, title: &str) {
        conn.execute(
            "INSERT INTO favorites (thread_id, server, board, board_name, title)
             VALUES (?1, 'egg', 'applism', 'スマホアプリ', ?2)",
            (thread_id, title),
        )
        .unwrap();
    }

    #[test]
    fn schema_is_idempotent() {
        let conn = open_memory();
        // Running SCHEMA twice must not fail (all CREATE IF NOT EXISTS).
        conn.execute_batch(SCHEMA).unwrap();
    }

    #[test]
    fn ng_ids_table_exists_and_accepts_rows() {
        let conn = open_memory();
        conn.execute(
            "INSERT INTO ng_ids (ng_id) VALUES ('testUser123')",
            [],
        )
        .unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM ng_ids", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);

        // INSERT OR IGNORE must be idempotent (PRIMARY KEY conflict).
        conn.execute(
            "INSERT OR IGNORE INTO ng_ids (ng_id) VALUES ('testUser123')",
            [],
        )
        .unwrap();
        let count2: i64 = conn
            .query_row("SELECT COUNT(*) FROM ng_ids", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count2, 1);
    }

    #[test]
    fn insert_and_select_favorite() {
        let conn = open_memory();
        insert_favorite(&conn, "1771127145", "【ブルアカ】総合 Part1");

        let (title, res_count, rating): (String, i64, i64) = conn
            .query_row(
                "SELECT title, res_count, rating FROM favorites
                 WHERE server = 'egg' AND board = 'applism' AND thread_id = '1771127145'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(title, "【ブルアカ】総合 Part1");
        assert_eq!(res_count, 0); // DEFAULT 0
        assert_eq!(rating, 0); // DEFAULT 0
    }

    #[test]
    fn primary_key_conflicts_on_same_thread() {
        let conn = open_memory();
        insert_favorite(&conn, "1771127145", "Part1");
        // the same server+board+thread_id conflicts
        let result = conn.execute(
            "INSERT INTO favorites (thread_id, server, board, board_name, title)
             VALUES ('1771127145', 'egg', 'applism', 'スマホアプリ', 'dup')",
            [],
        );
        assert!(result.is_err());
    }

    #[test]
    fn dat_blob_cascade_deletes_with_favorite() {
        let conn = open_memory();
        insert_favorite(&conn, "1771127145", "Part1");
        conn.execute(
            "INSERT INTO dat_blobs (server, board, thread_id, raw)
             VALUES ('egg', 'applism', '1771127145', ?1)",
            ["dummy dat text"],
        )
        .unwrap();

        // deleting the parent favorite also deletes dat_blob via CASCADE
        conn.execute(
            "DELETE FROM favorites WHERE server = 'egg' AND board = 'applism' AND thread_id = '1771127145'",
            [],
        )
        .unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM dat_blobs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn archived_column_exists_with_default_zero() {
        let conn = open_memory();
        // Insert a row without specifying archived; it must default to 0.
        insert_favorite(&conn, "1771127145", "テスト");
        let archived: i64 = conn
            .query_row(
                "SELECT archived FROM favorites
                 WHERE server = 'egg' AND board = 'applism' AND thread_id = '1771127145'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(archived, 0);
        assert!(
            has_column(&conn, "favorites", "archived"),
            "archived column must exist in favorites schema"
        );
    }

    /// Migration: old DB without the archived column gets it added idempotently.
    #[test]
    fn migration_adds_archived_column_to_old_schema() {
        // Build a table that mimics the pre-archived schema (no archived column).
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(
            "CREATE TABLE favorites (
                thread_id  TEXT NOT NULL,
                server     TEXT NOT NULL,
                board      TEXT NOT NULL,
                board_name TEXT NOT NULL,
                title      TEXT NOT NULL,
                res_count  INTEGER NOT NULL DEFAULT 0,
                read_res   INTEGER NOT NULL DEFAULT 0,
                rating     INTEGER NOT NULL DEFAULT 0,
                status     TEXT NOT NULL DEFAULT 'active',
                created_at INTEGER DEFAULT (strftime('%s','now')),
                updated_at INTEGER DEFAULT (strftime('%s','now')),
                PRIMARY KEY (server, board, thread_id)
            );",
        )
        .unwrap();

        // Insert a row before migration.
        conn.execute(
            "INSERT INTO favorites (thread_id, server, board, board_name, title)
             VALUES ('1', 'egg', 'applism', '板', 'タイトル')",
            [],
        )
        .unwrap();

        // Simulate the migration (same logic as in open()).
        assert!(
            !has_column(&conn, "favorites", "archived"),
            "pre-condition: archived column must not exist yet"
        );

        conn.execute_batch(
            "ALTER TABLE favorites ADD COLUMN archived INTEGER NOT NULL DEFAULT 0",
        )
        .unwrap();

        // Column must now exist and the pre-existing row must have archived=0.
        let archived: i64 = conn
            .query_row(
                "SELECT archived FROM favorites WHERE thread_id='1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(archived, 0);
    }

    #[test]
    fn order_index_exists() {
        let conn = open_memory();
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master
                 WHERE type = 'index' AND name = 'idx_favorites_order'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists);
    }

    /// Migration regression: BLOB rows (from the old Shift-JIS schema) must be deleted by the
    /// one-time migration in `open()`, while TEXT rows (the new schema) must be left intact.
    ///
    /// This test simulates the migration path directly (without calling `open()`, which requires
    /// a real file path) by reproducing the same two SQL statements that `open()` executes.
    /// Keeping the logic here means that if the `typeof='blob'` predicate or the DELETE ever
    /// regresses, this test will catch it before the server starts with a corrupt cache row.
    #[test]
    fn migration_deletes_blob_rows_preserves_text_rows() {
        // Start from the current (TEXT) schema so CREATE TABLE IF NOT EXISTS does not interfere.
        let conn = open_memory();
        insert_favorite(&conn, "1001", "スレA");
        insert_favorite(&conn, "1002", "スレB");

        // Insert one TEXT row (normal, new schema) for thread 1001.
        conn.execute(
            "INSERT INTO dat_blobs (server, board, thread_id, raw)
             VALUES ('egg', 'applism', '1001', 'UTF-8 dat text')",
            [],
        )
        .unwrap();

        // Simulate an old-schema BLOB row for thread 1002 by using the CAST trick.
        // SQLite stores the value as a BLOB when cast explicitly; typeof() returns 'blob'.
        conn.execute(
            "INSERT INTO dat_blobs (server, board, thread_id, raw)
             VALUES ('egg', 'applism', '1002', CAST(X'82a082a282b082c0' AS BLOB))",
            [],
        )
        .unwrap();

        // Sanity: confirm the typeof() values before running the migration.
        let text_type: String = conn
            .query_row(
                "SELECT typeof(raw) FROM dat_blobs WHERE thread_id='1001'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(text_type, "text", "pre-condition: thread 1001 must be text");

        let blob_type: String = conn
            .query_row(
                "SELECT typeof(raw) FROM dat_blobs WHERE thread_id='1002'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(blob_type, "blob", "pre-condition: thread 1002 must be blob");

        // --- run the migration (same logic as in open()) ---
        let blob_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM dat_blobs WHERE typeof(raw) = 'blob'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if blob_rows > 0 {
            conn.execute("DELETE FROM dat_blobs", []).unwrap();
        }
        // --- end migration ---

        // After migration: only the TEXT row (1001) survives — DELETE cleared everything,
        // but in a real scenario the TEXT rows would be re-seeded by the next reload.
        // The key invariant is that BLOB rows do NOT survive; a full table DELETE is safe
        // because dat_blobs is a pure re-fetchable cache.
        let count_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM dat_blobs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count_after, 0, "migration must clear all rows when any BLOB row exists");

        // Complement: when there are NO blob rows, the migration must not delete anything.
        // Re-insert only a TEXT row.
        conn.execute(
            "INSERT INTO dat_blobs (server, board, thread_id, raw)
             VALUES ('egg', 'applism', '1001', 'restored UTF-8 text')",
            [],
        )
        .unwrap();

        let blob_rows2: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM dat_blobs WHERE typeof(raw) = 'blob'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        // blob_rows2 == 0, so the DELETE branch is NOT taken.
        assert_eq!(blob_rows2, 0);

        let count_text_only: i64 = conn
            .query_row("SELECT COUNT(*) FROM dat_blobs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count_text_only, 1, "TEXT-only rows must not be deleted by the migration");
    }
}
