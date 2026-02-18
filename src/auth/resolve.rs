use std::sync::Arc;

use axum::Json;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use tracing::error;

use super::extract::extract_gh_token;
use crate::state::AppState;

/// Resolve a valid Copilot API token for this request.
///
/// 1. Check request headers for a GitHub token (Anthropic / OpenAI / Bearer conventions).
/// 2. Fall back to the default `GH_TOKEN` from the environment.
/// 3. If neither is available, return 403.
/// 4. Exchange the GH token for a short-lived Copilot token (cached).
pub async fn resolve_copilot_token(
	state: &Arc<AppState>,
	headers: &HeaderMap,
) -> Result<String, Response> {
	let gh_token = extract_gh_token(headers)
        .map(|s| s.to_string())
        .or_else(|| state.default_github_token.clone())
        .ok_or_else(|| {
            (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "type": "error",
                    "error": {
                        "type": "authentication_error",
                        "message": "no GitHub token provided — set GH_TOKEN or pass a token via x-api-key / Authorization header"
                    }
                })),
            )
                .into_response()
        })?;

	state
		.token_cache
		.get_copilot_token(&gh_token, &state.client, &state.vscode_version)
		.await
		.map_err(|e| {
			error!(error = %e, "copilot token exchange failed");
			(
				StatusCode::UNAUTHORIZED,
				Json(serde_json::json!({
					"type": "error",
					"error": {
						"type": "authentication_error",
						"message": format!("copilot token exchange failed: {e}")
					}
				})),
			)
				.into_response()
		})
}
