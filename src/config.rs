use std::env;

#[derive(Clone)]
pub struct Config {
    pub port: u16,
    pub base_path: String,
    pub db_path: String,
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

        Self {
            port,
            base_path,
            db_path,
        }
    }
}
