use tokio::sync::RwLock;

use crate::copilot::types::ModelsResponse;

pub struct AppState {
    pub github_token: String,
    pub copilot_token: RwLock<String>,
    pub account_type: String,
    pub vscode_version: String,
    pub models: RwLock<Option<ModelsResponse>>,
    pub client: reqwest::Client,
}

impl AppState {
    pub fn new(github_token: String, account_type: String, vscode_version: String) -> Self {
        Self {
            github_token,
            copilot_token: RwLock::new(String::new()),
            account_type,
            vscode_version,
            models: RwLock::new(None),
            client: reqwest::Client::new(),
        }
    }
}
