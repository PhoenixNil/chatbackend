mod config;
mod entities;
mod errors;
mod models;
mod repository;
mod rooms;
mod routes;
mod service;
mod state;
mod validation;
mod websocket;

use std::net::SocketAddr;

use config::Config;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tower_http=debug")),
        )
        .with_target(true)
        .compact()
        .init();

    let config = Config::from_env();
    let state = state::SharedAppState::new(state::AppState::new(&config).await?);
    let app = routes::router(state);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "chat backend listening");

    axum::serve(listener, app).await?;
    Ok(())
}
