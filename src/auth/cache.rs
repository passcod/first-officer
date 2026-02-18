use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::copilot::client::fetch_copilot_token;

/// Buffer in seconds — refresh a token if it expires within this window.
const EXPIRY_BUFFER_SECS: u64 = 120;

struct CachedToken {
	copilot_token: String,
	expires_at: u64,
}

impl CachedToken {
	fn is_valid(&self) -> bool {
		let now = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap()
			.as_secs();
		self.expires_at > now + EXPIRY_BUFFER_SECS
	}
}

/// Per-GH-token cache of short-lived Copilot API tokens.
///
/// Tokens are exchanged lazily on first use and re-exchanged when they
/// expire (or are close to expiring). Multiple concurrent requests with
/// the same GH token may trigger duplicate exchanges — that's harmless
/// since the exchange is idempotent.
pub struct TokenCache {
	entries: RwLock<HashMap<String, CachedToken>>,
}

impl TokenCache {
	pub fn new() -> Self {
		Self {
			entries: RwLock::new(HashMap::new()),
		}
	}

	/// Get a valid Copilot token for the given GH token, exchanging if needed.
	pub async fn get_copilot_token(
		&self,
		gh_token: &str,
		client: &reqwest::Client,
		vscode_version: &str,
	) -> Result<String, reqwest::Error> {
		// Fast path: read lock, check cache
		{
			let cache = self.entries.read().await;
			if let Some(entry) = cache.get(gh_token) {
				if entry.is_valid() {
					return Ok(entry.copilot_token.clone());
				}
				debug!("cached copilot token expired or expiring soon, refreshing");
			}
		}

		// Slow path: exchange and cache
		let resp = fetch_copilot_token(client, gh_token, vscode_version).await?;
		info!(
			expires_at = resp.expires_at,
			refresh_in = resp.refresh_in,
			"copilot token exchanged"
		);

		let copilot_token = resp.token.clone();

		let mut cache = self.entries.write().await;
		cache.insert(
			gh_token.to_string(),
			CachedToken {
				copilot_token: resp.token,
				expires_at: resp.expires_at,
			},
		);

		Ok(copilot_token)
	}

	/// Proactively refresh the token for a specific GH token.
	/// Used by the background refresh loop for the default token.
	pub async fn refresh(
		&self,
		gh_token: &str,
		client: &reqwest::Client,
		vscode_version: &str,
	) -> Result<u64, reqwest::Error> {
		let resp = fetch_copilot_token(client, gh_token, vscode_version).await?;
		let refresh_in = resp.refresh_in;

		let mut cache = self.entries.write().await;
		cache.insert(
			gh_token.to_string(),
			CachedToken {
				copilot_token: resp.token,
				expires_at: resp.expires_at,
			},
		);

		Ok(refresh_in)
	}

	/// Remove expired entries. Call periodically to prevent unbounded growth
	/// if many distinct GH tokens are used.
	pub async fn evict_expired(&self) {
		let now = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap()
			.as_secs();

		let mut cache = self.entries.write().await;
		let before = cache.len();
		cache.retain(|_, entry| entry.expires_at > now);
		let evicted = before - cache.len();
		if evicted > 0 {
			debug!(evicted, remaining = cache.len(), "evicted expired tokens");
		}
	}
}
