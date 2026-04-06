use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::event::TuiEvent;
use crate::runtime_bridge;

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
    pub scroll_offset: u16,
    pub tick: usize,
    quit: bool,
    event_tx: std::sync::mpsc::Sender<TuiEvent>,
    turn_start: Option<Instant>,
}

impl App {
    pub fn new(event_tx: std::sync::mpsc::Sender<TuiEvent>) -> Self {
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
            },
            scroll_offset: 0,
            tick: 0,
            quit: false,
            event_tx,
            turn_start: None,
        }
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
                self.status.input_tokens = usage.input_tokens;
                self.status.output_tokens = usage.output_tokens;
            }
            TuiEvent::TurnComplete(result) => {
                // Finalize: move streaming text into conversation as assistant block.
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
                // While waiting, only allow Esc to (future: cancel turn).
                if key.code == KeyCode::Esc {
                    // TODO: cancel running turn
                }
            }
        }
    }

    fn handle_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.submit_input(),
            KeyCode::Char(c) => {
                self.input_buffer.insert(self.input_cursor, c);
                self.input_cursor += c.len_utf8();
            }
            KeyCode::Backspace => {
                if self.input_cursor > 0 {
                    // Find the previous char boundary.
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
            KeyCode::Up => {
                // Scroll conversation up.
                self.scroll_offset = self.scroll_offset.saturating_add(1);
            }
            KeyCode::Down => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
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

        // Handle local commands.
        if input == "/exit" || input == "/quit" {
            self.quit = true;
            return;
        }

        // Add user message to conversation.
        self.conversation.push(ConversationBlock {
            role: "user".to_string(),
            content: input.clone(),
        });
        self.input_buffer.clear();
        self.input_cursor = 0;
        self.scroll_offset = 0;
        self.mode = AppMode::Waiting;
        self.turn_start = Some(Instant::now());

        // Spawn the turn on a background thread.
        let tx = self.event_tx.clone();
        std::thread::spawn(move || {
            runtime_bridge::run_turn_background(&input, &tx);
        });
    }
}
