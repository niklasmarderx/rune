use std::env;
use std::sync::{Arc, Mutex};

use api::{
    ContentBlockDelta, MessageRequest, OutputContentBlock, PromptCache, ProviderClient,
    StreamEvent as ApiStreamEvent, ToolChoice,
};
use runtime::{
    ApiClient, ApiRequest, AssistantEvent, ConfigLoader, ContentBlock, ConversationMessage,
    ConversationRuntime, McpServerManager, MessageRole, PermissionMode, PermissionPolicy,
    PromptCacheEvent, RuntimeError, Session, ToolError, ToolExecutor,
};
use serde_json::json;
use tools::GlobalToolRegistry;

use api::{InputContentBlock, InputMessage, ToolResultContentBlock};

use crate::event::TuiEvent;

// ---------------------------------------------------------------------------
// TuiApiClient — implements ApiClient, sends events through channel
// ---------------------------------------------------------------------------

pub struct TuiApiClient {
    rt: tokio::runtime::Runtime,
    client: ProviderClient,
    model: String,
    enable_tools: bool,
    tool_registry: GlobalToolRegistry,
    event_tx: std::sync::mpsc::Sender<TuiEvent>,
}

impl TuiApiClient {
    pub fn new(
        session_id: &str,
        model: String,
        enable_tools: bool,
        tool_registry: GlobalToolRegistry,
        event_tx: std::sync::mpsc::Sender<TuiEvent>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let auth = resolve_auth();
        let client = ProviderClient::from_model_with_anthropic_auth(&model, auth)?
            .with_prompt_cache(PromptCache::new(session_id));
        Ok(Self {
            rt: tokio::runtime::Runtime::new()?,
            client,
            model,
            enable_tools,
            tool_registry,
            event_tx,
        })
    }
}

impl ApiClient for TuiApiClient {
    #[allow(clippy::too_many_lines)]
    fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
        let tools = if self.enable_tools {
            let specs = self.tool_registry.definitions(None);
            (!specs.is_empty()).then_some(specs)
        } else {
            None
        };

        let message_request = MessageRequest {
            model: self.model.clone(),
            max_tokens: max_tokens_for_model(&self.model),
            messages: convert_messages(&request.messages),
            system: (!request.system_prompt.is_empty()).then(|| request.system_prompt.join("\n\n")),
            tools,
            tool_choice: self.enable_tools.then_some(ToolChoice::Auto),
            stream: true,
            reasoning_effort: None,
        };

        self.rt.block_on(async {
            let mut stream = self
                .client
                .stream_message(&message_request)
                .await
                .map_err(|e| RuntimeError::new(e.to_string()))?;

            let mut events = Vec::new();
            let mut pending_tool: Option<(String, String, String)> = None;
            let mut saw_stop = false;
            let mut thinking_text = String::new();

            while let Some(event) = stream
                .next_event()
                .await
                .map_err(|e| RuntimeError::new(e.to_string()))?
            {
                match event {
                    ApiStreamEvent::MessageStart(start) => {
                        for block in start.message.content {
                            process_output_block(
                                block,
                                &mut events,
                                &mut pending_tool,
                                &self.event_tx,
                                true,
                            );
                        }
                    }
                    ApiStreamEvent::ContentBlockStart(start) => {
                        process_output_block(
                            start.content_block,
                            &mut events,
                            &mut pending_tool,
                            &self.event_tx,
                            true,
                        );
                    }
                    ApiStreamEvent::ContentBlockDelta(delta) => match delta.delta {
                        ContentBlockDelta::TextDelta { text } => {
                            if !text.is_empty() {
                                let _ = self.event_tx.send(TuiEvent::TextDelta(text.clone()));
                                events.push(AssistantEvent::TextDelta(text));
                            }
                        }
                        ContentBlockDelta::InputJsonDelta { partial_json } => {
                            if let Some((_, _, input)) = &mut pending_tool {
                                input.push_str(&partial_json);
                            }
                        }
                        ContentBlockDelta::ThinkingDelta { thinking } => {
                            thinking_text.push_str(&thinking);
                        }
                        ContentBlockDelta::SignatureDelta { .. } => {}
                    },
                    ApiStreamEvent::ContentBlockStop(_) => {
                        if let Some((id, name, input)) = pending_tool.take() {
                            let _ = self
                                .event_tx
                                .send(TuiEvent::ToolUseStarted { name: name.clone() });
                            events.push(AssistantEvent::ToolUse { id, name, input });
                        }
                    }
                    ApiStreamEvent::MessageDelta(delta) => {
                        let usage = delta.usage.token_usage();
                        let _ = self.event_tx.send(TuiEvent::Usage(usage));
                        events.push(AssistantEvent::Usage(usage));
                    }
                    ApiStreamEvent::MessageStop(_) => {
                        saw_stop = true;
                        events.push(AssistantEvent::MessageStop);
                    }
                }
            }

            // Promote thinking text if no visible content.
            let has_content = events.iter().any(|e| {
                matches!(e, AssistantEvent::TextDelta(t) if !t.is_empty())
                    || matches!(e, AssistantEvent::ToolUse { .. })
            });
            if !has_content && !thinking_text.is_empty() {
                let _ = self
                    .event_tx
                    .send(TuiEvent::TextDelta(thinking_text.clone()));
                events.push(AssistantEvent::TextDelta(thinking_text));
                if !saw_stop {
                    events.push(AssistantEvent::MessageStop);
                }
            }

            if !saw_stop && has_content {
                events.push(AssistantEvent::MessageStop);
            }

            if events
                .iter()
                .any(|e| matches!(e, AssistantEvent::MessageStop))
            {
                push_prompt_cache_record(&self.client, &mut events);
                return Ok(events);
            }

            // Non-streaming fallback.
            let response = self
                .client
                .send_message(&MessageRequest {
                    stream: false,
                    ..message_request.clone()
                })
                .await
                .map_err(|e| RuntimeError::new(e.to_string()))?;

            let mut fb_events = Vec::new();
            let mut fb_pending = None;
            for block in response.content {
                process_output_block(
                    block,
                    &mut fb_events,
                    &mut fb_pending,
                    &self.event_tx,
                    false,
                );
                if let Some((id, name, input)) = fb_pending.take() {
                    fb_events.push(AssistantEvent::ToolUse { id, name, input });
                }
            }
            fb_events.push(AssistantEvent::Usage(response.usage.token_usage()));
            fb_events.push(AssistantEvent::MessageStop);
            push_prompt_cache_record(&self.client, &mut fb_events);
            Ok(fb_events)
        })
    }
}

// ---------------------------------------------------------------------------
// TuiToolExecutor — implements ToolExecutor, sends events through channel
// ---------------------------------------------------------------------------

pub struct TuiToolExecutor {
    tool_registry: GlobalToolRegistry,
    mcp_state: Option<Arc<Mutex<MiniMcpState>>>,
    event_tx: std::sync::mpsc::Sender<TuiEvent>,
}

pub struct MiniMcpState {
    pub rt: tokio::runtime::Runtime,
    pub manager: McpServerManager,
}

impl MiniMcpState {
    pub fn call_tool(
        &mut self,
        name: &str,
        args: Option<serde_json::Value>,
    ) -> Result<String, ToolError> {
        let response = self
            .rt
            .block_on(self.manager.call_tool(name, args))
            .map_err(|e| ToolError::new(e.to_string()))?;
        if let Some(error) = response.error {
            return Err(ToolError::new(format!(
                "MCP tool `{name}` error: {} ({})",
                error.message, error.code
            )));
        }
        let result = response
            .result
            .ok_or_else(|| ToolError::new(format!("MCP tool `{name}` returned no result")))?;
        serde_json::to_string_pretty(&result).map_err(|e| ToolError::new(e.to_string()))
    }

    pub fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.rt.block_on(self.manager.shutdown())?;
        Ok(())
    }
}

impl ToolExecutor for TuiToolExecutor {
    fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError> {
        let value: serde_json::Value = serde_json::from_str(input)
            .map_err(|e| ToolError::new(format!("invalid tool JSON: {e}")))?;

        let result = if self.tool_registry.has_runtime_tool(tool_name) {
            let Some(mcp) = &mut self.mcp_state else {
                return Err(ToolError::new("MCP not available"));
            };
            let mut state = mcp
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.call_tool(tool_name, Some(value))
        } else {
            self.tool_registry
                .execute(tool_name, &value)
                .map_err(ToolError::new)
        };

        match &result {
            Ok(output) => {
                let _ = self.event_tx.send(TuiEvent::ToolResultReceived {
                    name: tool_name.to_string(),
                    output: truncate(output, 200),
                    is_error: false,
                });
            }
            Err(error) => {
                let _ = self.event_tx.send(TuiEvent::ToolResultReceived {
                    name: tool_name.to_string(),
                    output: error.to_string(),
                    is_error: true,
                });
            }
        }

        result
    }
}

// ---------------------------------------------------------------------------
// Persistent runtime — built once on a dedicated worker thread, reused across turns
// ---------------------------------------------------------------------------

/// Holds everything needed to run multiple turns without rebuilding.
struct PersistentRuntime {
    runtime: ConversationRuntime<TuiApiClient, TuiToolExecutor>,
    mcp_state: Option<Arc<Mutex<MiniMcpState>>>,
}

impl PersistentRuntime {
    /// Build the runtime from config. Called once at startup.
    fn build(tx: &std::sync::mpsc::Sender<TuiEvent>) -> Result<Self, RuntimeError> {
        let cwd = env::current_dir().map_err(|e| RuntimeError::new(e.to_string()))?;
        let loader = ConfigLoader::default_for(&cwd);
        let runtime_config = loader
            .load()
            .map_err(|e| RuntimeError::new(e.to_string()))?;

        // Plugins
        let plugin_manager_config =
            plugins::PluginManagerConfig::new(loader.config_home().to_path_buf());
        let plugin_manager = plugins::PluginManager::new(plugin_manager_config);
        let plugin_registry = plugin_manager
            .plugin_registry()
            .map_err(|e| RuntimeError::new(e.to_string()))?;
        let plugin_hooks = plugin_registry
            .aggregated_hooks()
            .map_err(|e| RuntimeError::new(e.to_string()))?;
        let hook_config = runtime::RuntimeHookConfig::new(
            plugin_hooks.pre_tool_use,
            plugin_hooks.post_tool_use,
            plugin_hooks.post_tool_use_failure,
        );
        let feature_config = runtime_config
            .feature_config()
            .clone()
            .with_hooks(runtime_config.hooks().merged(&hook_config));

        // MCP
        let mut mcp_state_arc: Option<Arc<Mutex<MiniMcpState>>> = None;
        let mut mcp_runtime_tools = Vec::new();
        let mut mcp_manager = McpServerManager::from_runtime_config(&runtime_config);
        if !mcp_manager.server_names().is_empty() {
            let mcp_rt =
                tokio::runtime::Runtime::new().map_err(|e| RuntimeError::new(e.to_string()))?;
            let discovery = mcp_rt.block_on(mcp_manager.discover_tools_best_effort());
            for tool in &discovery.tools {
                mcp_runtime_tools.push(tools::RuntimeToolDefinition {
                    name: tool.qualified_name.clone(),
                    description: tool
                        .tool
                        .description
                        .clone()
                        .or_else(|| Some(format!("MCP tool `{}`", tool.qualified_name))),
                    input_schema: tool.tool.input_schema.clone().unwrap_or_else(
                        || json!({ "type": "object", "additionalProperties": true }),
                    ),
                    required_permission: PermissionMode::DangerFullAccess,
                });
            }
            mcp_state_arc = Some(Arc::new(Mutex::new(MiniMcpState {
                rt: mcp_rt,
                manager: mcp_manager,
            })));
        }

        // Tool registry
        let tool_registry = GlobalToolRegistry::with_plugin_tools(
            plugin_registry
                .aggregated_tools()
                .map_err(|e| RuntimeError::new(e.to_string()))?,
        )
        .map_err(|e| RuntimeError::new(e.clone()))?
        .with_runtime_tools(mcp_runtime_tools)
        .map_err(|e| RuntimeError::new(e.clone()))?;

        // Permission policy
        let mode = PermissionMode::DangerFullAccess;
        let policy = tool_registry
            .permission_specs(None)
            .map_err(RuntimeError::new)?
            .into_iter()
            .fold(
                PermissionPolicy::new(mode)
                    .with_permission_rules(feature_config.permission_rules()),
                |p, (name, req)| p.with_tool_requirement(name, req),
            );

        let session_id = "tui-session";
        let model = "claude-opus-4-6".to_string();
        let system_prompt = vec!["You are a helpful AI assistant.".to_string()];

        let api_client =
            TuiApiClient::new(session_id, model, true, tool_registry.clone(), tx.clone())
                .map_err(|e| RuntimeError::new(e.to_string()))?;

        let tool_executor = TuiToolExecutor {
            tool_registry,
            mcp_state: mcp_state_arc.clone(),
            event_tx: tx.clone(),
        };

        let conversation_runtime = ConversationRuntime::new_with_features(
            Session::new(),
            api_client,
            tool_executor,
            policy,
            system_prompt,
            &feature_config,
        );

        Ok(Self {
            runtime: conversation_runtime,
            mcp_state: mcp_state_arc,
        })
    }

    fn run_turn(&mut self, input: &str) -> Result<runtime::TurnSummary, RuntimeError> {
        self.runtime.run_turn(input, None)
    }

    fn message_count(&self) -> usize {
        self.runtime.session().messages.len()
    }
}

impl Drop for PersistentRuntime {
    fn drop(&mut self) {
        if let Some(mcp) = &self.mcp_state {
            if let Ok(mut state) = mcp.lock() {
                let _ = state.shutdown();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// RuntimeWorker — owns PersistentRuntime on a dedicated thread
// ---------------------------------------------------------------------------

enum WorkerCommand {
    RunTurn(String),
    QueryMessageCount(std::sync::mpsc::Sender<usize>),
}

/// Handle to the runtime worker thread. Send commands, runtime stays on one thread.
pub struct RuntimeWorker {
    cmd_tx: std::sync::mpsc::Sender<WorkerCommand>,
}

impl RuntimeWorker {
    /// Build the runtime on the current thread, then spawn a worker thread.
    /// Returns an error if the runtime cannot be constructed.
    pub fn start(event_tx: std::sync::mpsc::Sender<TuiEvent>) -> Result<Self, RuntimeError> {
        // Build runtime on the main thread (before raw mode) so errors print normally.
        let mut persistent = PersistentRuntime::build(&event_tx)?;

        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<WorkerCommand>();

        std::thread::spawn(move || {
            while let Ok(cmd) = cmd_rx.recv() {
                match cmd {
                    WorkerCommand::RunTurn(input) => {
                        let result = persistent.run_turn(&input);
                        let _ = event_tx.send(TuiEvent::TurnComplete(result));
                    }
                    WorkerCommand::QueryMessageCount(reply) => {
                        let _ = reply.send(persistent.message_count());
                    }
                }
            }
            // Channel closed — worker shuts down, PersistentRuntime drops (cleans up MCP).
        });

        Ok(Self { cmd_tx })
    }

    /// Submit a turn to be executed. Non-blocking; result arrives via `TuiEvent`.
    pub fn submit_turn(&self, input: String) {
        let _ = self.cmd_tx.send(WorkerCommand::RunTurn(input));
    }

    /// Query the current session message count (blocks briefly for reply).
    pub fn message_count(&self) -> usize {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        let _ = self.cmd_tx.send(WorkerCommand::QueryMessageCount(reply_tx));
        reply_rx.recv().unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn resolve_auth() -> Option<api::AuthSource> {
    if let Ok(key) = env::var("ANTHROPIC_API_KEY") {
        return Some(api::AuthSource::ApiKey(key));
    }
    None
}

fn max_tokens_for_model(model: &str) -> u32 {
    if model.contains("opus") {
        32_000
    } else {
        64_000
    }
}

fn convert_messages(messages: &[ConversationMessage]) -> Vec<InputMessage> {
    messages
        .iter()
        .filter_map(|msg| {
            let role = match msg.role {
                MessageRole::System | MessageRole::User | MessageRole::Tool => "user",
                MessageRole::Assistant => "assistant",
            };
            let content = msg
                .blocks
                .iter()
                .map(|block| match block {
                    ContentBlock::Text { text } => InputContentBlock::Text { text: text.clone() },
                    ContentBlock::ToolUse { id, name, input } => InputContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: serde_json::from_str(input)
                            .unwrap_or_else(|_| json!({ "raw": input })),
                    },
                    ContentBlock::ToolResult {
                        tool_use_id,
                        output,
                        is_error,
                        ..
                    } => InputContentBlock::ToolResult {
                        tool_use_id: tool_use_id.clone(),
                        content: vec![ToolResultContentBlock::Text {
                            text: output.clone(),
                        }],
                        is_error: *is_error,
                    },
                })
                .collect::<Vec<_>>();
            (!content.is_empty()).then(|| InputMessage {
                role: role.to_string(),
                content,
            })
        })
        .collect()
}

fn process_output_block(
    block: OutputContentBlock,
    events: &mut Vec<AssistantEvent>,
    pending_tool: &mut Option<(String, String, String)>,
    tx: &std::sync::mpsc::Sender<TuiEvent>,
    streaming_input: bool,
) {
    match block {
        OutputContentBlock::Text { text } => {
            if !text.is_empty() {
                let _ = tx.send(TuiEvent::TextDelta(text.clone()));
                events.push(AssistantEvent::TextDelta(text));
            }
        }
        OutputContentBlock::ToolUse { id, name, input } => {
            let initial_input = if streaming_input
                && input.is_object()
                && input.as_object().is_some_and(serde_json::Map::is_empty)
            {
                String::new()
            } else {
                input.to_string()
            };
            *pending_tool = Some((id, name, initial_input));
        }
        OutputContentBlock::Thinking { thinking, .. } => {
            if !thinking.is_empty() {
                events.push(AssistantEvent::TextDelta(thinking));
            }
        }
        OutputContentBlock::RedactedThinking { .. } => {}
    }
}

fn push_prompt_cache_record(client: &ProviderClient, events: &mut Vec<AssistantEvent>) {
    if let Some(record) = client.take_last_prompt_cache_record() {
        if let Some(cache_break) = record.cache_break {
            events.push(AssistantEvent::PromptCache(PromptCacheEvent {
                unexpected: cache_break.unexpected,
                reason: cache_break.reason,
                previous_cache_read_input_tokens: cache_break.previous_cache_read_input_tokens,
                current_cache_read_input_tokens: cache_break.current_cache_read_input_tokens,
                token_drop: cache_break.token_drop,
            }));
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}
