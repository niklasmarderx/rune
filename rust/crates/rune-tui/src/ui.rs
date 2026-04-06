use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, AppMode, ToolStatus};

const SPINNER_FRAMES: &[&str] = &[
    "\u{28f7}", "\u{28ef}", "\u{28df}", "\u{287f}", "\u{28bf}", "\u{28fb}", "\u{28fd}", "\u{28fe}",
];

pub fn draw(frame: &mut Frame, app: &App) {
    #[allow(clippy::cast_possible_truncation)]
    let tool_height = if app.tool_activity.is_empty() {
        0
    } else {
        (app.tool_activity.len() as u16 + 2).min(7)
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),           // status bar
            Constraint::Min(5),              // conversation
            Constraint::Length(tool_height), // tool activity
            Constraint::Length(3),           // input bar
        ])
        .split(frame.area());

    render_status_bar(frame, chunks[0], app);
    render_conversation(frame, chunks[1], app);
    if tool_height > 0 {
        render_tool_panel(frame, chunks[2], app);
    }
    render_input(frame, chunks[3], app);
}

fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let elapsed = app
        .elapsed_secs()
        .map_or(String::new(), |s| format!(" | {s:.1}s"));
    let content = format!(
        " {} | in:{} out:{}{elapsed}",
        app.status.model, app.status.input_tokens, app.status.output_tokens,
    );
    let bar = Paragraph::new(content).style(
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(bar, area);
}

fn render_conversation(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines: Vec<Line<'_>> = Vec::new();

    for block in &app.conversation {
        let (label, style) = match block.role.as_str() {
            "user" => (
                "You",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            "assistant" => (
                "Rune",
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
            "error" => (
                "Error",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            _ => ("???", Style::default()),
        };
        lines.push(Line::from(vec![Span::styled(format!("{label}: "), style)]));
        for text_line in block.content.lines() {
            lines.push(Line::from(text_line.to_string()));
        }
        lines.push(Line::from(""));
    }

    // Streaming text (if currently waiting for model).
    if !app.streaming_text.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "Rune: ",
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )]));
        for text_line in app.streaming_text.lines() {
            lines.push(Line::from(text_line.to_string()));
        }
        // Blinking cursor indicator.
        let spinner = SPINNER_FRAMES[app.tick % SPINNER_FRAMES.len()];
        lines.push(Line::from(Span::styled(
            spinner.to_string(),
            Style::default().fg(Color::Yellow),
        )));
    }

    // Welcome message if empty.
    if app.conversation.is_empty() && app.streaming_text.is_empty() {
        lines.push(Line::from(Span::styled(
            "Welcome to Rune TUI. Type a message and press Enter.",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::styled(
            "Press Esc or Ctrl+C to quit. /exit also works.",
            Style::default().fg(Color::DarkGray),
        )));
    }

    #[allow(clippy::cast_possible_truncation)]
    let total_lines = lines.len() as u16;
    let visible = area.height.saturating_sub(2); // account for borders
    let max_scroll = total_lines.saturating_sub(visible);
    let scroll = app.scroll_offset.min(max_scroll);

    let conversation = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Conversation "),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(conversation, area);
}

fn render_tool_panel(frame: &mut Frame, area: Rect, app: &App) {
    let lines: Vec<Line<'_>> = app
        .tool_activity
        .iter()
        .map(|tool| {
            let (icon, color) = match tool.status {
                ToolStatus::Running => {
                    let spinner = SPINNER_FRAMES[app.tick % SPINNER_FRAMES.len()];
                    (spinner, Color::Yellow)
                }
                ToolStatus::Succeeded => ("\u{2713}", Color::Green), // checkmark
                ToolStatus::Failed => ("\u{2717}", Color::Red),      // X mark
            };
            Line::from(vec![
                Span::styled(format!(" {icon} "), Style::default().fg(color)),
                Span::raw(&tool.name),
            ])
        })
        .collect();

    let panel =
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Tools "));
    frame.render_widget(panel, area);
}

fn render_input(frame: &mut Frame, area: Rect, app: &App) {
    let (input_text, style) = match app.mode {
        AppMode::Input => (app.input_buffer.as_str(), Style::default()),
        AppMode::Waiting => {
            let spinner = SPINNER_FRAMES[app.tick % SPINNER_FRAMES.len()];
            // Show spinner while waiting — we have to return a short-lived reference,
            // so we construct the string differently below.
            let _ = spinner;
            ("Thinking...", Style::default().fg(Color::DarkGray))
        }
    };

    let display_text = if app.mode == AppMode::Waiting {
        let spinner = SPINNER_FRAMES[app.tick % SPINNER_FRAMES.len()];
        format!(" {spinner} Thinking...")
    } else {
        format!(" > {input_text}")
    };

    let input = Paragraph::new(display_text)
        .style(style)
        .block(Block::default().borders(Borders::ALL).title(" Input "));
    frame.render_widget(input, area);

    // Set cursor position when in input mode.
    if app.mode == AppMode::Input {
        // +3: border(1) + space(1) + "> "(2) but we used " > " so offset = 4
        #[allow(clippy::cast_possible_truncation)]
        let cursor_x = area.x + 4 + app.input_cursor as u16;
        let cursor_y = area.y + 1; // inside the border
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}
