use std::env;
use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post};
use tower_http::cors::CorsLayer;
use tracing::{error, info};

mod auth;
mod copilot;
mod routes;
mod state;
mod translate;

use auth::token::{initial_token_exchange, spawn_refresh_loop};
use copilot::client::fetch_models;
use state::AppState;

const DEFAULT_VSCODE_VERSION: &str = "1.100.0";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "first_officer=info".parse().unwrap()),
        )
        .init();

    let github_token = env::var("GH_TOKEN").expect("GH_TOKEN environment variable is required");
    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(4141);
    let account_type = env::var("ACCOUNT_TYPE").unwrap_or_else(|_| "individual".to_string());
    let vscode_version =
        env::var("VSCODE_VERSION").unwrap_or_else(|_| DEFAULT_VSCODE_VERSION.to_string());

    let state = Arc::new(AppState::new(github_token, account_type, vscode_version));

    if let Err(e) = initial_token_exchange(&state).await {
        error!(error = %e, "failed to acquire initial copilot token");
        std::process::exit(1);
    }

    match fetch_models(&state).await {
        Ok(models) => {
            let names: Vec<&str> = models.data.iter().map(|m| m.id.as_str()).collect();
            info!(count = models.data.len(), models = ?names, "cached models");
            *state.models.write().await = Some(models);
        }
        Err(e) => {
            error!(error = %e, "failed to fetch models (continuing without cache)");
        }
    }

    spawn_refresh_loop(Arc::clone(&state));

    let app = Router::new()
        .route("/", get(routes::health::health))
        .route(
            "/v1/chat/completions",
            post(routes::completions::post_completions),
        )
        .route(
            "/chat/completions",
            post(routes::completions::post_completions),
        )
        .route("/v1/models", get(routes::models::get_models))
        .route("/models", get(routes::models::get_models))
        .route("/v1/messages", post(routes::messages::post_messages))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("failed to bind");

    info!(port, "first-officer listening");

    axum::serve(listener, app).await.expect("server error");
}
