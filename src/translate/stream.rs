use crate::copilot::types::ChatCompletionChunk;
use crate::translate::types::{
    AnthropicUsage, ContentBlockStartBody, ContentDelta, MessageDeltaBody, MessageStartBody,
    StopReason, StreamEvent, StreamState,
};

pub fn translate_chunk(chunk: &ChatCompletionChunk, state: &mut StreamState) -> Vec<StreamEvent> {
    let mut events = Vec::new();

    if chunk.choices.is_empty() {
        return events;
    }

    let choice = &chunk.choices[0];
    let delta = &choice.delta;

    if !state.message_start_sent {
        let (input_tokens, cache_read) = extract_input_usage(chunk);
        events.push(StreamEvent::MessageStart {
            message: MessageStartBody {
                id: chunk.id.clone(),
                r#type: "message",
                role: "assistant",
                content: Vec::new(),
                model: chunk.model.clone(),
                stop_reason: None,
                stop_sequence: None,
                usage: AnthropicUsage {
                    input_tokens,
                    output_tokens: 0,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: if cache_read > 0 {
                        Some(cache_read)
                    } else {
                        None
                    },
                },
            },
        });
        state.message_start_sent = true;
    }

    if let Some(ref text) = delta.content {
        // If a tool block is open, close it before starting a text block
        if state.is_tool_block_open() {
            events.push(StreamEvent::ContentBlockStop {
                index: state.content_block_index,
            });
            state.content_block_index += 1;
            state.content_block_open = false;
        }

        if !state.content_block_open {
            events.push(StreamEvent::ContentBlockStart {
                index: state.content_block_index,
                content_block: ContentBlockStartBody::Text {
                    text: String::new(),
                },
            });
            state.content_block_open = true;
        }

        events.push(StreamEvent::ContentBlockDelta {
            index: state.content_block_index,
            delta: ContentDelta::Text { text: text.clone() },
        });
    }

    if let Some(ref tool_calls) = delta.tool_calls {
        for tool_call in tool_calls {
            // New tool call starting (has id and function name)
            if let (Some(id), Some(func)) = (&tool_call.id, &tool_call.function)
                && let Some(ref name) = func.name {
                    // Close any previously open block
                    if state.content_block_open {
                        events.push(StreamEvent::ContentBlockStop {
                            index: state.content_block_index,
                        });
                        state.content_block_index += 1;
                        state.content_block_open = false;
                    }

                    let anthropic_block_index = state.content_block_index;
                    state.tool_calls.insert(
                        tool_call.index,
                        crate::translate::types::ToolCallState {
                            id: id.clone(),
                            name: name.clone(),
                            anthropic_block_index,
                        },
                    );

                    events.push(StreamEvent::ContentBlockStart {
                        index: anthropic_block_index,
                        content_block: ContentBlockStartBody::ToolUse {
                            id: id.clone(),
                            name: name.clone(),
                            input: serde_json::Value::Object(Default::default()),
                        },
                    });
                    state.content_block_open = true;
                }

            // Tool call arguments delta
            if let Some(ref func) = tool_call.function
                && let Some(ref arguments) = func.arguments
                    && let Some(tc_state) = state.tool_calls.get(&tool_call.index) {
                        events.push(StreamEvent::ContentBlockDelta {
                            index: tc_state.anthropic_block_index,
                            delta: ContentDelta::InputJson {
                                partial_json: arguments.clone(),
                            },
                        });
                    }
        }
    }

    if let Some(ref finish_reason) = choice.finish_reason {
        if state.content_block_open {
            events.push(StreamEvent::ContentBlockStop {
                index: state.content_block_index,
            });
            state.content_block_open = false;
        }

        let (input_tokens, cache_read) = extract_input_usage(chunk);

        events.push(StreamEvent::MessageDelta {
            delta: MessageDeltaBody {
                stop_reason: Some(map_stop_reason(finish_reason)),
                stop_sequence: None,
            },
            usage: Some(AnthropicUsage {
                input_tokens,
                output_tokens: chunk
                    .usage
                    .as_ref()
                    .map(|u| u.completion_tokens)
                    .unwrap_or(0),
                cache_creation_input_tokens: None,
                cache_read_input_tokens: if cache_read > 0 {
                    Some(cache_read)
                } else {
                    None
                },
            }),
        });

        events.push(StreamEvent::MessageStop {});
    }

    events
}

fn extract_input_usage(chunk: &ChatCompletionChunk) -> (u64, u64) {
    match &chunk.usage {
        Some(u) => {
            let cached = u
                .prompt_tokens_details
                .as_ref()
                .map(|d| d.cached_tokens)
                .unwrap_or(0);
            (u.prompt_tokens.saturating_sub(cached), cached)
        }
        None => (0, 0),
    }
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

    fn make_chunk(id: &str, model: &str, choices: Vec<ChunkChoice>) -> ChatCompletionChunk {
        ChatCompletionChunk {
            id: id.to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1234567890,
            model: model.to_string(),
            choices,
            system_fingerprint: None,
            usage: None,
        }
    }

    fn text_delta(content: &str) -> ChunkChoice {
        ChunkChoice {
            index: 0,
            delta: Delta {
                content: Some(content.to_string()),
                role: None,
                tool_calls: None,
            },
            finish_reason: None,
            logprobs: None,
        }
    }

    fn finish_choice(reason: &str) -> ChunkChoice {
        ChunkChoice {
            index: 0,
            delta: Delta {
                content: None,
                role: None,
                tool_calls: None,
            },
            finish_reason: Some(reason.to_string()),
            logprobs: None,
        }
    }

    #[test]
    fn first_chunk_emits_message_start_and_text() {
        let mut state = StreamState::new();
        let chunk = make_chunk("c1", "gpt-4", vec![text_delta("Hello")]);
        let events = translate_chunk(&chunk, &mut state);

        assert_eq!(events.len(), 3);
        assert_eq!(events[0].event_type(), "message_start");
        assert_eq!(events[1].event_type(), "content_block_start");
        assert_eq!(events[2].event_type(), "content_block_delta");
        assert!(state.message_start_sent);
        assert!(state.content_block_open);
    }

    #[test]
    fn subsequent_text_reuses_block() {
        let mut state = StreamState::new();
        let chunk1 = make_chunk("c1", "gpt-4", vec![text_delta("Hello")]);
        translate_chunk(&chunk1, &mut state);

        let chunk2 = make_chunk("c1", "gpt-4", vec![text_delta(" world")]);
        let events = translate_chunk(&chunk2, &mut state);

        // Should only emit a delta, no new block start
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type(), "content_block_delta");
    }

    #[test]
    fn finish_reason_closes_and_stops() {
        let mut state = StreamState::new();
        let chunk1 = make_chunk("c1", "gpt-4", vec![text_delta("Hi")]);
        translate_chunk(&chunk1, &mut state);

        let chunk2 = make_chunk("c1", "gpt-4", vec![finish_choice("stop")]);
        let events = translate_chunk(&chunk2, &mut state);

        assert_eq!(events.len(), 3);
        assert_eq!(events[0].event_type(), "content_block_stop");
        assert_eq!(events[1].event_type(), "message_delta");
        assert_eq!(events[2].event_type(), "message_stop");
        assert!(!state.content_block_open);
    }

    #[test]
    fn tool_call_creates_new_block() {
        let mut state = StreamState::new();

        // First: message_start from an empty role-only delta
        let chunk1 = make_chunk(
            "c1",
            "gpt-4",
            vec![ChunkChoice {
                index: 0,
                delta: Delta {
                    content: None,
                    role: Some("assistant".to_string()),
                    tool_calls: Some(vec![DeltaToolCall {
                        index: 0,
                        id: Some("call_1".to_string()),
                        r#type: Some("function".to_string()),
                        function: Some(DeltaFunction {
                            name: Some("get_weather".to_string()),
                            arguments: None,
                        }),
                    }]),
                },
                finish_reason: None,
                logprobs: None,
            }],
        );
        let events = translate_chunk(&chunk1, &mut state);

        // message_start + content_block_start (tool_use)
        assert!(
            events
                .iter()
                .any(|e| e.event_type() == "content_block_start")
        );
        assert!(state.tool_calls.contains_key(&0));
        assert!(state.content_block_open);

        // Arguments delta
        let chunk2 = make_chunk(
            "c1",
            "gpt-4",
            vec![ChunkChoice {
                index: 0,
                delta: Delta {
                    content: None,
                    role: None,
                    tool_calls: Some(vec![DeltaToolCall {
                        index: 0,
                        id: None,
                        r#type: None,
                        function: Some(DeltaFunction {
                            name: None,
                            arguments: Some(r#"{"loc"#.to_string()),
                        }),
                    }]),
                },
                finish_reason: None,
                logprobs: None,
            }],
        );
        let events2 = translate_chunk(&chunk2, &mut state);
        assert_eq!(events2.len(), 1);
        assert_eq!(events2[0].event_type(), "content_block_delta");
    }

    #[test]
    fn text_after_tool_closes_tool_block() {
        let mut state = StreamState::new();

        // Start with a tool call
        let chunk1 = make_chunk(
            "c1",
            "gpt-4",
            vec![ChunkChoice {
                index: 0,
                delta: Delta {
                    content: None,
                    role: None,
                    tool_calls: Some(vec![DeltaToolCall {
                        index: 0,
                        id: Some("call_1".to_string()),
                        r#type: Some("function".to_string()),
                        function: Some(DeltaFunction {
                            name: Some("func".to_string()),
                            arguments: None,
                        }),
                    }]),
                },
                finish_reason: None,
                logprobs: None,
            }],
        );
        translate_chunk(&chunk1, &mut state);
        assert!(state.is_tool_block_open());

        // Then text arrives
        let chunk2 = make_chunk("c1", "gpt-4", vec![text_delta("After tool")]);
        let events = translate_chunk(&chunk2, &mut state);

        let types: Vec<&str> = events.iter().map(|e| e.event_type()).collect();
        assert!(types.contains(&"content_block_stop"));
        assert!(types.contains(&"content_block_start"));
        assert!(types.contains(&"content_block_delta"));
    }
}
