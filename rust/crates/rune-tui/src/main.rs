mod app;
mod event;
mod runtime_bridge;
mod ui;
mod widgets;

use std::io;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::App;
use event::EventLoop;

/// RAII guard that restores terminal state on drop.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Install a panic hook that restores the terminal before printing the panic.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        default_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let guard = TerminalGuard;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (event_loop, event_tx) = EventLoop::new(std::time::Duration::from_millis(100));

    let mut app = App::new(event_tx);
    let rx = event_loop.into_receiver();

    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        if let Ok(event) = rx.recv() {
            app.handle_event(event);
            if app.should_quit() {
                break;
            }
        }
    }

    drop(guard);
    Ok(())
}
