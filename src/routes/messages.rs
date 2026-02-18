use std::convert::Infallible;
use std::sync::Arc;

use axum::Json;
use axum::extract::{FromRequest, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures::StreamExt;
use futures::stream::Stream;
use tracing::{debug, error, info, warn};

use crate::auth::resolve::resolve_copilot_token;
use crate::copilot::client::chat_completions_raw;
use crate::copilot::types::ChatCompletionChunk;
use crate::state::AppState;
use crate::translate::request::{has_vision_content, is_agent_call, translate_request};
use crate::translate::response::translate_response;
use crate::translate::stream::translate_chunk;
use crate::translate::types::{MessagesRequest, StreamState};

pub struct JsonWithLogging<T>(T);

impl<T> FromRequest<Arc<AppState>> for JsonWithLogging<T>
where
	T: serde::de::DeserializeOwned,
{
	type Rejection = Response;

	async fn from_request(req: Request, _state: &Arc<AppState>) -> Result<Self, Self::Rejection> {
		let (_parts, body) = req.into_parts();
		let bytes = match axum::body::to_bytes(body, usize::MAX).await {
			Ok(b) => b,
			Err(e) => {
				error!(error = %e, "failed to read request body");
				return Err((
					StatusCode::BAD_REQUEST,
					Json(serde_json::json!({
						"type": "error",
						"error": {
							"type": "invalid_request_error",
							"message": format!("failed to read request body: {e}")
						}
					})),
				)
					.into_response());
			}
		};

		match serde_json::from_slice::<T>(&bytes) {
			Ok(value) => Ok(JsonWithLogging(value)),
			Err(e) => {
				error!(
					error = %e,
					body = %String::from_utf8_lossy(&bytes),
					"failed to deserialize request body"
				);
				Err((
					StatusCode::UNPROCESSABLE_ENTITY,
					Json(serde_json::json!({
						"type": "error",
						"error": {
							"type": "invalid_request_error",
							"message": format!("Failed to deserialize the JSON body into the target type: {e}")
						}
					})),
				)
					.into_response())
			}
		}
	}
}

pub async fn post_messages(
	State(state): State<Arc<AppState>>,
	headers: HeaderMap,
	JsonWithLogging(mut req): JsonWithLogging<MessagesRequest>,
) -> Response {
	let copilot_token = match resolve_copilot_token(&state, &headers).await {
		Ok(t) => t,
		Err(resp) => return resp,
	};

	let display_model = req.model.clone();

	// Ensure models are learned for resolution
	if state.renamer.dump_learned().is_empty() {
		debug!("no learned model mappings, fetching models on-demand");
		if let Err(e) = ensure_models_cached(&state, &copilot_token).await {
			warn!(error = %e, "failed to fetch models for resolution, proceeding anyway");
		}
	}

	let resolved_model = state.renamer.resolve(&req.model);
	info!(
		display = %display_model,
		resolved = %resolved_model,
		"model resolution"
	);
	req.model = resolved_model;

	let is_streaming = req.stream.unwrap_or(false);
	let vision = has_vision_content(&req);
	let agent = is_agent_call(&req);

	info!(
		model = %display_model,
		streaming = is_streaming,
		messages = req.messages.len(),
		vision = vision,
		agent = agent,
		thinking = req.thinking.is_some(),
		"incoming /v1/messages request"
	);

	let openai_req = translate_request(&req, state.emulate_thinking);
	let body = match serde_json::to_vec(&openai_req) {
		Ok(b) => b,
		Err(e) => {
			error!(error = %e, "failed to serialize translated request");
			return StatusCode::INTERNAL_SERVER_ERROR.into_response();
		}
	};

	debug!(
		upstream_model = %openai_req.model,
		upstream_messages = openai_req.messages.len(),
		max_tokens = ?openai_req.max_tokens,
		"sending request to Copilot API"
	);

	let upstream = match chat_completions_raw(
		&state.client,
		&copilot_token,
		&state.account_type,
		&state.vscode_version,
		&body,
		vision,
		agent,
	)
	.await
	{
		Ok(r) => r,
		Err(e) => {
			error!(error = %e, model = %display_model, "copilot request failed");
			return (
				StatusCode::BAD_GATEWAY,
				Json(serde_json::json!({
					"type": "error",
					"error": {
						"type": "api_error",
						"message": format!("upstream request failed: {e}")
					}
				})),
			)
				.into_response();
		}
	};

	debug!(
		status = %upstream.status(),
		streaming = is_streaming,
		"received response from Copilot API"
	);

	if !is_streaming {
		return handle_non_streaming(upstream, display_model, state.emulate_thinking).await;
	}

	handle_streaming(upstream, display_model, state.emulate_thinking).into_response()
}

async fn handle_non_streaming(
	upstream: reqwest::Response,
	display_model: String,
	emulate_thinking: bool,
) -> Response {
	let bytes = match upstream.bytes().await {
		Ok(b) => b,
		Err(e) => {
			error!(error = %e, "failed to read upstream response");
			return StatusCode::BAD_GATEWAY.into_response();
		}
	};

	let openai_resp = match serde_json::from_slice(&bytes) {
		Ok(r) => r,
		Err(e) => {
			error!(
				error = %e,
				body = %String::from_utf8_lossy(&bytes),
				"failed to parse upstream response"
			);
			return StatusCode::BAD_GATEWAY.into_response();
		}
	};

	let mut anthropic_resp = translate_response(&openai_resp, emulate_thinking);
	anthropic_resp.model = display_model.clone();

	info!(
		model = %display_model,
		stop_reason = ?anthropic_resp.stop_reason,
		content_blocks = anthropic_resp.content.len(),
		input_tokens = anthropic_resp.usage.input_tokens,
		output_tokens = anthropic_resp.usage.output_tokens,
		"non-streaming response complete"
	);

	Json(anthropic_resp).into_response()
}

fn handle_streaming(
	upstream: reqwest::Response,
	display_model: String,
	emulate_thinking: bool,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
	let stream = async_stream::stream! {
		let mut state = StreamState::new(emulate_thinking);
		let mut bytes_stream = upstream.bytes_stream();
		let mut buffer = String::new();

		while let Some(chunk_result) = bytes_stream.next().await {
			let chunk_bytes = match chunk_result {
				Ok(b) => b,
				Err(e) => {
					error!(error = %e, "error reading upstream stream");
					break;
				}
			};

			buffer.push_str(&String::from_utf8_lossy(&chunk_bytes));

			// Process complete SSE lines from the buffer
			while let Some(event_data) = extract_next_sse_data(&mut buffer) {
				if event_data == "[DONE]" {
					debug!("upstream SSE stream done");
					break;
				}

				let mut chunk: ChatCompletionChunk = match serde_json::from_str(&event_data) {
					Ok(c) => c,
					Err(e) => {
						debug!(error = %e, data = %event_data, "skipping unparsable chunk");
						continue;
					}
				};

				chunk.model = display_model.clone();
				let events = translate_chunk(&chunk, &mut state);
				for ev in events {
					let data = match serde_json::to_string(&ev) {
						Ok(d) => d,
						Err(e) => {
							error!(error = %e, "failed to serialize stream event");
							continue;
						}
					};

					let sse_event = Event::default()
						.event(ev.event_type())
						.data(data);

					yield Ok(sse_event);
				}
			}
		}

		info!(model = %display_model, "streaming response complete");

		// Flush any buffered content from the thinking parser
		if let Some(parser) = state.thinking_parser.take()
			&& let Some(final_event) = parser.finish() {
				match final_event {
					crate::translate::thinking::ThinkingEvent::ThinkingDelta(thinking_text) => {
						let ev = crate::translate::types::StreamEvent::ContentBlockDelta {
							index: state.content_block_index,
							delta: crate::translate::types::ContentDelta::Thinking {
								thinking: thinking_text,
							},
						};
						if let Ok(data) = serde_json::to_string(&ev) {
							let sse_event = Event::default()
								.event(ev.event_type())
								.data(data);
							yield Ok(sse_event);
						}
					}
					crate::translate::thinking::ThinkingEvent::TextDelta(text_chunk) => {
						let ev = crate::translate::types::StreamEvent::ContentBlockDelta {
							index: state.content_block_index,
							delta: crate::translate::types::ContentDelta::Text { text: text_chunk },
						};
						if let Ok(data) = serde_json::to_string(&ev) {
							let sse_event = Event::default()
								.event(ev.event_type())
								.data(data);
							yield Ok(sse_event);
						}
					}
					_ => {} // ThinkingStart/End shouldn't happen in finish
				}
			}
	};

	Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Extract the next complete SSE data field from the buffer.
/// SSE format: lines starting with "data: " followed by content, separated by blank lines.
fn extract_next_sse_data(buffer: &mut String) -> Option<String> {
	// Look for a complete SSE event (terminated by a double newline)
	loop {
		let boundary = buffer.find("\n\n");
		if boundary.is_none() {
			// Also try \r\n\r\n
			if let Some(pos) = buffer.find("\r\n\r\n") {
				let event_block = buffer[..pos].to_string();
				buffer.drain(..pos + 4);
				if let Some(data) = parse_sse_data(&event_block) {
					return Some(data);
				}
				continue;
			}
			return None;
		}

		let pos = boundary.unwrap();
		let event_block = buffer[..pos].to_string();
		buffer.drain(..pos + 2);

		if let Some(data) = parse_sse_data(&event_block) {
			return Some(data);
		}
		// If we couldn't extract data from this block (e.g. comment lines), keep going
	}
}

fn parse_sse_data(block: &str) -> Option<String> {
	let mut data_parts = Vec::new();
	for line in block.lines() {
		let line = line.trim_start();
		if let Some(rest) = line.strip_prefix("data:") {
			let value = rest.strip_prefix(' ').unwrap_or(rest);
			data_parts.push(value.to_string());
		}
	}
	if data_parts.is_empty() {
		None
	} else {
		Some(data_parts.join("\n"))
	}
}

async fn ensure_models_cached(state: &AppState, copilot_token: &str) -> Result<(), anyhow::Error> {
	// Check if cache is valid
	{
		let models = state.models.read().await;
		if let Some(cached) = models.as_ref()
			&& state.is_models_cache_valid(cached)
		{
			debug!("models cache is valid, using cached mappings");
			return Ok(());
		}
	}

	// Fetch and cache models
	let mut models = crate::copilot::client::fetch_models(
		&state.client,
		copilot_token,
		&state.account_type,
		&state.vscode_version,
	)
	.await?;

	// Apply model renaming and register mappings
	for model in &mut models.data {
		let renamed = state.renamer.rename(&model.id);
		state.renamer.register(&model.id, &renamed);
		if renamed != model.id {
			info!(from = %model.id, to = %renamed, "renamed model");
			model.id = renamed;
		}
	}

	let names: Vec<&str> = models.data.iter().map(|m| m.id.as_str()).collect();
	info!(count = models.data.len(), models = ?names, "cached models");

	let learned = state.renamer.dump_learned();
	info!(count = learned.len(), "learned model mappings");
	for (display_name, upstream_name) in &learned {
		info!(display = %display_name, upstream = %upstream_name, "mapping");
	}

	*state.models.write().await = Some(crate::state::CachedModels {
		response: models,
		cached_at: std::time::SystemTime::now(),
	});

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn extract_sse_data_simple() {
		let mut buf = "data: hello\n\n".to_string();
		assert_eq!(extract_next_sse_data(&mut buf), Some("hello".to_string()));
		assert!(buf.is_empty());
	}

	#[test]
	fn extract_sse_data_with_event_type() {
		let mut buf = "event: message\ndata: {\"text\":\"hi\"}\n\n".to_string();
		assert_eq!(
			extract_next_sse_data(&mut buf),
			Some("{\"text\":\"hi\"}".to_string())
		);
	}

	#[test]
	fn extract_sse_data_done() {
		let mut buf = "data: [DONE]\n\n".to_string();
		assert_eq!(extract_next_sse_data(&mut buf), Some("[DONE]".to_string()));
	}

	#[test]
	fn extract_sse_data_incomplete() {
		let mut buf = "data: partial".to_string();
		assert_eq!(extract_next_sse_data(&mut buf), None);
		assert_eq!(buf, "data: partial");
	}

	#[test]
	fn extract_multiple_events() {
		let mut buf = "data: first\n\ndata: second\n\n".to_string();
		assert_eq!(extract_next_sse_data(&mut buf), Some("first".to_string()));
		assert_eq!(extract_next_sse_data(&mut buf), Some("second".to_string()));
		assert_eq!(extract_next_sse_data(&mut buf), None);
	}
}
