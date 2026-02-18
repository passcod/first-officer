use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures::StreamExt;
use tracing::{debug, error, info};

use crate::auth::resolve::resolve_copilot_token;
use crate::copilot::client::chat_completions_raw;
use crate::copilot::types::ChatCompletionsRequest;
use crate::state::AppState;

pub async fn post_completions(
	State(state): State<Arc<AppState>>,
	headers: HeaderMap,
	body: axum::body::Bytes,
) -> Response {
	let copilot_token = match resolve_copilot_token(&state, &headers).await {
		Ok(t) => t,
		Err(resp) => return resp,
	};

	let body = resolve_model_name(&state, &body);
	let vision = detect_vision(&body);
	let is_agent = detect_agent(&body);

	// Log the incoming request
	if let Ok(req) = serde_json::from_slice::<ChatCompletionsRequest>(&body) {
		let is_streaming = req.stream.unwrap_or(false);
		info!(
			model = %req.model,
			streaming = is_streaming,
			messages = req.messages.len(),
			vision = vision,
			agent = is_agent,
			"incoming /v1/chat/completions request"
		);
		debug!(
			max_tokens = ?req.max_tokens,
			temperature = ?req.temperature,
			"request parameters"
		);
	}

	let resp = chat_completions_raw(
		&state.client,
		&copilot_token,
		&state.account_type,
		&state.vscode_version,
		&body,
		vision,
		is_agent,
	)
	.await;

	let upstream = match resp {
		Ok(r) => r,
		Err(e) => {
			error!(error = %e, "copilot chat completions request failed");
			return StatusCode::BAD_GATEWAY.into_response();
		}
	};

	debug!(status = %upstream.status(), "received response from Copilot API");

	let status = upstream.status();
	let is_stream = upstream
		.headers()
		.get("content-type")
		.and_then(|v| v.to_str().ok())
		.map(|ct| ct.contains("text/event-stream"))
		.unwrap_or(false);

	let mut headers = HeaderMap::new();
	if is_stream {
		headers.insert("content-type", "text/event-stream".parse().unwrap());
		headers.insert("cache-control", "no-cache".parse().unwrap());

		let byte_stream = upstream.bytes_stream().map(|chunk| {
			chunk.map_err(|e| {
				error!(error = %e, "error reading upstream stream");
				std::io::Error::other(e)
			})
		});

		info!("streaming response started");
		(status, headers, Body::from_stream(byte_stream)).into_response()
	} else {
		headers.insert("content-type", "application/json".parse().unwrap());

		let bytes = match upstream.bytes().await {
			Ok(b) => b,
			Err(e) => {
				error!(error = %e, "error reading upstream response");
				return StatusCode::BAD_GATEWAY.into_response();
			}
		};

		info!(status = %status, bytes = bytes.len(), "non-streaming response complete");
		(status, headers, bytes).into_response()
	}
}

fn detect_vision(body: &[u8]) -> bool {
	let Ok(req) = serde_json::from_slice::<ChatCompletionsRequest>(body) else {
		return false;
	};
	req.messages.iter().any(|msg| {
		msg.content
			.as_ref()
			.map(|c| match c {
				crate::copilot::types::Content::Parts(parts) => parts
					.iter()
					.any(|p| matches!(p, crate::copilot::types::ContentPart::ImageUrl { .. })),
				_ => false,
			})
			.unwrap_or(false)
	})
}

fn detect_agent(body: &[u8]) -> bool {
	let Ok(req) = serde_json::from_slice::<ChatCompletionsRequest>(body) else {
		return false;
	};
	req.messages
		.iter()
		.any(|msg| msg.role == "assistant" || msg.role == "tool")
}

/// If the request's model name is a renamed display name, swap it back to the
/// upstream Copilot model ID before forwarding.
fn resolve_model_name(state: &AppState, body: &[u8]) -> Vec<u8> {
	if !state.renamer.has_rules() {
		return body.to_vec();
	}
	let Ok(mut req) = serde_json::from_slice::<ChatCompletionsRequest>(body) else {
		return body.to_vec();
	};
	let resolved = state.renamer.resolve(&req.model);
	if resolved == req.model {
		return body.to_vec();
	}
	req.model = resolved;
	serde_json::to_vec(&req).unwrap_or_else(|_| body.to_vec())
}
