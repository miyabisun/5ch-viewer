use std::env;

#[derive(Clone)]
pub struct Config {
    pub port: u16,
    pub base_path: String,
    pub db_path: String,
    pub image_cache_dir: String,
    /// Path to the persistent cookie jar file (JSON). Loaded at startup, saved after
    /// each successful post so acorn/MonaTicket survive process restarts.
    pub cookies_path: String,
    /// Origin used to reach 5ch. Empty = production default (`https://{server}.5ch.io`).
    /// When set (e.g. a local mock in integration tests), every board/thread URL is built
    /// against this single origin instead of per-server 5ch.io hosts.
    pub fivech_base_url: String,
}

impl Config {
    pub fn from_env() -> Self {
        let port = env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(3000);

        let base_path = env::var("BASE_PATH")
            .unwrap_or_default()
            .trim_end_matches('/')
            .to_string();

        if !base_path.is_empty() {
            let re = regex_lite::Regex::new(r"^/[\w\-/]*$").unwrap();
            if !re.is_match(&base_path) {
                panic!("Invalid BASE_PATH: {}", base_path);
            }
        }

        let db_path =
            env::var("DATABASE_PATH").unwrap_or_else(|_| "./data/5ch-viewer.db".to_string());

        let image_cache_dir = env::var("IMAGE_CACHE_DIR")
            .ok()
            .map(|path| path.trim().to_string())
            .filter(|path| !path.is_empty())
            .unwrap_or_else(|| {
                std::path::Path::new(&db_path)
                    .parent()
                    .map(|p| p.join("images").to_string_lossy().into_owned())
                    .unwrap_or_else(|| "./images".to_string())
            });

        // Cookie jar persisted alongside the DB (same data directory by default).
        let cookies_path = env::var("COOKIES_PATH").unwrap_or_else(|_| {
            // Derive from db_path: replace the last path component with "cookies.json".
            std::path::Path::new(&db_path)
                .parent()
                .map(|p| p.join("cookies.json").to_string_lossy().into_owned())
                .unwrap_or_else(|| "./data/cookies.json".to_string())
        });

        // FIVECH_BASE_URL overrides the 5ch origin (used by integration tests to point at a mock).
        let fivech_base_url = env::var("FIVECH_BASE_URL")
            .unwrap_or_default()
            .trim_end_matches('/')
            .to_string();

        Self {
            port,
            base_path,
            db_path,
            image_cache_dir,
            cookies_path,
            fivech_base_url,
        }
    }
}
