use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// --- Messages Request ---

#[derive(Debug, Clone, Deserialize)]
pub struct MessagesRequest {
	pub model: String,
	pub messages: Vec<AnthropicMessage>,
	pub max_tokens: u64,
	#[serde(default)]
	pub system: Option<SystemPrompt>,
	#[serde(default)]
	pub metadata: Option<Metadata>,
	#[serde(default)]
	pub stop_sequences: Option<Vec<String>>,
	#[serde(default)]
	pub stream: Option<bool>,
	#[serde(default)]
	pub temperature: Option<f64>,
	#[serde(default)]
	pub top_p: Option<f64>,
	#[serde(default)]
	#[expect(dead_code, reason = "part of the Anthropic API schema")]
	pub top_k: Option<u64>,
	#[serde(default)]
	pub tools: Option<Vec<AnthropicTool>>,
	#[serde(default)]
	pub tool_choice: Option<AnthropicToolChoice>,
	#[serde(default)]
	#[expect(dead_code, reason = "part of the Anthropic API schema")]
	pub thinking: Option<ThinkingConfig>,
	#[serde(default)]
	#[expect(dead_code, reason = "part of the Anthropic API schema")]
	pub service_tier: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SystemPrompt {
	Text(String),
	Blocks(Vec<TextBlock>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Metadata {
	pub user_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThinkingConfig {
	#[expect(dead_code, reason = "part of the Anthropic API schema")]
	pub r#type: String,
	#[expect(dead_code, reason = "part of the Anthropic API schema")]
	pub budget_tokens: Option<u64>,
}

// --- Messages ---

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "role")]
pub enum AnthropicMessage {
	#[serde(rename = "user")]
	User { content: UserContent },
	#[serde(rename = "assistant")]
	Assistant { content: AssistantContent },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum UserContent {
	Text(String),
	Blocks(Vec<UserContentBlock>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AssistantContent {
	Text(String),
	Blocks(Vec<AssistantContentBlock>),
}

// --- Content Blocks ---

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum UserContentBlock {
	#[serde(rename = "text")]
	Text(TextBlock),
	#[serde(rename = "image")]
	Image(ImageBlock),
	#[serde(rename = "tool_result")]
	ToolResult(ToolResultBlock),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AssistantContentBlock {
	#[serde(rename = "text")]
	Text(TextBlock),
	#[serde(rename = "tool_use")]
	ToolUse(ToolUseBlock),
	#[serde(rename = "thinking")]
	Thinking(ThinkingBlock),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBlock {
	pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImageBlock {
	pub source: ImageSource,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImageSource {
	#[expect(dead_code, reason = "part of the Anthropic API schema")]
	pub r#type: String,
	pub media_type: String,
	pub data: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolResultBlock {
	pub tool_use_id: String,
	pub content: String,
	#[serde(default)]
	#[expect(dead_code, reason = "part of the Anthropic API schema")]
	pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseBlock {
	pub id: String,
	pub name: String,
	pub input: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingBlock {
	pub thinking: String,
}

// --- Tools ---

#[derive(Debug, Clone, Deserialize)]
pub struct AnthropicTool {
	pub name: String,
	#[serde(default)]
	pub description: Option<String>,
	pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnthropicToolChoice {
	pub r#type: String,
	#[serde(default)]
	pub name: Option<String>,
}

// --- Non-streaming Response ---

#[derive(Debug, Clone, Serialize)]
pub struct MessagesResponse {
	pub id: String,
	pub r#type: &'static str,
	pub role: &'static str,
	pub content: Vec<AssistantContentBlock>,
	pub model: String,
	pub stop_reason: Option<StopReason>,
	pub stop_sequence: Option<String>,
	pub usage: AnthropicUsage,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
	EndTurn,
	MaxTokens,
	#[expect(dead_code, reason = "part of the Anthropic API schema")]
	StopSequence,
	ToolUse,
	#[expect(dead_code, reason = "part of the Anthropic API schema")]
	PauseTurn,
	#[expect(dead_code, reason = "part of the Anthropic API schema")]
	Refusal,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnthropicUsage {
	pub input_tokens: u64,
	pub output_tokens: u64,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub cache_creation_input_tokens: Option<u64>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub cache_read_input_tokens: Option<u64>,
}

// --- Streaming Events ---

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
	#[serde(rename = "message_start")]
	MessageStart { message: MessageStartBody },

	#[serde(rename = "content_block_start")]
	ContentBlockStart {
		index: u32,
		content_block: ContentBlockStartBody,
	},

	#[serde(rename = "content_block_delta")]
	ContentBlockDelta { index: u32, delta: ContentDelta },

	#[serde(rename = "content_block_stop")]
	ContentBlockStop { index: u32 },

	#[serde(rename = "message_delta")]
	MessageDelta {
		delta: MessageDeltaBody,
		#[serde(skip_serializing_if = "Option::is_none")]
		usage: Option<AnthropicUsage>,
	},

	#[serde(rename = "message_stop")]
	MessageStop {},

	#[serde(rename = "ping")]
	#[expect(dead_code, reason = "part of the Anthropic SSE protocol")]
	Ping {},

	#[serde(rename = "error")]
	#[expect(dead_code, reason = "part of the Anthropic SSE protocol")]
	Error { error: StreamError },
}

impl StreamEvent {
	pub fn event_type(&self) -> &'static str {
		match self {
			Self::MessageStart { .. } => "message_start",
			Self::ContentBlockStart { .. } => "content_block_start",
			Self::ContentBlockDelta { .. } => "content_block_delta",
			Self::ContentBlockStop { .. } => "content_block_stop",
			Self::MessageDelta { .. } => "message_delta",
			Self::MessageStop { .. } => "message_stop",
			Self::Ping { .. } => "ping",
			Self::Error { .. } => "error",
		}
	}
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageStartBody {
	pub id: String,
	pub r#type: &'static str,
	pub role: &'static str,
	pub content: Vec<()>,
	pub model: String,
	pub stop_reason: Option<StopReason>,
	pub stop_sequence: Option<String>,
	pub usage: AnthropicUsage,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ContentBlockStartBody {
	#[serde(rename = "text")]
	Text { text: String },
	#[serde(rename = "tool_use")]
	ToolUse {
		id: String,
		name: String,
		input: serde_json::Value,
	},
	#[serde(rename = "thinking")]
	#[expect(dead_code, reason = "part of the Anthropic API schema")]
	Thinking { thinking: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ContentDelta {
	#[serde(rename = "text_delta")]
	Text { text: String },
	#[serde(rename = "input_json_delta")]
	InputJson { partial_json: String },
	#[serde(rename = "thinking_delta")]
	#[expect(dead_code, reason = "part of the Anthropic API schema")]
	Thinking { thinking: String },
	#[serde(rename = "signature_delta")]
	#[expect(dead_code, reason = "part of the Anthropic API schema")]
	Signature { signature: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageDeltaBody {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub stop_reason: Option<StopReason>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub stop_sequence: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamError {
	pub r#type: String,
	pub message: String,
}

// --- Stream State Machine ---

pub struct StreamState {
	pub message_start_sent: bool,
	pub content_block_index: u32,
	pub content_block_open: bool,
	pub tool_calls: HashMap<u32, ToolCallState>,
}

pub struct ToolCallState {
	#[expect(dead_code, reason = "stored for diagnostics")]
	pub id: String,
	#[expect(dead_code, reason = "stored for diagnostics")]
	pub name: String,
	pub anthropic_block_index: u32,
}

impl StreamState {
	pub fn new() -> Self {
		Self {
			message_start_sent: false,
			content_block_index: 0,
			content_block_open: false,
			tool_calls: HashMap::new(),
		}
	}

	pub fn is_tool_block_open(&self) -> bool {
		if !self.content_block_open {
			return false;
		}
		self.tool_calls
			.values()
			.any(|tc| tc.anthropic_block_index == self.content_block_index)
	}
}
