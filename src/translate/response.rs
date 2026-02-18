use crate::copilot::types::{ChatCompletionResponse, ToolCall};
use crate::translate::types::{
	AnthropicUsage, AssistantContentBlock, MessagesResponse, StopReason, TextBlock, ToolUseBlock,
};

pub fn translate_response(resp: &ChatCompletionResponse) -> MessagesResponse {
	let mut text_blocks: Vec<AssistantContentBlock> = Vec::new();
	let mut tool_blocks: Vec<AssistantContentBlock> = Vec::new();
	let mut stop_reason = None;

	for (i, choice) in resp.choices.iter().enumerate() {
		if let Some(ref content) = choice.message.content
			&& !content.is_empty()
		{
			text_blocks.push(AssistantContentBlock::Text(TextBlock {
				text: content.clone(),
			}));
		}

		if let Some(ref tool_calls) = choice.message.tool_calls {
			for tc in tool_calls {
				tool_blocks.push(translate_tool_call(tc));
			}
		}

		if i == 0 {
			stop_reason = choice.finish_reason.as_deref().map(map_stop_reason);
		}
		if choice.finish_reason.as_deref() == Some("tool_calls") {
			stop_reason = Some(StopReason::ToolUse);
		}
	}

	let mut content = text_blocks;
	content.append(&mut tool_blocks);

	let (input_tokens, output_tokens, cache_read) = match &resp.usage {
		Some(u) => {
			let cached = u
				.prompt_tokens_details
				.as_ref()
				.map(|d| d.cached_tokens)
				.unwrap_or(0);
			(
				u.prompt_tokens.saturating_sub(cached),
				u.completion_tokens,
				cached,
			)
		}
		None => (0, 0, 0),
	};

	MessagesResponse {
		id: resp.id.clone(),
		r#type: "message",
		role: "assistant",
		model: resp.model.clone(),
		content,
		stop_reason,
		stop_sequence: None,
		usage: AnthropicUsage {
			input_tokens,
			output_tokens,
			cache_creation_input_tokens: None,
			cache_read_input_tokens: if cache_read > 0 {
				Some(cache_read)
			} else {
				None
			},
		},
	}
}

fn translate_tool_call(tc: &ToolCall) -> AssistantContentBlock {
	let input: serde_json::Value = serde_json::from_str(&tc.function.arguments)
		.unwrap_or(serde_json::Value::Object(Default::default()));

	AssistantContentBlock::ToolUse(ToolUseBlock {
		id: tc.id.clone(),
		name: tc.function.name.clone(),
		input,
	})
}

fn map_stop_reason(reason: &str) -> StopReason {
	match reason {
		"stop" => StopReason::EndTurn,
		"length" => StopReason::MaxTokens,
		"tool_calls" => StopReason::ToolUse,
		"content_filter" => StopReason::EndTurn,
		_ => StopReason::EndTurn,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::copilot::types::*;

	#[test]
	fn translate_simple_text_response() {
		let resp = ChatCompletionResponse {
			id: "chatcmpl-123".to_string(),
			object: "chat.completion".to_string(),
			created: 1234567890,
			model: "gpt-4".to_string(),
			choices: vec![Choice {
				index: 0,
				message: ResponseMessage {
					role: "assistant".to_string(),
					content: Some("Hello!".to_string()),
					tool_calls: None,
				},
				finish_reason: Some("stop".to_string()),
				logprobs: None,
			}],
			system_fingerprint: None,
			usage: Some(Usage {
				prompt_tokens: 10,
				completion_tokens: 5,
				total_tokens: 15,
				prompt_tokens_details: None,
			}),
		};

		let result = translate_response(&resp);
		assert_eq!(result.id, "chatcmpl-123");
		assert_eq!(result.model, "gpt-4");
		assert_eq!(result.content.len(), 1);
		assert!(matches!(&result.content[0], AssistantContentBlock::Text(t) if t.text == "Hello!"));
		assert!(matches!(result.stop_reason, Some(StopReason::EndTurn)));
		assert_eq!(result.usage.input_tokens, 10);
		assert_eq!(result.usage.output_tokens, 5);
	}

	#[test]
	fn translate_tool_call_response() {
		let resp = ChatCompletionResponse {
			id: "chatcmpl-456".to_string(),
			object: "chat.completion".to_string(),
			created: 1234567890,
			model: "gpt-4".to_string(),
			choices: vec![Choice {
				index: 0,
				message: ResponseMessage {
					role: "assistant".to_string(),
					content: Some("Let me check that.".to_string()),
					tool_calls: Some(vec![ToolCall {
						id: "call_abc".to_string(),
						r#type: "function".to_string(),
						function: ToolCallFunction {
							name: "get_weather".to_string(),
							arguments: r#"{"location":"London"}"#.to_string(),
						},
					}]),
				},
				finish_reason: Some("tool_calls".to_string()),
				logprobs: None,
			}],
			system_fingerprint: None,
			usage: Some(Usage {
				prompt_tokens: 20,
				completion_tokens: 10,
				total_tokens: 30,
				prompt_tokens_details: None,
			}),
		};

		let result = translate_response(&resp);
		assert!(matches!(result.stop_reason, Some(StopReason::ToolUse)));
		assert_eq!(result.content.len(), 2);
		assert!(
			matches!(&result.content[0], AssistantContentBlock::Text(t) if t.text == "Let me check that.")
		);
		assert!(
			matches!(&result.content[1], AssistantContentBlock::ToolUse(tu) if tu.name == "get_weather")
		);
	}

	#[test]
	fn translate_with_cached_tokens() {
		let resp = ChatCompletionResponse {
			id: "chatcmpl-789".to_string(),
			object: "chat.completion".to_string(),
			created: 1234567890,
			model: "gpt-4".to_string(),
			choices: vec![Choice {
				index: 0,
				message: ResponseMessage {
					role: "assistant".to_string(),
					content: Some("Hi".to_string()),
					tool_calls: None,
				},
				finish_reason: Some("stop".to_string()),
				logprobs: None,
			}],
			system_fingerprint: None,
			usage: Some(Usage {
				prompt_tokens: 100,
				completion_tokens: 5,
				total_tokens: 105,
				prompt_tokens_details: Some(PromptTokensDetails { cached_tokens: 40 }),
			}),
		};

		let result = translate_response(&resp);
		assert_eq!(result.usage.input_tokens, 60);
		assert_eq!(result.usage.output_tokens, 5);
		assert_eq!(result.usage.cache_read_input_tokens, Some(40));
	}
}
