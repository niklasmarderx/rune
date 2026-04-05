use std::io::{self, Write};

use commands::{render_slash_command_help, resume_supported_slash_commands};

use crate::{
    BUILD_TARGET, DEFAULT_DATE, GIT_SHA, LATEST_SESSION_REFERENCE, PRIMARY_SESSION_EXTENSION,
    VERSION,
};

pub(crate) fn render_repl_help() -> String {
    [
        "REPL".to_string(),
        "  /exit                Quit the REPL".to_string(),
        "  /quit                Quit the REPL".to_string(),
        "  Up/Down              Navigate prompt history".to_string(),
        "  Tab                  Complete commands, modes, and recent sessions".to_string(),
        "  Ctrl-C               Clear input (or exit on empty prompt)".to_string(),
        "  Shift+Enter/Ctrl+J   Insert a newline".to_string(),
        "  Auto-save            .rune/sessions/<session-id>.jsonl".to_string(),
        "  Resume latest        /resume latest".to_string(),
        "  Browse sessions      /session list".to_string(),
        String::new(),
        render_slash_command_help(),
    ]
    .join(
        "
",
    )
}

pub(crate) fn render_version_report() -> String {
    let git_sha = GIT_SHA.unwrap_or("unknown");
    let target = BUILD_TARGET.unwrap_or("unknown");
    format!(
        "Rune Code\n  Version          {VERSION}\n  Git SHA          {git_sha}\n  Target           {target}\n  Build date       {DEFAULT_DATE}"
    )
}

#[allow(clippy::too_many_lines)]
pub(crate) fn print_help_to(out: &mut impl Write) -> io::Result<()> {
    writeln!(out, "rune v{VERSION}")?;
    writeln!(out)?;
    writeln!(out, "Usage:")?;
    writeln!(
        out,
        "  rune [--model MODEL] [--allowedTools TOOL[,TOOL...]]"
    )?;
    writeln!(out, "      Start the interactive REPL")?;
    writeln!(
        out,
        "  rune [--model MODEL] [--output-format text|json] prompt TEXT"
    )?;
    writeln!(out, "      Send one prompt and exit")?;
    writeln!(
        out,
        "  rune [--model MODEL] [--output-format text|json] TEXT"
    )?;
    writeln!(out, "      Shorthand non-interactive prompt mode")?;
    writeln!(
        out,
        "  rune --resume [SESSION.jsonl|session-id|latest] [/status] [/compact] [...]"
    )?;
    writeln!(
        out,
        "      Inspect or maintain a saved session without entering the REPL"
    )?;
    writeln!(out, "  rune help")?;
    writeln!(out, "      Alias for --help")?;
    writeln!(out, "  rune version")?;
    writeln!(out, "      Alias for --version")?;
    writeln!(out, "  rune status")?;
    writeln!(
        out,
        "      Show the current local workspace status snapshot"
    )?;
    writeln!(out, "  rune sandbox")?;
    writeln!(out, "      Show the current sandbox isolation snapshot")?;
    writeln!(out, "  rune dump-manifests")?;
    writeln!(out, "  rune bootstrap-plan")?;
    writeln!(out, "  rune agents")?;
    writeln!(out, "  rune mcp")?;
    writeln!(out, "  rune skills")?;
    writeln!(out, "  rune system-prompt [--cwd PATH] [--date YYYY-MM-DD]")?;
    writeln!(out, "  rune login")?;
    writeln!(out, "  rune logout")?;
    writeln!(out, "  rune init")?;
    writeln!(out)?;
    writeln!(out, "Flags:")?;
    writeln!(
        out,
        "  --model MODEL              Override the active model"
    )?;
    writeln!(
        out,
        "  --output-format FORMAT     Non-interactive output format: text or json"
    )?;
    writeln!(
        out,
        "  --permission-mode MODE     Set read-only, workspace-write, or danger-full-access"
    )?;
    writeln!(
        out,
        "  --dangerously-skip-permissions  Skip all permission checks"
    )?;
    writeln!(out, "  --allowedTools TOOLS       Restrict enabled tools (repeatable; comma-separated aliases supported)")?;
    writeln!(
        out,
        "  --version, -V              Print version and build information locally"
    )?;
    writeln!(out)?;
    writeln!(out, "Interactive slash commands:")?;
    writeln!(out, "{}", render_slash_command_help())?;
    writeln!(out)?;
    let resume_commands = resume_supported_slash_commands()
        .into_iter()
        .map(|spec| match spec.argument_hint {
            Some(argument_hint) => format!("/{} {}", spec.name, argument_hint),
            None => format!("/{}", spec.name),
        })
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(out, "Resume-safe commands: {resume_commands}")?;
    writeln!(out)?;
    writeln!(out, "Session shortcuts:")?;
    writeln!(
        out,
        "  REPL turns auto-save to .rune/sessions/<session-id>.{PRIMARY_SESSION_EXTENSION}"
    )?;
    writeln!(
        out,
        "  Use `{LATEST_SESSION_REFERENCE}` with --resume, /resume, or /session switch to target the newest saved session"
    )?;
    writeln!(
        out,
        "  Use /session list in the REPL to browse managed sessions"
    )?;
    writeln!(out, "Examples:")?;
    writeln!(out, "  rune --model claude-opus \"summarize this repo\"")?;
    writeln!(
        out,
        "  rune --output-format json prompt \"explain src/main.rs\""
    )?;
    writeln!(
        out,
        "  rune --allowedTools read,glob \"summarize Cargo.toml\""
    )?;
    writeln!(out, "  rune --resume {LATEST_SESSION_REFERENCE}")?;
    writeln!(
        out,
        "  rune --resume {LATEST_SESSION_REFERENCE} /status /diff /export notes.txt"
    )?;
    writeln!(out, "  rune agents")?;
    writeln!(out, "  rune mcp show my-server")?;
    writeln!(out, "  rune /skills")?;
    writeln!(out, "  rune login")?;
    writeln!(out, "  rune init")?;
    Ok(())
}

pub(crate) fn print_help() {
    let _ = print_help_to(&mut io::stdout());
}
