use goch_viewer::config::Config;
use goch_viewer::state::AppState;
use goch_viewer::{db, routes, sync};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let config = Config::from_env();
    let port = config.port;
    let conn = db::open(&config.db_path);

    let state = AppState::new(conn, config);

    sync::start_sync(state.clone());

    let app = routes::build_router(state);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind");
    tracing::info!("Server running on http://localhost:{port}");
    axum::serve(listener, app).await.expect("Server error");
}
