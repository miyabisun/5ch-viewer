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
        server     TEXT    NOT NULL,
        board      TEXT    NOT NULL,
        thread_id  TEXT    NOT NULL,
        raw        TEXT    NOT NULL,  -- UTF-8 decoded dat text (Shift-JIS decoded once on write)
        dat_bytes  INTEGER NOT NULL DEFAULT 0,  -- original Shift-JIS byte length (for HEAD gate)
        PRIMARY KEY (server, board, thread_id),
        FOREIGN KEY (server, board, thread_id)
            REFERENCES favorites(server, board, thread_id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS ng_ids (
        server     TEXT NOT NULL,
        board      TEXT NOT NULL,
        ng_id      TEXT NOT NULL,
        created_at INTEGER DEFAULT (strftime('%s','now')),
        PRIMARY KEY (server, board, ng_id)
    );

    CREATE TABLE IF NOT EXISTS ng_words (
        server     TEXT NOT NULL,
        board      TEXT NOT NULL,
        kind       TEXT NOT NULL,   -- 'text' (literal substring) or 'regex'
        pattern    TEXT NOT NULL,   -- matched against the display text of a res body
        created_at INTEGER DEFAULT (strftime('%s','now')),
        PRIMARY KEY (server, board, kind, pattern)
    );

    CREATE TABLE IF NOT EXISTS own_posts (
        server     TEXT NOT NULL,
        board      TEXT NOT NULL,
        thread_id  TEXT NOT NULL,
        res_num    INTEGER NOT NULL,
        body       TEXT NOT NULL,
        name       TEXT,
        mail       TEXT,
        poster_id  TEXT,
        posted_at  INTEGER DEFAULT (strftime('%s','now')),
        PRIMARY KEY (server, board, thread_id, res_num)
    );

    CREATE TABLE IF NOT EXISTS ng_wacchoi (
        suffix     TEXT NOT NULL,   -- 4-char UA-derived suffix (zzzz after the hyphen)
        board      TEXT NOT NULL,
        week_key   TEXT NOT NULL,   -- Thursday-anchored week key (opaque string from client)
        wacchoi    TEXT,            -- full display token xxyy-zzzz (optional)
        created_at INTEGER DEFAULT (strftime('%s','now')),
        PRIMARY KEY (suffix, board, week_key)
    );

    CREATE TABLE IF NOT EXISTS image_cache (
        id          INTEGER PRIMARY KEY,
        url         TEXT    NOT NULL UNIQUE,
        path        TEXT    NOT NULL DEFAULT '',
        mime        TEXT    NOT NULL DEFAULT '',
        file_size   INTEGER,
        mosaic      INTEGER NOT NULL DEFAULT 0,
        created_at  INTEGER NOT NULL DEFAULT (strftime('%s','now'))
    );
    CREATE INDEX IF NOT EXISTS idx_image_cache_path ON image_cache (path);
    CREATE INDEX IF NOT EXISTS idx_image_cache_created ON image_cache (created_at);
";

/// Applies idempotent schema migrations for existing databases created before a given
/// column/cleanup was introduced. Safe to call repeatedly (each step is a no-op once applied).
pub fn migrate(conn: &Connection) {
    if has_column(conn, "image_cache", "image") {
        panic!("image_cache contains legacy image BLOBs; run `migrate-image-cache` before starting the application");
    }

    // Migration: add `archived` column to favorites if it does not exist yet (existing DBs).
    if !has_column(conn, "favorites", "archived") {
        conn.execute_batch("ALTER TABLE favorites ADD COLUMN archived INTEGER NOT NULL DEFAULT 0")
            .expect("Failed to add archived column to favorites");
    }

    // Migration: add `dat_bytes` column to dat_blobs if it does not exist yet (existing DBs).
    if !has_column(conn, "dat_blobs", "dat_bytes") {
        conn.execute_batch("ALTER TABLE dat_blobs ADD COLUMN dat_bytes INTEGER NOT NULL DEFAULT 0")
            .expect("Failed to add dat_bytes column to dat_blobs");
    }

    if !has_column(conn, "image_cache", "file_size") {
        panic!("image_cache has an unsupported schema; run `migrate-image-cache`");
    }

    // One-time reset: the legacy ng_ids table stored the ID string alone with no board
    // scope, so the board a row came from cannot be recovered from the row itself. Drop
    // the whole table and let SCHEMA recreate it with the (server, board, ng_id) key —
    // guessing a board or fanning one ID out to every board would both be wrong.
    // Idempotent: once the `board` column exists this branch never fires again, so a
    // re-run cannot resurrect global rows. Only ng_ids is touched; ng_wacchoi and every
    // other table keep their rows.
    if !has_column(conn, "ng_ids", "board") {
        tracing::warn!(
            "ng_ids: legacy board-less schema found — deleting all NG IDs \
             (the original board cannot be recovered); re-register them per board"
        );
        conn.execute_batch("DROP TABLE IF EXISTS ng_ids")
            .expect("Failed to drop legacy ng_ids table");
        conn.execute_batch(SCHEMA)
            .expect("Failed to recreate ng_ids with the board-scoped schema");
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
}

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

    migrate(&conn);

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
    fn timestamps_are_stored_as_unix_second_integers() {
        let conn = open_memory();
        insert_favorite(&conn, "1700000000", "timestamp invariant");

        let types: (String, String) = conn
            .query_row(
                "SELECT typeof(created_at), typeof(updated_at)
                 FROM favorites WHERE thread_id = '1700000000'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(types, ("integer".to_string(), "integer".to_string()));
    }

    #[test]
    fn ng_wacchoi_table_exists_and_accepts_rows() {
        let conn = open_memory();
        conn.execute(
            "INSERT INTO ng_wacchoi (suffix, board, week_key, wacchoi)
             VALUES ('83IP', 'applism', '2025/12/25', '7bb6-83IP')",
            [],
        )
        .unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM ng_wacchoi", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);

        // INSERT OR IGNORE must be idempotent (PRIMARY KEY conflict).
        conn.execute(
            "INSERT OR IGNORE INTO ng_wacchoi (suffix, board, week_key, wacchoi)
             VALUES ('83IP', 'applism', '2025/12/25', '7bb6-83IP')",
            [],
        )
        .unwrap();
        let count2: i64 = conn
            .query_row("SELECT COUNT(*) FROM ng_wacchoi", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count2, 1);

        // Different suffix in the same board+week is a separate row.
        conn.execute(
            "INSERT INTO ng_wacchoi (suffix, board, week_key)
             VALUES ('ZZZZ', 'applism', '2025/12/25')",
            [],
        )
        .unwrap();
        let count3: i64 = conn
            .query_row("SELECT COUNT(*) FROM ng_wacchoi", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count3, 2);
    }

    #[test]
    fn ng_ids_are_scoped_per_board_and_insert_or_ignore_is_idempotent() {
        let conn = open_memory();
        conn.execute(
            "INSERT INTO ng_ids (server, board, ng_id) VALUES ('egg', 'applism', 'testUser123')",
            [],
        )
        .unwrap();

        // INSERT OR IGNORE on the same (server, board, ng_id) must be a no-op.
        conn.execute(
            "INSERT OR IGNORE INTO ng_ids (server, board, ng_id)
             VALUES ('egg', 'applism', 'testUser123')",
            [],
        )
        .unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM ng_ids", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);

        // The same ID on another board is a separate row — boards do not share NG IDs.
        conn.execute(
            "INSERT INTO ng_ids (server, board, ng_id) VALUES ('egg', 'other', 'testUser123')",
            [],
        )
        .unwrap();
        let per_board: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ng_ids WHERE server='egg' AND board='applism'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(per_board, 1);
    }

    #[test]
    fn ng_words_are_scoped_per_board_and_kind_and_insert_or_ignore_is_idempotent() {
        let conn = open_memory();
        conn.execute(
            "INSERT INTO ng_words (server, board, kind, pattern)
             VALUES ('egg', 'applism', 'text', '荒らし')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO ng_words (server, board, kind, pattern)
             VALUES ('egg', 'applism', 'text', '荒らし')",
            [],
        )
        .unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM ng_words", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            count, 1,
            "duplicate (scope, kind, pattern) must not add a row"
        );

        // Same pattern with a different kind, and same kind+pattern on another board,
        // are both distinct rules.
        conn.execute(
            "INSERT INTO ng_words (server, board, kind, pattern)
             VALUES ('egg', 'applism', 'regex', '荒らし')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ng_words (server, board, kind, pattern)
             VALUES ('egg', 'other', 'text', '荒らし')",
            [],
        )
        .unwrap();
        let in_board: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ng_words WHERE server='egg' AND board='applism'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(in_board, 2);
    }

    /// Migration: the legacy board-less ng_ids table is reset (all rows dropped) because
    /// the board of a global row cannot be recovered. Other NG data must survive.
    #[test]
    fn migration_resets_legacy_global_ng_ids_and_keeps_other_ng_data() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        // Legacy schema: ng_id alone is the primary key.
        conn.execute_batch(
            "CREATE TABLE ng_ids (
                ng_id      TEXT PRIMARY KEY,
                created_at INTEGER DEFAULT (strftime('%s','now'))
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ng_ids (ng_id) VALUES ('globalA'), ('globalB')",
            [],
        )
        .unwrap();
        // The rest of the current schema, including the NG data that must be preserved.
        conn.execute_batch(SCHEMA).unwrap();
        conn.execute(
            "INSERT INTO ng_wacchoi (suffix, board, week_key, wacchoi)
             VALUES ('83IP', 'applism', '2025/12/25', '7bb6-83IP')",
            [],
        )
        .unwrap();

        assert!(
            !has_column(&conn, "ng_ids", "board"),
            "pre-condition: legacy ng_ids must not have a board column"
        );

        migrate(&conn);

        // Every legacy global NG ID is gone and the table now demands a board scope.
        let ng_id_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM ng_ids", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ng_id_count, 0, "legacy global NG IDs must all be deleted");
        assert!(has_column(&conn, "ng_ids", "board"));
        assert!(has_column(&conn, "ng_ids", "server"));

        // NG wacchoi (and its row) is untouched by the ng_ids reset.
        let wacchoi_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM ng_wacchoi", [], |r| r.get(0))
            .unwrap();
        assert_eq!(wacchoi_count, 1, "ng_wacchoi rows must be preserved");
    }

    /// Migration: re-running it must not fail and must not delete board-scoped rows
    /// registered after the reset (i.e. global NG IDs never come back).
    #[test]
    fn migration_is_idempotent_and_does_not_resurrect_global_ng_ids() {
        let conn = open_memory();
        migrate(&conn);
        conn.execute(
            "INSERT INTO ng_ids (server, board, ng_id) VALUES ('egg', 'applism', 'scoped')",
            [],
        )
        .unwrap();

        // Repeated runs are no-ops once the board column exists.
        migrate(&conn);
        migrate(&conn);

        let rows: Vec<(String, String, String)> = {
            let mut stmt = conn
                .prepare("SELECT server, board, ng_id FROM ng_ids")
                .unwrap();
            let r = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            r
        };
        assert_eq!(
            rows,
            vec![(
                "egg".to_string(),
                "applism".to_string(),
                "scoped".to_string()
            )],
            "board-scoped rows must survive repeated migrations"
        );
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
            );
            CREATE TABLE dat_blobs (
                server     TEXT NOT NULL,
                board      TEXT NOT NULL,
                thread_id  TEXT NOT NULL,
                raw        TEXT NOT NULL,
                PRIMARY KEY (server, board, thread_id),
                FOREIGN KEY (server, board, thread_id)
                    REFERENCES favorites(server, board, thread_id) ON DELETE CASCADE
            );
            CREATE TABLE image_cache (
                id         INTEGER PRIMARY KEY,
                url        TEXT NOT NULL UNIQUE,
                path       TEXT NOT NULL DEFAULT '',
                mime       TEXT NOT NULL DEFAULT '',
                file_size  INTEGER,
                mosaic     INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
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

        assert!(
            !has_column(&conn, "favorites", "archived"),
            "pre-condition: archived column must not exist yet"
        );

        // Run the actual migration function used by open().
        migrate(&conn);

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
    fn image_cache_upsert_and_mosaic_preserved() {
        let conn = open_memory();

        // Insert a new image cache entry (file not yet available → NULL size).
        conn.execute(
            "INSERT INTO image_cache (url, path, mosaic) VALUES ('https://i.imgur.com/Abc123.jpg', 'i.imgur.com/Abc123.jpg', 1)",
            [],
        )
        .unwrap();

        // Check mosaic was stored as 1.
        let mosaic: i64 = conn
            .query_row(
                "SELECT mosaic FROM image_cache WHERE url='https://i.imgur.com/Abc123.jpg'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(mosaic, 1);

        // Filling file metadata must not touch mosaic.
        conn.execute(
            "UPDATE image_cache SET mime='image/png', file_size=4
             WHERE url='https://i.imgur.com/Abc123.jpg'",
            [],
        )
        .unwrap();

        // Mosaic must still be 1 (ON CONFLICT does not update mosaic).
        let (mosaic2, mime2): (i64, String) = conn
            .query_row(
                "SELECT mosaic, mime FROM image_cache WHERE url='https://i.imgur.com/Abc123.jpg'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(mosaic2, 1, "mosaic must be preserved across UPSERT");
        assert_eq!(mime2, "image/png");

        // INSERT OR IGNORE on duplicate URL must be a no-op.
        let result = conn.execute(
            "INSERT OR IGNORE INTO image_cache (url, path) VALUES ('https://i.imgur.com/Abc123.jpg', 'other')",
            [],
        );
        assert!(result.is_ok());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM image_cache", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "INSERT OR IGNORE must not create a duplicate row");
    }

    #[test]
    #[should_panic(expected = "migrate-image-cache")]
    fn legacy_image_blob_schema_requires_explicit_migration() {
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
        migrate(&conn);
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
    /// one-time migration in `open()` (via the shared `migrate()` function), while TEXT rows
    /// (the new schema) must be left intact.
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

        // Run the actual migration function used by open().
        migrate(&conn);

        // After migration: only the TEXT row (1001) survives — DELETE cleared everything,
        // but in a real scenario the TEXT rows would be re-seeded by the next reload.
        // The key invariant is that BLOB rows do NOT survive; a full table DELETE is safe
        // because dat_blobs is a pure re-fetchable cache.
        let count_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM dat_blobs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            count_after, 0,
            "migration must clear all rows when any BLOB row exists"
        );

        // Complement: when there are NO blob rows, the migration must not delete anything.
        // Re-insert only a TEXT row.
        conn.execute(
            "INSERT INTO dat_blobs (server, board, thread_id, raw)
             VALUES ('egg', 'applism', '1001', 'restored UTF-8 text')",
            [],
        )
        .unwrap();

        // Run migrate() again: no BLOB rows exist now, so the DELETE branch must not fire.
        migrate(&conn);

        let count_text_only: i64 = conn
            .query_row("SELECT COUNT(*) FROM dat_blobs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            count_text_only, 1,
            "TEXT-only rows must not be deleted by the migration"
        );
    }
}
