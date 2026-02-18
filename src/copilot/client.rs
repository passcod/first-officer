use anyhow::Context;
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
) -> Result<ModelsResponse, anyhow::Error> {
	let base = copilot_base_url(account_type);
	debug!(url = %format!("{base}/models"), "fetching models from Copilot API");
	let headers = copilot_headers(copilot_token, vscode_version, false);
	let resp = client
		.get(format!("{base}/models"))
		.headers(headers)
		.send()
		.await
		.context("failed to send models request")?;

	let status = resp.status();
	debug!(status = %status, "received models response");

	let resp = resp
		.error_for_status()
		.context("models request returned error status")?;

	let body = resp
		.bytes()
		.await
		.context("failed to read models response body")?;
	debug!(size = body.len(), "received models response body");

	let models: ModelsResponse = serde_json::from_slice(&body).map_err(|e| {
		tracing::error!(
			error = %e,
			status = %status,
			body = %String::from_utf8_lossy(&body),
			"failed to parse models response"
		);
		e
	})?;

	debug!("models fetched and parsed successfully");
	Ok(models)
}

pub async fn chat_completions_raw(
	client: &Client,
	copilot_token: &str,
	account_type: &str,
	vscode_version: &str,
	body: &[u8],
	vision: bool,
	is_agent: bool,
) -> Result<reqwest::Response, anyhow::Error> {
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
		.await
		.context("failed to send chat completions request")?;

	let status = resp.status();
	if !status.is_success() {
		let error_body = resp.bytes().await.unwrap_or_default();
		let error_text = String::from_utf8_lossy(&error_body);
		tracing::error!(
			status = %status,
			body = %error_text,
			"Copilot API returned error status"
		);
		anyhow::bail!("HTTP {status}: {error_text}");
	}

	debug!(status = %status, "received chat completions response");
	Ok(resp)
}
