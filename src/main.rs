mod config;
mod db;
mod error;
mod goch;
mod models;
mod routes;
mod sanitize;
mod spa;
mod state;
mod sync;

use config::Config;
use state::AppState;
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env();
    let conn = db::open(&config.db_path);
    let http = reqwest::Client::builder()
        .user_agent(state::USER_AGENT)
        .build()
        .expect("Failed to build HTTP client");

    let state = AppState {
        db: Arc::new(Mutex::new(conn)),
        config: config.clone(),
        http,
    };

    sync::start_sync(state.clone());

    let app = routes::build_router(state);

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind");
    tracing::info!("Server running on http://localhost:{}", config.port);
    axum::serve(listener, app).await.expect("Server error");
}
