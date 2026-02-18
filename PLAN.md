# first-officer

Reverse-engineered GitHub Copilot API proxy exposing OpenAI and Anthropic
compatible endpoints, written in Rust.

## Scope

In scope:
- Auth: accept a GitHub token via env var, exchange for Copilot token, refresh
- OpenAI-compatible proxy: /v1/chat/completions (streaming + non-streaming),
  /v1/models
- Anthropic-compatible translation: /v1/messages (streaming + non-streaming)
- Health check endpoint

Out of scope (for now):
- Token counting / count_tokens endpoint (return dummy values if needed)
- Embeddings
- Usage/quota display
- Rate limiting / manual approval
- CLI interactivity / Claude Code integration
- Device code OAuth flow (require GH_TOKEN from env)

## Architecture

```
src/
  main.rs           - entry point, env config, server startup
  state.rs          - shared app state (tokens, models, vscode version)
  copilot/
    mod.rs
    types.rs        - OpenAI-format request/response types (serde)
    api.rs          - header construction, base URL logic
    client.rs       - chat completions, models (reqwest calls)
  auth/
    mod.rs
    token.rs        - copilot token exchange + refresh loop
  translate/
    mod.rs
    types.rs        - Anthropic API types (serde)
    request.rs      - Anthropic request → OpenAI request
    response.rs     - OpenAI response → Anthropic response (non-streaming)
    stream.rs       - OpenAI SSE chunks → Anthropic SSE events (state machine)
  routes/
    mod.rs
    completions.rs  - POST /v1/chat/completions (passthrough)
    models.rs       - GET /v1/models (passthrough)
    messages.rs     - POST /v1/messages (translate + proxy)
    health.rs       - GET /
```

## Dependency choices

- axum: HTTP server + routing
- reqwest + reqwest-eventsource: HTTP client + SSE consumption
- tokio: async runtime
- serde + serde_json: serialisation
- tracing + tracing-subscriber: logging
- uuid: request IDs for Copilot headers
- futures: stream combinators for SSE
- tower-http: CORS middleware
- thiserror: error types

## Implementation order

### 1. Types (copilot/types.rs, translate/types.rs)

Define serde structs for:
- OpenAI: ChatCompletionsPayload, Message, Tool, ToolCall, ContentPart,
  ChatCompletionResponse, ChatCompletionChunk, ModelsResponse
- Anthropic: MessagesPayload, AnthropicMessage, ContentBlock variants,
  AnthropicResponse, all stream event types

Use untagged/internally-tagged enums where the API uses discriminated unions
(e.g. content blocks distinguished by `type` field).

### 2. State + config (state.rs, main.rs)

AppState holds:
- copilot_token: RwLock<String> (refreshed periodically)
- github_token: String (from env, immutable)
- account_type: String (from env, default "individual")
- vscode_version: String (cached at startup)
- models: RwLock<Option<ModelsResponse>> (cached at startup)
- reqwest::Client (reused)

Config from env vars:
- GH_TOKEN (required)
- PORT (default 4141)
- ACCOUNT_TYPE (default "individual")
- RUST_LOG (for tracing)

### 3. Auth (auth/token.rs)

- fetch_copilot_token(): POST to api.github.com/copilot_internal/v2/token
  with github headers, returns (token, refresh_in)
- spawn_refresh_loop(): tokio::spawn a loop that sleeps for (refresh_in - 60)
  seconds then refreshes, updating state.copilot_token

### 4. Copilot client (copilot/api.rs, copilot/client.rs)

api.rs:
- copilot_base_url(account_type) → URL string
- copilot_headers(token, vscode_version, vision) → HeaderMap
- github_headers(github_token, vscode_version) → HeaderMap
- Constants: COPILOT_VERSION, EDITOR_PLUGIN_VERSION, USER_AGENT, etc.

client.rs:
- chat_completions_raw(client, state, payload_bytes) → reqwest::Response
  Just forward the body, return the raw response for streaming/non-streaming
- get_models(client, state) → ModelsResponse
- fetch_vscode_version(client) → String (scrape latest from update API or
  hardcode a recent version)

### 5. OpenAI routes (routes/completions.rs, routes/models.rs)

completions.rs:
- Extract raw JSON body
- Forward to copilot chat completions endpoint
- If response is streaming (check content-type for text/event-stream),
  stream SSE events back verbatim
- If non-streaming, return JSON response as-is

models.rs:
- Return cached models from state, or proxy through

### 6. Anthropic translation (translate/)

request.rs - translateToOpenAI():
- Convert system prompt (string or text blocks → system message)
- Convert messages: user messages with tool_result blocks split into tool
  messages + user messages; assistant messages with tool_use blocks get
  tool_calls field
- Convert tools: Anthropic tool schema → OpenAI function tool
- Convert tool_choice: auto/any/tool/none mapping
- Model name normalization (claude-sonnet-4-XXXXX → claude-sonnet-4)

response.rs - translateToAnthropic():
- Map choices[0].message to content blocks (text + tool_use)
- Map finish_reason to stop_reason
- Map usage fields (handling cached tokens)

stream.rs - StreamState + translate_chunk():
- State machine tracking: message_start sent, current content block index,
  whether a block is open, active tool calls
- For each OpenAI chunk, emit 0..N Anthropic events:
  - First chunk: emit message_start
  - Text delta: open text block if needed (closing tool block first), emit
    content_block_delta
  - Tool call with id+name: close previous block, emit content_block_start
    for tool_use
  - Tool call arguments: emit content_block_delta with input_json_delta
  - finish_reason present: close open block, emit message_delta + message_stop

### 7. Messages route (routes/messages.rs)

- Parse AnthropicMessagesPayload
- Translate to OpenAI format
- Call copilot chat completions
- If non-streaming: translate response, return JSON
- If streaming: return SSE response, consuming OpenAI SSE stream and emitting
  Anthropic SSE events through the state machine

### 8. Wire it all up (main.rs)

- Read env vars
- Build AppState
- Fetch initial copilot token
- Cache vscode version + models
- Spawn refresh loop
- Build axum Router with all routes + CORS
- Serve

## Container

Dockerfile (two-stage):
- Builder: rust:bookworm, cargo build --release --target aarch64-unknown-linux-musl
- Runner: scratch or alpine, copy binary
- EXPOSE 4141, ENTRYPOINT ["/first-officer"]

For cross-compilation, can also use cargo-zigbuild or cross.

## Testing approach

- Unit tests for translate/request.rs and translate/response.rs with fixture
  JSON payloads
- Unit tests for stream.rs state machine with sequences of chunks
- Integration test: spin up the server with a mock Copilot API, send requests
  through both OpenAI and Anthropic endpoints