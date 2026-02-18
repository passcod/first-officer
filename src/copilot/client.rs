use reqwest::Client;
use tracing::debug;

use super::api::{GITHUB_API_BASE_URL, copilot_base_url, copilot_headers, github_headers};
use super::types::{CopilotTokenResponse, ModelsResponse};

pub async fn fetch_copilot_token(
	client: &Client,
	gh_token: &str,
	vscode_version: &str,
) -> Result<CopilotTokenResponse, reqwest::Error> {
	debug!("fetching copilot token from GitHub API");
	let headers = github_headers(gh_token, vscode_version);
	let resp = client
		.get(format!("{GITHUB_API_BASE_URL}/copilot_internal/v2/token"))
		.headers(headers)
		.send()
		.await?
		.error_for_status()?
		.json()
		.await?;
	debug!("copilot token fetched successfully");
	Ok(resp)
}

pub async fn fetch_models(
	client: &Client,
	copilot_token: &str,
	account_type: &str,
	vscode_version: &str,
) -> Result<ModelsResponse, reqwest::Error> {
	let base = copilot_base_url(account_type);
	debug!(url = %format!("{base}/models"), "fetching models from Copilot API");
	let headers = copilot_headers(copilot_token, vscode_version, false);
	let resp = client
		.get(format!("{base}/models"))
		.headers(headers)
		.send()
		.await?
		.error_for_status()?
		.json()
		.await?;
	debug!("models fetched successfully");
	Ok(resp)
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
	let base = copilot_base_url(account_type);
	debug!(
		url = %format!("{base}/chat/completions"),
		body_size = body.len(),
		vision = vision,
		agent = is_agent,
		"sending chat completions request to Copilot API"
	);
	let mut headers = copilot_headers(copilot_token, vscode_version, vision);
	headers.insert(
		"x-initiator",
		if is_agent { "agent" } else { "user" }.parse().unwrap(),
	);
	let resp = client
		.post(format!("{base}/chat/completions"))
		.headers(headers)
		.body(body.to_vec())
		.send()
		.await?
		.error_for_status()?;
	debug!(status = %resp.status(), "received chat completions response");
	Ok(resp)
}
