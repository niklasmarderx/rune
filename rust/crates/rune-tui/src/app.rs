use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::event::TuiEvent;
use crate::runtime_bridge::RuntimeWorker;

/// Operational mode of the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    /// User can type input.
    Input,
    /// A turn is running in the background.
    Waiting,
}

/// A single block in the conversation history.
pub struct ConversationBlock {
    pub role: String,
    pub content: String,
}

/// Status of a tool invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Running,
    Succeeded,
    Failed,
}

/// Tracks one tool invocation in the tool activity panel.
pub struct ToolActivity {
    pub name: String,
    pub status: ToolStatus,
}

/// Data shown in the status bar.
pub struct StatusBar {
    pub model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_create_tokens: u32,
    pub cost_usd: f64,
    pub turn_count: u32,
    pub message_count: usize,
}

/// Main application state.
pub struct App {
    pub mode: AppMode,
    pub conversation: Vec<ConversationBlock>,
    pub streaming_text: String,
    pub tool_activity: Vec<ToolActivity>,
    pub input_buffer: String,
    pub input_cursor: usize,
    pub status: StatusBar,
    /// Lines scrolled UP from the bottom. 0 = following tail (auto-scroll).
    pub scroll_up: u16,
    pub tick: usize,
    pub git_branch: String,
    quit: bool,
    event_tx: std::sync::mpsc::Sender<TuiEvent>,
    turn_start: Option<Instant>,
    input_history: Vec<String>,
    history_index: Option<usize>,
    shared_runtime: Option<RuntimeWorker>,
}

impl App {
    pub fn new(event_tx: std::sync::mpsc::Sender<TuiEvent>) -> Self {
        let git_branch = resolve_git_branch();
        Self {
            mode: AppMode::Input,
            conversation: Vec::new(),
            streaming_text: String::new(),
            tool_activity: Vec::new(),
            input_buffer: String::new(),
            input_cursor: 0,
            status: StatusBar {
                model: "claude-opus-4-6".to_string(),
                input_tokens: 0,
                output_tokens: 0,
                cache_read_tokens: 0,
                cache_create_tokens: 0,
                cost_usd: 0.0,
                turn_count: 0,
                message_count: 0,
            },
            scroll_up: 0,
            tick: 0,
            git_branch,
            quit: false,
            event_tx,
            turn_start: None,
            input_history: Vec::new(),
            history_index: None,
            shared_runtime: None,
        }
    }

    pub fn set_runtime_worker(&mut self, worker: RuntimeWorker) {
        self.shared_runtime = Some(worker);
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn elapsed_secs(&self) -> Option<f64> {
        self.turn_start.map(|start| start.elapsed().as_secs_f64())
    }

    pub fn handle_event(&mut self, event: TuiEvent) {
        match event {
            TuiEvent::Key(key) => self.handle_key(key),
            TuiEvent::TextDelta(text) => {
                self.streaming_text.push_str(&text);
                // Auto-scroll to bottom when new content arrives.
                self.scroll_up = 0;
            }
            TuiEvent::ToolUseStarted { name } => {
                self.tool_activity.push(ToolActivity {
                    name,
                    status: ToolStatus::Running,
                });
            }
            TuiEvent::ToolResultReceived { name, is_error, .. } => {
                if let Some(tool) = self
                    .tool_activity
                    .iter_mut()
                    .rev()
                    .find(|t| t.name == name && t.status == ToolStatus::Running)
                {
                    tool.status = if is_error {
                        ToolStatus::Failed
                    } else {
                        ToolStatus::Succeeded
                    };
                }
            }
            TuiEvent::Usage(usage) => {
                self.status.input_tokens =
                    self.status.input_tokens.saturating_add(usage.input_tokens);
                self.status.output_tokens = self
                    .status
                    .output_tokens
                    .saturating_add(usage.output_tokens);
                self.status.cache_read_tokens = self
                    .status
                    .cache_read_tokens
                    .saturating_add(usage.cache_read_input_tokens);
                self.status.cache_create_tokens = self
                    .status
                    .cache_create_tokens
                    .saturating_add(usage.cache_creation_input_tokens);
                self.status.cost_usd =
                    runtime::pricing_for_model(&self.status.model).map_or(0.0, |pricing| {
                        let cumulative = runtime::TokenUsage {
                            input_tokens: self.status.input_tokens,
                            output_tokens: self.status.output_tokens,
                            cache_creation_input_tokens: self.status.cache_create_tokens,
                            cache_read_input_tokens: self.status.cache_read_tokens,
                        };
                        cumulative
                            .estimate_cost_usd_with_pricing(pricing)
                            .total_cost_usd()
                    });
            }
            TuiEvent::TurnComplete(result) => {
                let text = std::mem::take(&mut self.streaming_text);
                if !text.is_empty() {
                    self.conversation.push(ConversationBlock {
                        role: "assistant".to_string(),
                        content: text,
                    });
                }
                if let Err(error) = result {
                    self.conversation.push(ConversationBlock {
                        role: "error".to_string(),
                        content: error.to_string(),
                    });
                }
                self.tool_activity.clear();
                self.turn_start = None;
                self.status.turn_count += 1;

                // Update message count from runtime.
                if let Some(worker) = &self.shared_runtime {
                    self.status.message_count = worker.message_count();
                }

                self.mode = AppMode::Input;
            }
            TuiEvent::PromptCache(_) | TuiEvent::Resize(_, _) => {}
            TuiEvent::Tick => {
                self.tick = self.tick.wrapping_add(1);
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        // Ctrl+C always quits.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }

        match self.mode {
            AppMode::Input => self.handle_input_key(key),
            AppMode::Waiting => {
                if key.code == KeyCode::Esc {
                    // Finalize what we have so far.
                    let text = std::mem::take(&mut self.streaming_text);
                    if !text.is_empty() {
                        self.conversation.push(ConversationBlock {
                            role: "assistant".to_string(),
                            content: format!("{text}\n[cancelled]"),
                        });
                    }
                    self.tool_activity.clear();
                    self.turn_start = None;
                    self.mode = AppMode::Input;
                }
            }
        }
    }

    fn handle_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    // Shift+Enter inserts a newline for multi-line input.
                    self.input_buffer.insert(self.input_cursor, '\n');
                    self.input_cursor += 1;
                } else {
                    self.submit_input();
                }
            }
            KeyCode::Char(c) => {
                self.input_buffer.insert(self.input_cursor, c);
                self.input_cursor += c.len_utf8();
                self.history_index = None;
            }
            KeyCode::Backspace => {
                if self.input_cursor > 0 {
                    let prev = self.input_buffer[..self.input_cursor]
                        .char_indices()
                        .next_back()
                        .map_or(0, |(i, _)| i);
                    self.input_buffer.drain(prev..self.input_cursor);
                    self.input_cursor = prev;
                }
            }
            KeyCode::Left => {
                if self.input_cursor > 0 {
                    self.input_cursor = self.input_buffer[..self.input_cursor]
                        .char_indices()
                        .next_back()
                        .map_or(0, |(i, _)| i);
                }
            }
            KeyCode::Right => {
                if self.input_cursor < self.input_buffer.len() {
                    self.input_cursor = self.input_buffer[self.input_cursor..]
                        .char_indices()
                        .nth(1)
                        .map_or(self.input_buffer.len(), |(i, _)| self.input_cursor + i);
                }
            }
            KeyCode::Home => {
                self.input_cursor = 0;
            }
            KeyCode::End => {
                self.input_cursor = self.input_buffer.len();
            }
            KeyCode::Up => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    // Shift+Up scrolls conversation up (away from bottom).
                    self.scroll_up = self.scroll_up.saturating_add(3);
                } else if !self.input_history.is_empty() {
                    // Up navigates input history.
                    let idx = match self.history_index {
                        Some(i) => i.saturating_sub(1),
                        None => self.input_history.len() - 1,
                    };
                    self.history_index = Some(idx);
                    self.input_buffer = self.input_history[idx].clone();
                    self.input_cursor = self.input_buffer.len();
                }
            }
            KeyCode::Down => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    // Shift+Down scrolls conversation down (toward bottom).
                    self.scroll_up = self.scroll_up.saturating_sub(3);
                } else if let Some(idx) = self.history_index {
                    if idx + 1 < self.input_history.len() {
                        let next = idx + 1;
                        self.history_index = Some(next);
                        self.input_buffer = self.input_history[next].clone();
                        self.input_cursor = self.input_buffer.len();
                    } else {
                        // Past the end of history — clear input.
                        self.history_index = None;
                        self.input_buffer.clear();
                        self.input_cursor = 0;
                    }
                }
            }
            KeyCode::PageUp => {
                self.scroll_up = self.scroll_up.saturating_add(10);
            }
            KeyCode::PageDown => {
                self.scroll_up = self.scroll_up.saturating_sub(10);
            }
            KeyCode::Esc => {
                self.quit = true;
            }
            _ => {}
        }
    }

    fn submit_input(&mut self) {
        let input = self.input_buffer.trim().to_string();
        if input.is_empty() {
            return;
        }

        // Save to input history (skip duplicates of last entry).
        if self.input_history.last() != Some(&input) {
            self.input_history.push(input.clone());
        }
        self.history_index = None;

        // Handle local commands.
        match input.as_str() {
            "/exit" | "/quit" => {
                self.quit = true;
                return;
            }
            "/clear" => {
                self.conversation.clear();
                self.streaming_text.clear();
                self.input_buffer.clear();
                self.input_cursor = 0;
                self.scroll_up = 0;
                return;
            }
            "/help" => {
                self.conversation.push(ConversationBlock {
                    role: "system".to_string(),
                    content: [
                        "Available commands:",
                        "  /help   — Show this help",
                        "  /clear  — Clear conversation display",
                        "  /status — Show session status",
                        "  /exit   — Quit the TUI",
                        "",
                        "Shortcuts:",
                        "  Esc          — Quit (or cancel running turn)",
                        "  Ctrl+C       — Force quit",
                        "  Up/Down      — Input history",
                        "  Shift+Up/Dn  — Scroll conversation",
                        "  PgUp/PgDn    — Scroll conversation (fast)",
                        "  Home/End     — Jump to start/end of input",
                    ]
                    .join("\n"),
                });
                self.input_buffer.clear();
                self.input_cursor = 0;
                return;
            }
            "/status" => {
                let status_text = format!(
                    "Model: {}\nTurns: {}\nMessages: {}\nTokens: {} in / {} out\nCache: {} read / {} create\nCost: ${:.4}\nBranch: {}",
                    self.status.model,
                    self.status.turn_count,
                    self.status.message_count,
                    self.status.input_tokens,
                    self.status.output_tokens,
                    self.status.cache_read_tokens,
                    self.status.cache_create_tokens,
                    self.status.cost_usd,
                    self.git_branch,
                );
                self.conversation.push(ConversationBlock {
                    role: "system".to_string(),
                    content: status_text,
                });
                self.input_buffer.clear();
                self.input_cursor = 0;
                return;
            }
            _ => {}
        }

        // Add user message to conversation.
        self.conversation.push(ConversationBlock {
            role: "user".to_string(),
            content: input.clone(),
        });
        self.input_buffer.clear();
        self.input_cursor = 0;
        self.scroll_up = 0;
        self.mode = AppMode::Waiting;
        self.turn_start = Some(Instant::now());

        // Submit turn to the runtime worker.
        if let Some(worker) = &self.shared_runtime {
            worker.submit_turn(input);
        } else {
            let _ = self
                .event_tx
                .send(TuiEvent::TurnComplete(Err(runtime::RuntimeError::new(
                    "Runtime not initialized",
                ))));
        }
    }
}

fn resolve_git_branch() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_default()
}
