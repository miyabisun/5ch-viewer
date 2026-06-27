use viewer_of_5ch::config::Config;
use viewer_of_5ch::state::AppState;
use viewer_of_5ch::{db, routes, sync};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    // Warn if FIVECH_ALLOW_LOOPBACK_FOR_TEST is set in a release build — it has no effect
    // there (guarded by cfg!(debug_assertions) in is_safe_ip), but the operator may be
    // confused about the security posture.
    if !cfg!(debug_assertions) && std::env::var("FIVECH_ALLOW_LOOPBACK_FOR_TEST").is_ok() {
        tracing::warn!(
            "FIVECH_ALLOW_LOOPBACK_FOR_TEST is set in a release build; it has NO effect. \
             This variable is only honored in debug builds for integration tests."
        );
    }

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
