use crate::config::Config;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

/// 5ch は User-Agent に `Monazilla/1.00` を含まないとブロックされる。
pub const USER_AGENT: &str = "Monazilla/1.00 Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36";

#[derive(Clone)]
pub struct AppState {
    /// 非同期プールは不要 — SQLite は単純な Mutex で十分速い。
    /// ロックガードは {} ブロック内に収めること（.await をまたぐと Send 違反）。
    pub db: Arc<Mutex<Connection>>,
    pub config: Config,
    pub http: reqwest::Client,
}
