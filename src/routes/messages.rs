use std::convert::Infallible;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures::StreamExt;
use futures::stream::Stream;
use tracing::{debug, error};

use crate::auth::resolve::resolve_copilot_token;
use crate::copilot::client::chat_completions_raw;
use crate::copilot::types::ChatCompletionChunk;
use crate::state::AppState;
use crate::translate::request::{has_vision_content, is_agent_call, translate_request};
use crate::translate::response::translate_response;
use crate::translate::stream::translate_chunk;
use crate::translate::types::{MessagesRequest, StreamState};

pub async fn post_messages(
	State(state): State<Arc<AppState>>,
	headers: HeaderMap,
	Json(mut req): Json<MessagesRequest>,
) -> Response {
	let copilot_token = match resolve_copilot_token(&state, &headers).await {
		Ok(t) => t,
		Err(resp) => return resp,
	};

	let display_model = req.model.clone();
	req.model = state.renamer.resolve(&req.model);

	let is_streaming = req.stream.unwrap_or(false);
	let vision = has_vision_content(&req);
	let agent = is_agent_call(&req);

	let openai_req = translate_request(&req, state.emulate_thinking);
	let body = match serde_json::to_vec(&openai_req) {
		Ok(b) => b,
		Err(e) => {
			error!(error = %e, "failed to serialize translated request");
			return StatusCode::INTERNAL_SERVER_ERROR.into_response();
		}
	};

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
			error!(error = %e, "copilot request failed");
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
	anthropic_resp.model = display_model;
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
