use reqwest::header::{HeaderMap, HeaderValue};
use uuid::Uuid;

const EDITOR_PLUGIN_VERSION: &str = "copilot-chat/0.26.7";
const USER_AGENT: &str = "GitHubCopilotChat/0.26.7";
const API_VERSION: &str = "2025-04-01";

pub const GITHUB_API_BASE_URL: &str = "https://api.github.com";

pub fn copilot_base_url(account_type: &str) -> String {
	match account_type {
		"individual" => "https://api.githubcopilot.com".to_string(),
		other => format!("https://api.{other}.githubcopilot.com"),
	}
}

pub fn copilot_headers(copilot_token: &str, vscode_version: &str, vision: bool) -> HeaderMap {
	let mut headers = HeaderMap::new();
	headers.insert(
		"authorization",
		format!("Bearer {copilot_token}").parse().unwrap(),
	);
	headers.insert("content-type", HeaderValue::from_static("application/json"));
	headers.insert(
		"copilot-integration-id",
		HeaderValue::from_static("vscode-chat"),
	);
	headers.insert(
		"editor-version",
		format!("vscode/{vscode_version}").parse().unwrap(),
	);
	headers.insert(
		"editor-plugin-version",
		HeaderValue::from_static(EDITOR_PLUGIN_VERSION),
	);
	headers.insert("user-agent", HeaderValue::from_static(USER_AGENT));
	headers.insert(
		"openai-intent",
		HeaderValue::from_static("conversation-panel"),
	);
	headers.insert(
		"x-github-api-version",
		HeaderValue::from_static(API_VERSION),
	);
	headers.insert(
		"x-request-id",
		HeaderValue::from_str(&Uuid::new_v4().to_string()).unwrap(),
	);
	headers.insert(
		"x-vscode-user-agent-library-version",
		HeaderValue::from_static("electron-fetch"),
	);
	if vision {
		headers.insert("copilot-vision-request", HeaderValue::from_static("true"));
	}
	headers
}

pub fn github_headers(github_token: &str, vscode_version: &str) -> HeaderMap {
	let mut headers = HeaderMap::new();
	headers.insert("content-type", HeaderValue::from_static("application/json"));
	headers.insert("accept", HeaderValue::from_static("application/json"));
	headers.insert(
		"authorization",
		format!("token {github_token}").parse().unwrap(),
	);
	headers.insert(
		"editor-version",
		format!("vscode/{vscode_version}").parse().unwrap(),
	);
	headers.insert(
		"editor-plugin-version",
		HeaderValue::from_static(EDITOR_PLUGIN_VERSION),
	);
	headers.insert("user-agent", HeaderValue::from_static(USER_AGENT));
	headers.insert(
		"x-github-api-version",
		HeaderValue::from_static(API_VERSION),
	);
	headers.insert(
		"x-vscode-user-agent-library-version",
		HeaderValue::from_static("electron-fetch"),
	);
	headers
}
