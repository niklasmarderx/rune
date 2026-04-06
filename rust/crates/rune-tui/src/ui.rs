use std::fmt::Write as _;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, AppMode, ToolStatus};
use crate::widgets::markdown;

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

    // Dynamic input height: 3 lines base, grows with newlines (up to 8).
    #[allow(clippy::cast_possible_truncation)]
    let input_lines = app.input_buffer.lines().count().max(1) as u16;
    let input_height = (input_lines + 2).min(8); // +2 for borders

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),            // status bar
            Constraint::Min(5),               // conversation
            Constraint::Length(tool_height),  // tool activity
            Constraint::Length(input_height), // input bar
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

    let branch = if app.git_branch.is_empty() {
        String::new()
    } else {
        format!(" | \u{e0a0} {}", app.git_branch)
    };

    let cache_info = if app.status.cache_read_tokens > 0 {
        format!(" cache:{}", app.status.cache_read_tokens)
    } else {
        String::new()
    };

    let cost = if app.status.cost_usd > 0.0 {
        format!(" | ${:.4}", app.status.cost_usd)
    } else {
        String::new()
    };

    let content = format!(
        " {} | in:{} out:{}{cache_info}{cost}{branch}{elapsed}",
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
            "system" => (
                "\u{2139}",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ),
            _ => ("???", Style::default()),
        };
        lines.push(Line::from(vec![Span::styled(format!("{label}: "), style)]));
        if block.role == "assistant" {
            lines.extend(markdown::markdown_to_lines(&block.content));
        } else {
            for text_line in block.content.lines() {
                lines.push(Line::from(text_line.to_string()));
            }
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
        lines.extend(markdown::markdown_to_lines(&app.streaming_text));
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
            "Type /help for commands. Press Esc or Ctrl+C to quit.",
            Style::default().fg(Color::DarkGray),
        )));
    }

    #[allow(clippy::cast_possible_truncation)]
    let total_lines = lines.len() as u16;
    let visible = area.height.saturating_sub(2); // account for borders
    let max_scroll = total_lines.saturating_sub(visible);
    // scroll_up=0 means follow tail (show bottom), scroll_up>0 means scrolled up.
    let scroll = max_scroll.saturating_sub(app.scroll_up);

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
                ToolStatus::Succeeded => ("\u{2713}", Color::Green),
                ToolStatus::Failed => ("\u{2717}", Color::Red),
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
    let display_text = match app.mode {
        AppMode::Input => {
            // Multi-line: prefix first line with "> ", indent continuation lines.
            let mut result = String::new();
            for (i, line) in app.input_buffer.split('\n').enumerate() {
                if i == 0 {
                    let _ = write!(result, " > {line}");
                } else {
                    let _ = write!(result, "\n   {line}");
                }
            }
            if result.is_empty() {
                " > ".to_string()
            } else {
                result
            }
        }
        AppMode::Waiting => {
            let spinner = SPINNER_FRAMES[app.tick % SPINNER_FRAMES.len()];
            format!(" {spinner} Thinking... (Esc to cancel)")
        }
    };

    let style = match app.mode {
        AppMode::Input => Style::default(),
        AppMode::Waiting => Style::default().fg(Color::DarkGray),
    };

    let input = Paragraph::new(display_text)
        .style(style)
        .block(Block::default().borders(Borders::ALL).title(" Input "));
    frame.render_widget(input, area);

    // Set cursor position when in input mode.
    if app.mode == AppMode::Input {
        // Find which line and column the cursor is on.
        let before_cursor = &app.input_buffer[..app.input_cursor];
        let cursor_line = before_cursor.matches('\n').count();
        let last_newline = before_cursor.rfind('\n').map_or(0, |pos| pos + 1);
        let col_in_line = app.input_cursor - last_newline;
        // +1 for border, +3 for " > " prefix (or "   " on continuation lines)
        #[allow(clippy::cast_possible_truncation)]
        let cursor_x = area.x + 1 + 3 + col_in_line as u16;
        #[allow(clippy::cast_possible_truncation)]
        let cursor_y = area.y + 1 + cursor_line as u16;
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}
