use serde::{Deserialize, Serialize};

// --- Chat Completions Request ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionsRequest {
	pub model: String,
	pub messages: Vec<Message>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_tokens: Option<u64>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub temperature: Option<f64>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub top_p: Option<f64>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub stop: Option<Stop>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub stream: Option<bool>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub n: Option<u32>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub frequency_penalty: Option<f64>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub presence_penalty: Option<f64>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tools: Option<Vec<Tool>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tool_choice: Option<ToolChoice>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub user: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Stop {
	Single(String),
	Multiple(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
	pub role: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub content: Option<Content>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub name: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tool_calls: Option<Vec<ToolCall>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Content {
	Text(String),
	Parts(Vec<ContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
	#[serde(rename = "text")]
	Text { text: String },
	#[serde(rename = "image_url")]
	ImageUrl { image_url: ImageUrl },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
	pub url: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
	pub r#type: String,
	pub function: FunctionDef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
	pub name: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub description: Option<String>,
	pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
	String(String),
	Named(NamedToolChoice),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedToolChoice {
	pub r#type: String,
	pub function: NamedToolChoiceFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedToolChoiceFunction {
	pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
	pub id: String,
	pub r#type: String,
	pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
	pub name: String,
	pub arguments: String,
}

// --- Chat Completions Response (non-streaming) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
	pub id: String,
	pub object: String,
	pub created: u64,
	pub model: String,
	pub choices: Vec<Choice>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub system_fingerprint: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
	pub index: u32,
	pub message: ResponseMessage,
	pub finish_reason: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub logprobs: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMessage {
	pub role: String,
	pub content: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
	#[serde(default)]
	pub prompt_tokens: u64,
	#[serde(default)]
	pub completion_tokens: u64,
	#[serde(default)]
	pub total_tokens: u64,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub prompt_tokens_details: Option<PromptTokensDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTokensDetails {
	#[serde(default)]
	pub cached_tokens: u64,
}

// --- Chat Completions Streaming ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
	pub id: String,
	pub object: String,
	pub created: u64,
	pub model: String,
	pub choices: Vec<ChunkChoice>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub system_fingerprint: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkChoice {
	pub index: u32,
	pub delta: Delta,
	pub finish_reason: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub logprobs: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub content: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub role: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tool_calls: Option<Vec<DeltaToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaToolCall {
	pub index: u32,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub id: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub r#type: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub function: Option<DeltaFunction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaFunction {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub name: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub arguments: Option<String>,
}

// --- Models ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsResponse {
	pub data: Vec<Model>,
	pub object: String,
}

// Anthropic-compatible models response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicModelsResponse {
	pub data: Vec<AnthropicModelInfo>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub first_id: Option<String>,
	pub has_more: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub last_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicModelInfo {
	pub id: String,
	pub created_at: String,
	pub display_name: String,
	pub r#type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
	pub id: String,
	pub name: String,
	pub object: String,
	pub vendor: String,
	pub version: String,
	#[serde(default)]
	pub model_picker_enabled: bool,
	#[serde(default)]
	pub preview: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub capabilities: Option<ModelCapabilities>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub policy: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
	pub family: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub limits: Option<ModelLimits>,
	pub object: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub supports: Option<serde_json::Value>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tokenizer: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub r#type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelLimits {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_context_window_tokens: Option<u64>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_output_tokens: Option<u64>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_prompt_tokens: Option<u64>,
}

// --- Copilot Token ---

#[derive(Debug, Clone, Deserialize)]
pub struct CopilotTokenResponse {
	pub token: String,
	pub refresh_in: u64,
	pub expires_at: u64,
}
