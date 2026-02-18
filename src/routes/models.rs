use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use tracing::{info, warn};

use crate::auth::extract::extract_gh_token;
use crate::copilot::client::fetch_models;
use crate::state::AppState;

pub async fn get_models(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
	// Try to serve from cache first
	{
		let models = state.models.read().await;
		if let Some(m) = models.as_ref() {
			info!(count = m.data.len(), "serving models list from cache");
			return Json(m.clone()).into_response();
		}
	}

	// Cache is empty - fetch on-demand
	info!("models not cached, fetching on-demand");

	// Get a GitHub token from request or default
	let gh_token = extract_gh_token(&headers)
		.map(|s| s.to_string())
		.or_else(|| state.default_github_token.clone());

	let gh_token = match gh_token {
		Some(t) => t,
		None => {
			warn!("no GitHub token available for on-demand model fetch");
			return (
				StatusCode::SERVICE_UNAVAILABLE,
				Json(serde_json::json!({
					"error": {
						"type": "unavailable",
						"message": "models not cached and no GitHub token provided"
					}
				})),
			)
				.into_response();
		}
	};

	// Exchange for copilot token
	let copilot_token = match state
		.token_cache
		.get_copilot_token(&gh_token, &state.client, &state.vscode_version)
		.await
	{
		Ok(t) => t,
		Err(e) => {
			warn!(error = %e, "failed to exchange token for on-demand model fetch");
			return (
				StatusCode::UNAUTHORIZED,
				Json(serde_json::json!({
					"error": {
						"type": "authentication_error",
						"message": format!("token exchange failed: {e}")
					}
				})),
			)
				.into_response();
		}
	};

	// Fetch models
	let mut models = match fetch_models(
		&state.client,
		&copilot_token,
		&state.account_type,
		&state.vscode_version,
	)
	.await
	{
		Ok(m) => m,
		Err(e) => {
			warn!(error = %e, "failed to fetch models on-demand");
			return (
				StatusCode::BAD_GATEWAY,
				Json(serde_json::json!({
					"error": {
						"type": "api_error",
						"message": format!("failed to fetch models: {e}")
					}
				})),
			)
				.into_response();
		}
	};

	// Apply model renaming
	for model in &mut models.data {
		let renamed = state.renamer.rename(&model.id);
		state.renamer.register(&model.id, &renamed);
		if renamed != model.id {
			info!(from = %model.id, to = %renamed, "renamed model");
			model.id = renamed;
		}
	}

	info!(
		count = models.data.len(),
		"fetched and cached models on-demand"
	);

	// Update cache
	*state.models.write().await = Some(models.clone());

	Json(models).into_response()
}
