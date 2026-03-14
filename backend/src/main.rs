use std::net::SocketAddr;
use axum::Router;
use tower_http::cors::{CorsLayer, Any};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod error;
mod models;
mod services;
mod api;

#[tokio::main]
async fn main() {
    // Load .env
    dotenvy::from_filename("../.env").ok();
    dotenvy::dotenv().ok();

    // Init tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "mirofish_backend=debug,tower_http=debug".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("MiroFish Backend (Rust) starting...");

    // Validate config
    let config = config::Config::from_env();
    if let Err(errors) = config.validate() {
        for e in &errors {
            tracing::warn!("Config warning: {}", e);
        }
    }

    // Create app state
    let state = api::AppState::new(config);

    // Ensure upload directories exist
    tokio::fs::create_dir_all(&state.config.upload_folder).await.ok();
    tokio::fs::create_dir_all(&state.config.simulation_data_dir).await.ok();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .nest("/api/graph", api::graph::router())
        .nest("/api/simulation", api::simulation::router())
        .nest("/api/report", api::report::router())
        .route("/health", axum::routing::get(|| async {
            axum::Json(serde_json::json!({"status": "ok", "service": "MiroFish Backend (Rust)"}))
        }))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 5001));
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
