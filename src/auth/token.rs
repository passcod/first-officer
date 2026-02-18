use std::sync::Arc;
use std::time::Duration;

use tracing::{error, info};

use crate::copilot::client::fetch_copilot_token;
use crate::state::AppState;

pub async fn initial_token_exchange(state: &AppState) -> anyhow::Result<()> {
    let resp = fetch_copilot_token(state).await?;
    let mut token = state.copilot_token.write().await;
    *token = resp.token;
    info!(
        expires_at = resp.expires_at,
        refresh_in = resp.refresh_in,
        "copilot token acquired"
    );
    Ok(())
}

pub fn spawn_refresh_loop(state: Arc<AppState>) {
    tokio::spawn(async move {
        loop {
            let sleep_secs = match fetch_copilot_token(&state).await {
                Ok(resp) => {
                    let mut token = state.copilot_token.write().await;
                    *token = resp.token;
                    let delay = resp.refresh_in.saturating_sub(60);
                    info!(
                        refresh_in = resp.refresh_in,
                        delay, "copilot token refreshed"
                    );
                    delay
                }
                Err(e) => {
                    error!(error = %e, "failed to refresh copilot token, retrying in 30s");
                    30
                }
            };

            tokio::time::sleep(Duration::from_secs(sleep_secs)).await;
        }
    });
}
