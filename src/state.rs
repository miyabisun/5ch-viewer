use crate::config::Config;
use rusqlite::Connection;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

/// 5ch blocks requests whose User-Agent does not contain `Monazilla/1.00`.
pub const USER_AGENT: &str = "Monazilla/1.00 Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36";

/// Builds the shared HTTP client with the Monazilla User-Agent 5ch requires.
pub fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .expect("Failed to build HTTP client")
}

/// Identifies a single thread's dat (server/board/thread_id).
pub type DatKey = (String, String, String);

#[derive(Clone)]
pub struct AppState {
    /// No async pool needed — SQLite is fast enough behind a simple Mutex.
    /// Keep the lock guard inside a {} block (holding it across .await violates Send).
    pub db: Arc<Mutex<Connection>>,
    pub config: Config,
    pub http: reqwest::Client,
    /// Dats currently being fetched from 5ch. Both the foreground viewer reload and the
    /// background board prefetch consult this set so a given dat is never downloaded twice
    /// concurrently (e.g. opening a thread reloads it *and* prefetches its whole board).
    pub inflight: Arc<Mutex<HashSet<DatKey>>>,
}

impl AppState {
    /// Builds the shared state with a fresh HTTP client and an empty in-flight set. Callers
    /// supply only the DB connection and config; the inflight guard set is an internal detail.
    pub fn new(db: Connection, config: Config) -> Self {
        AppState {
            db: Arc::new(Mutex::new(db)),
            config,
            http: build_http_client(),
            inflight: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Claims a dat for fetching. Returns an RAII guard when this caller won the claim, or
    /// `None` when another task is already fetching it (so the caller should skip). The guard
    /// removes the key on drop, so a panic or early return cannot leak a permanent claim.
    pub fn claim_dat(&self, key: &DatKey) -> Option<InflightGuard> {
        let mut set = self.inflight.lock().unwrap();
        if set.contains(key) {
            return None;
        }
        set.insert(key.clone());
        Some(InflightGuard {
            inflight: self.inflight.clone(),
            key: key.clone(),
        })
    }
}

/// Releases an in-flight dat claim when dropped.
pub struct InflightGuard {
    inflight: Arc<Mutex<HashSet<DatKey>>>,
    key: DatKey,
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        self.inflight.lock().unwrap().remove(&self.key);
    }
}
