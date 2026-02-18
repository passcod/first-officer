use std::sync::Arc;
use std::time::Duration;

use tracing::{error, info};

use crate::state::AppState;

/// Exchange the default GH token for an initial Copilot token and cache it.
/// Returns an error if no default token is configured.
pub async fn initial_token_exchange(state: &AppState) -> anyhow::Result<()> {
	let gh_token = state
		.default_github_token
		.as_deref()
		.ok_or_else(|| anyhow::anyhow!("no default GH_TOKEN configured"))?;

	let copilot_token = state
		.token_cache
		.get_copilot_token(gh_token, &state.client, &state.vscode_version)
		.await?;

	info!(
		token_len = copilot_token.len(),
		"default copilot token acquired"
	);
	Ok(())
}

/// Spawn a background loop that proactively refreshes the Copilot token
/// for the default GH token. Only runs if a default token is configured.
pub fn spawn_refresh_loop(state: Arc<AppState>) {
	let gh_token = match state.default_github_token.clone() {
		Some(t) => t,
		None => return,
	};

	let evict_state = Arc::clone(&state);
	tokio::spawn(async move {
		// Initial delay — the token was just exchanged at startup.
		tokio::time::sleep(Duration::from_secs(600)).await;

		loop {
			let sleep_secs = match state
				.token_cache
				.refresh(&gh_token, &state.client, &state.vscode_version)
				.await
			{
				Ok(refresh_in) => {
					let delay = refresh_in.saturating_sub(60);
					info!(refresh_in, delay, "default copilot token refreshed");
					delay
				}
				Err(e) => {
					error!(error = %e, "failed to refresh default copilot token, retrying in 30s");
					30
				}
			};

			tokio::time::sleep(Duration::from_secs(sleep_secs)).await;
		}
	});

	// Periodically evict expired entries from other (per-request) tokens.
	tokio::spawn(async move {
		loop {
			tokio::time::sleep(Duration::from_secs(300)).await;
			evict_state.token_cache.evict_expired().await;
		}
	});
}
