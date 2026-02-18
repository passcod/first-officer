use std::env;
use std::time::{Duration, SystemTime};

use crate::auth::cache::TokenCache;
use crate::copilot::types::ModelsResponse;
use crate::rename::ModelRenamer;
use tokio::sync::RwLock;

pub struct CachedModels {
	pub response: ModelsResponse,
	pub cached_at: SystemTime,
}

pub struct AppState {
	pub default_github_token: Option<String>,
	pub account_type: String,
	pub vscode_version: String,
	pub models: RwLock<Option<CachedModels>>,
	pub models_cache_ttl: Duration,
	pub client: reqwest::Client,
	pub renamer: ModelRenamer,
	pub token_cache: TokenCache,
	pub emulate_thinking: bool,
}

impl AppState {
	pub fn new(
		default_github_token: Option<String>,
		account_type: String,
		vscode_version: String,
		renamer: ModelRenamer,
	) -> Self {
		let emulate_thinking = env::var("EMULATE_THINKING")
			.map(|v| v != "false")
			.unwrap_or(true);

		let models_cache_ttl_secs = env::var("MODELS_CACHE_TTL")
			.ok()
			.and_then(|v| v.parse::<u64>().ok())
			.unwrap_or(3600); // Default: 1 hour

		Self {
			default_github_token,
			account_type,
			vscode_version,
			models: RwLock::new(None),
			client: reqwest::Client::new(),
			renamer,
			token_cache: TokenCache::new(),
			emulate_thinking,
			models_cache_ttl: Duration::from_secs(models_cache_ttl_secs),
		}
	}

	pub fn is_models_cache_valid(&self, cached: &CachedModels) -> bool {
		cached
			.cached_at
			.elapsed()
			.map(|elapsed| elapsed < self.models_cache_ttl)
			.unwrap_or(false)
	}
}
