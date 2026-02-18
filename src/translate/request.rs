use crate::copilot::types::{
    ChatCompletionsRequest, Content, ContentPart, FunctionDef, ImageUrl, Message, NamedToolChoice,
    NamedToolChoiceFunction, Stop, Tool, ToolCall, ToolCallFunction, ToolChoice,
};
use crate::translate::types::{
    AnthropicMessage, AnthropicTool, AnthropicToolChoice, AssistantContent, AssistantContentBlock,
    MessagesRequest, SystemPrompt, UserContent, UserContentBlock,
};

pub fn translate_request(req: &MessagesRequest) -> ChatCompletionsRequest {
    ChatCompletionsRequest {
        model: normalize_model_name(&req.model),
        messages: translate_messages(&req.messages, &req.system),
        max_tokens: Some(req.max_tokens),
        temperature: req.temperature,
        top_p: req.top_p,
        stop: req.stop_sequences.as_ref().map(|s| {
            if s.len() == 1 {
                Stop::Single(s[0].clone())
            } else {
                Stop::Multiple(s.clone())
            }
        }),
        stream: req.stream,
        n: None,
        frequency_penalty: None,
        presence_penalty: None,
        tools: req.tools.as_ref().map(|t| translate_tools(t)),
        tool_choice: req.tool_choice.as_ref().and_then(translate_tool_choice),
        user: req.metadata.as_ref().and_then(|m| m.user_id.clone()),
    }
}

fn normalize_model_name(model: &str) -> String {
    if let Some(rest) = model.strip_prefix("claude-sonnet-4-")
        && !rest.is_empty()
    {
        return "claude-sonnet-4".to_string();
    }
    if let Some(rest) = model.strip_prefix("claude-opus-4-")
        && !rest.is_empty()
    {
        return "claude-opus-4".to_string();
    }
    model.to_string()
}

fn translate_messages(
    messages: &[AnthropicMessage],
    system: &Option<SystemPrompt>,
) -> Vec<Message> {
    let mut out = Vec::new();

    if let Some(sys) = system {
        out.push(Message {
            role: "system".to_string(),
            content: Some(Content::Text(system_prompt_to_string(sys))),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        });
    }

    for msg in messages {
        match msg {
            AnthropicMessage::User { content } => {
                out.extend(translate_user_message(content));
            }
            AnthropicMessage::Assistant { content } => {
                out.extend(translate_assistant_message(content));
            }
        }
    }

    out
}

fn system_prompt_to_string(sys: &SystemPrompt) -> String {
    match sys {
        SystemPrompt::Text(s) => s.clone(),
        SystemPrompt::Blocks(blocks) => blocks
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n"),
    }
}

fn translate_user_message(content: &UserContent) -> Vec<Message> {
    match content {
        UserContent::Text(s) => vec![Message {
            role: "user".to_string(),
            content: Some(Content::Text(s.clone())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        UserContent::Blocks(blocks) => {
            let mut out = Vec::new();

            // Tool results must come first
            for block in blocks {
                if let UserContentBlock::ToolResult(tr) = block {
                    out.push(Message {
                        role: "tool".to_string(),
                        content: Some(Content::Text(tr.content.clone())),
                        name: None,
                        tool_calls: None,
                        tool_call_id: Some(tr.tool_use_id.clone()),
                    });
                }
            }

            let other_blocks: Vec<&UserContentBlock> = blocks
                .iter()
                .filter(|b| !matches!(b, UserContentBlock::ToolResult(_)))
                .collect();

            if !other_blocks.is_empty() {
                let has_image = other_blocks
                    .iter()
                    .any(|b| matches!(b, UserContentBlock::Image(_)));

                if has_image {
                    let parts: Vec<ContentPart> = other_blocks
                        .iter()
                        .filter_map(|b| match b {
                            UserContentBlock::Text(t) => Some(ContentPart::Text {
                                text: t.text.clone(),
                            }),
                            UserContentBlock::Image(img) => Some(ContentPart::ImageUrl {
                                image_url: ImageUrl {
                                    url: format!(
                                        "data:{};base64,{}",
                                        img.source.media_type, img.source.data
                                    ),
                                    detail: None,
                                },
                            }),
                            UserContentBlock::ToolResult(_) => None,
                        })
                        .collect();
                    out.push(Message {
                        role: "user".to_string(),
                        content: Some(Content::Parts(parts)),
                        name: None,
                        tool_calls: None,
                        tool_call_id: None,
                    });
                } else {
                    let text: String = other_blocks
                        .iter()
                        .filter_map(|b| match b {
                            UserContentBlock::Text(t) => Some(t.text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n\n");
                    out.push(Message {
                        role: "user".to_string(),
                        content: Some(Content::Text(text)),
                        name: None,
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
            }

            out
        }
    }
}

fn translate_assistant_message(content: &AssistantContent) -> Vec<Message> {
    match content {
        AssistantContent::Text(s) => vec![Message {
            role: "assistant".to_string(),
            content: Some(Content::Text(s.clone())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        AssistantContent::Blocks(blocks) => {
            let tool_use_blocks: Vec<&AssistantContentBlock> = blocks
                .iter()
                .filter(|b| matches!(b, AssistantContentBlock::ToolUse(_)))
                .collect();

            let text_content: String = blocks
                .iter()
                .filter_map(|b| match b {
                    AssistantContentBlock::Text(t) => Some(t.text.as_str()),
                    AssistantContentBlock::Thinking(t) => Some(t.thinking.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n\n");

            if tool_use_blocks.is_empty() {
                vec![Message {
                    role: "assistant".to_string(),
                    content: if text_content.is_empty() {
                        None
                    } else {
                        Some(Content::Text(text_content))
                    },
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                }]
            } else {
                let tool_calls: Vec<ToolCall> = tool_use_blocks
                    .iter()
                    .filter_map(|b| match b {
                        AssistantContentBlock::ToolUse(tu) => Some(ToolCall {
                            id: tu.id.clone(),
                            r#type: "function".to_string(),
                            function: ToolCallFunction {
                                name: tu.name.clone(),
                                arguments: serde_json::to_string(&tu.input).unwrap_or_default(),
                            },
                        }),
                        _ => None,
                    })
                    .collect();

                vec![Message {
                    role: "assistant".to_string(),
                    content: if text_content.is_empty() {
                        None
                    } else {
                        Some(Content::Text(text_content))
                    },
                    name: None,
                    tool_calls: Some(tool_calls),
                    tool_call_id: None,
                }]
            }
        }
    }
}

fn translate_tools(tools: &[AnthropicTool]) -> Vec<Tool> {
    tools
        .iter()
        .map(|t| Tool {
            r#type: "function".to_string(),
            function: FunctionDef {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.input_schema.clone(),
            },
        })
        .collect()
}

fn translate_tool_choice(tc: &AnthropicToolChoice) -> Option<ToolChoice> {
    match tc.r#type.as_str() {
        "auto" => Some(ToolChoice::String("auto".to_string())),
        "any" => Some(ToolChoice::String("required".to_string())),
        "none" => Some(ToolChoice::String("none".to_string())),
        "tool" => tc.name.as_ref().map(|name| {
            ToolChoice::Named(NamedToolChoice {
                r#type: "function".to_string(),
                function: NamedToolChoiceFunction { name: name.clone() },
            })
        }),
        _ => None,
    }
}

/// Detect if any message in the Anthropic request contains image content.
pub fn has_vision_content(req: &MessagesRequest) -> bool {
    req.messages.iter().any(|msg| match msg {
        AnthropicMessage::User {
            content: UserContent::Blocks(blocks),
        } => blocks
            .iter()
            .any(|b| matches!(b, UserContentBlock::Image(_))),
        _ => false,
    })
}

/// Detect if the conversation includes agent-like messages (assistant or tool).
pub fn is_agent_call(req: &MessagesRequest) -> bool {
    req.messages
        .iter()
        .any(|msg| matches!(msg, AnthropicMessage::Assistant { .. }))
}
