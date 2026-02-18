use reqwest::Client;

use super::api::{GITHUB_API_BASE_URL, copilot_base_url, copilot_headers, github_headers};
use super::types::{CopilotTokenResponse, ModelsResponse};
use crate::state::AppState;

pub async fn fetch_copilot_token(state: &AppState) -> Result<CopilotTokenResponse, reqwest::Error> {
    let headers = github_headers(&state.github_token, &state.vscode_version);
    state
        .client
        .get(format!("{GITHUB_API_BASE_URL}/copilot_internal/v2/token"))
        .headers(headers)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
}

pub async fn fetch_models(state: &AppState) -> Result<ModelsResponse, reqwest::Error> {
    let token = state.copilot_token.read().await;
    let headers = copilot_headers(&token, &state.vscode_version, false);
    let base = copilot_base_url(&state.account_type);
    state
        .client
        .get(format!("{base}/models"))
        .headers(headers)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
}

pub async fn chat_completions_raw(
    client: &Client,
    copilot_token: &str,
    account_type: &str,
    vscode_version: &str,
    body: &[u8],
    vision: bool,
    is_agent: bool,
) -> Result<reqwest::Response, reqwest::Error> {
    let mut headers = copilot_headers(copilot_token, vscode_version, vision);
    headers.insert(
        "x-initiator",
        if is_agent { "agent" } else { "user" }.parse().unwrap(),
    );
    let base = copilot_base_url(account_type);
    client
        .post(format!("{base}/chat/completions"))
        .headers(headers)
        .body(body.to_vec())
        .send()
        .await?
        .error_for_status()
}
