use crate::config::Config;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

/// 5ch blocks requests whose User-Agent does not contain `Monazilla/1.00`.
pub const USER_AGENT: &str = "Monazilla/1.00 Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36";

#[derive(Clone)]
pub struct AppState {
    /// No async pool needed — SQLite is fast enough behind a simple Mutex.
    /// Keep the lock guard inside a {} block (holding it across .await violates Send).
    pub db: Arc<Mutex<Connection>>,
    pub config: Config,
    pub http: reqwest::Client,
}
