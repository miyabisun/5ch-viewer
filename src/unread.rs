//! The unread rule shared by every server response (docs/spec.md §8.1):
//! a post is unread when it comes after the read position and is not the
//! user's own. The client applies the same rule to the `own_res` it receives.

use std::collections::HashMap;

/// Thread key used to group own posts.
pub type ThreadKey = (String, String, String);

/// Number of unread posts: after `read_res`, within the saved dat, not own.
#[must_use]
pub fn unread_count(res_count: i64, read_res: i64, own_res: &[i64]) -> i64 {
    let own_unread = own_res
        .iter()
        .filter(|&&n| n > read_res && n <= res_count)
        .fold(0_i64, |count, _| count + 1);
    (res_count - read_res - own_unread).max(0)
}

/// Own post numbers of every thread, ascending.
///
/// # Errors
///
/// Returns the database error when `own_posts` cannot be read.
pub fn own_posts_by_thread(
    conn: &rusqlite::Connection,
) -> rusqlite::Result<HashMap<ThreadKey, Vec<i64>>> {
    let mut stmt =
        conn.prepare("SELECT server, board, thread_id, res_num FROM own_posts ORDER BY res_num")?;
    let rows = stmt.query_map([], |row| {
        Ok(((row.get(0)?, row.get(1)?, row.get(2)?), row.get(3)?))
    })?;
    let mut own: HashMap<ThreadKey, Vec<i64>> = HashMap::new();
    for row in rows {
        let (key, num) = row?;
        own.entry(key).or_default().push(num);
    }
    Ok(own)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};

    #[test]
    fn counts_posts_after_the_read_position() {
        assert_eq!(unread_count(30, 28, &[]), 2);
    }

    #[test]
    fn own_posts_after_the_read_position_are_not_unread() {
        assert_eq!(unread_count(61, 40, &[61]), 20);
        assert_eq!(unread_count(61, 60, &[61]), 0);
    }

    #[test]
    fn own_posts_already_read_or_beyond_the_dat_are_ignored() {
        assert_eq!(unread_count(50, 40, &[12, 40, 51]), 10);
    }

    #[test]
    fn never_negative_when_the_read_position_is_ahead() {
        assert_eq!(unread_count(30, 50, &[]), 0);
    }

    #[test]
    fn groups_own_posts_by_thread_in_ascending_order() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::SCHEMA).unwrap();
        for (thread, num) in [("1", 30), ("1", 12), ("2", 5)] {
            conn.execute(
                "INSERT INTO own_posts (server, board, thread_id, res_num, body)
                 VALUES ('egg', 'applism', ?1, ?2, '')",
                params![thread, num],
            )
            .unwrap();
        }

        let own = own_posts_by_thread(&conn).unwrap();
        let key = |t: &str| ("egg".to_string(), "applism".to_string(), t.to_string());
        assert_eq!(own[&key("1")], vec![12, 30]);
        assert_eq!(own[&key("2")], vec![5]);
        assert_eq!(own.len(), 2);
    }
}
