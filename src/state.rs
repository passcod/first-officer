use crate::auth::cache::TokenCache;
use crate::copilot::types::ModelsResponse;
use crate::rename::ModelRenamer;
use tokio::sync::RwLock;

pub struct AppState {
	pub default_github_token: Option<String>,
	pub account_type: String,
	pub vscode_version: String,
	pub models: RwLock<Option<ModelsResponse>>,
	pub client: reqwest::Client,
	pub renamer: ModelRenamer,
	pub token_cache: TokenCache,
}

impl AppState {
	pub fn new(
		default_github_token: Option<String>,
		account_type: String,
		vscode_version: String,
		renamer: ModelRenamer,
	) -> Self {
		Self {
			default_github_token,
			account_type,
			vscode_version,
			models: RwLock::new(None),
			client: reqwest::Client::new(),
			renamer,
			token_cache: TokenCache::new(),
		}
	}
}
