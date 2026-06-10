use rusqlite::Connection;

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
        raw        BLOB NOT NULL,
        PRIMARY KEY (server, board, thread_id),
        FOREIGN KEY (server, board, thread_id)
            REFERENCES favorites(server, board, thread_id) ON DELETE CASCADE
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
        conn.execute_batch(SCHEMA).unwrap();
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
            [b"dummy dat bytes".as_slice()],
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
}
