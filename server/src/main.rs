use std::net::SocketAddr;
use std::sync::Arc;

use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod api;
mod error;
mod state;
mod store;

use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let data_path =
        std::env::var("RAYVIEW_DATA").unwrap_or_else(|_| "rayview_data.json".to_string());
    let store = store::Store::load_or_create(&data_path)?;
    let state = Arc::new(AppState::new(store));

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = api::router(state.clone())
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let port: u16 = std::env::var("RAYVIEW_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(9631);
    let host = std::env::var("RAYVIEW_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let addr = SocketAddr::new(host.parse()?, port);
    tracing::info!("Rayview Meta server listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}
