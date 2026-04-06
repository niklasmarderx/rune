use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent};
use runtime::{PromptCacheEvent, RuntimeError, TokenUsage, TurnSummary};

/// Events flowing between the background runtime and the TUI render loop.
pub enum TuiEvent {
    /// Streaming text chunk from the model.
    TextDelta(String),
    /// A tool call has started.
    ToolUseStarted { name: String },
    /// A tool call has completed.
    #[allow(dead_code)]
    ToolResultReceived {
        name: String,
        output: String,
        is_error: bool,
    },
    /// Token usage update from the model.
    Usage(TokenUsage),
    /// Prompt cache telemetry.
    #[allow(dead_code)]
    PromptCache(PromptCacheEvent),
    /// The background turn has completed.
    TurnComplete(Result<TurnSummary, RuntimeError>),
    /// A terminal key event.
    Key(KeyEvent),
    /// Terminal was resized.
    #[allow(dead_code)]
    Resize(u16, u16),
    /// Periodic tick for animations.
    Tick,
}

/// Polls crossterm events and tick timer, forwarding into an mpsc channel.
pub struct EventLoop {
    rx: mpsc::Receiver<TuiEvent>,
}

impl EventLoop {
    /// Spawn the event polling thread. Returns the loop handle and a sender
    /// that the runtime bridge can use to inject events.
    pub fn new(tick_rate: Duration) -> (Self, mpsc::Sender<TuiEvent>) {
        let (tx, rx) = mpsc::channel();
        let input_tx = tx.clone();

        thread::spawn(move || {
            loop {
                if event::poll(tick_rate).unwrap_or(false) {
                    match event::read() {
                        Ok(Event::Key(key)) => {
                            if input_tx.send(TuiEvent::Key(key)).is_err() {
                                break;
                            }
                        }
                        Ok(Event::Resize(w, h)) => {
                            if input_tx.send(TuiEvent::Resize(w, h)).is_err() {
                                break;
                            }
                        }
                        _ => {}
                    }
                } else {
                    // No input within tick_rate — send a tick for animations.
                    if input_tx.send(TuiEvent::Tick).is_err() {
                        break;
                    }
                }
            }
        });

        (Self { rx }, tx)
    }

    /// Consume the event loop and return the receiver for the main loop.
    pub fn into_receiver(self) -> mpsc::Receiver<TuiEvent> {
        self.rx
    }
}
