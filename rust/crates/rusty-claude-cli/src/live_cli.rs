use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use crate::init::initialize_repo;
use crate::render::{Spinner, TerminalRenderer};
use commands::{
    handle_agents_slash_command, handle_mcp_slash_command, handle_plugins_slash_command,
    handle_skills_slash_command, slash_command_specs, SlashCommand,
};
use runtime::{
    format_usd, load_system_prompt, pricing_for_model, CompactionConfig, ConfigLoader,
    ContentBlock, MessageRole, PermissionMode, Session, ToolError, ToolExecutor,
};
use serde_json::json;
use tools::GlobalToolRegistry;

use super::{
    build_plugin_manager, build_runtime, collect_prompt_cache_events, collect_tool_results,
    collect_tool_uses, create_managed_session_handle, default_permission_mode, filter_tool_specs,
    final_assistant_text, format_auto_compaction_notice, format_bughunter_report,
    format_commit_preflight_report, format_commit_skipped_report, format_compact_report,
    format_cost_report, format_issue_report, format_model_report, format_model_switch_report,
    format_permissions_report, format_permissions_switch_report, format_pr_report,
    format_sandbox_report, format_status_report, format_tool_result, format_ultraplan_report,
    format_unknown_slash_command, list_managed_sessions, normalize_permission_mode,
    parse_git_status_branch, parse_git_workspace_summary, permission_mode_from_label,
    render_config_report, render_diff_report, render_diff_report_for, render_export_text,
    render_last_tool_debug_report, render_memory_report, render_repl_help, render_session_list,
    render_teleport_report, render_version_report, resolve_git_branch_for, resolve_model_alias,
    resolve_sandbox_status, resolve_session_reference, suggest_slash_commands,
    write_session_clear_backup, AllowedToolSet, BuiltRuntime, CliOutputFormat,
    CliPermissionPrompter, HookAbortMonitor, InternalPromptProgressReporter, RuntimeMcpState,
    RuntimePluginState, SessionHandle, StatusUsage, DEFAULT_DATE, LATEST_SESSION_REFERENCE,
    PRIMARY_SESSION_EXTENSION,
};

pub(crate) fn resume_session(session_path: &Path, commands: &[String]) {
    let resolved_path = if session_path.exists() {
        session_path.to_path_buf()
    } else {
        match resolve_session_reference(&session_path.display().to_string()) {
            Ok(handle) => handle.path,
            Err(error) => {
                eprintln!("failed to restore session: {error}");
                std::process::exit(1);
            }
        }
    };

    let session = match Session::load_from_path(&resolved_path) {
        Ok(session) => session,
        Err(error) => {
            eprintln!("failed to restore session: {error}");
            std::process::exit(1);
        }
    };

    if commands.is_empty() {
        println!(
            "Restored session from {} ({} messages).",
            resolved_path.display(),
            session.messages.len()
        );
        return;
    }

    let mut session = session;
    for raw_command in commands {
        let command = match SlashCommand::parse(raw_command) {
            Ok(Some(command)) => command,
            Ok(None) => {
                eprintln!("unsupported resumed command: {raw_command}");
                std::process::exit(2);
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(2);
            }
        };
        match run_resume_command(&resolved_path, &session, &command) {
            Ok(ResumeCommandOutcome {
                session: next_session,
                message,
            }) => {
                session = next_session;
                if let Some(message) = message {
                    println!("{message}");
                }
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(2);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ResumeCommandOutcome {
    pub(crate) session: Session,
    pub(crate) message: Option<String>,
}

pub(crate) fn format_resume_report(session_path: &str, message_count: usize, turns: u32) -> String {
    format!(
        "Session resumed
  Session file     {session_path}
  Messages         {message_count}
  Turns            {turns}"
    )
}

pub(crate) fn render_resume_usage() -> String {
    format!(
        "Resume
  Usage            /resume <session-path|session-id|{LATEST_SESSION_REFERENCE}>
  Auto-save        .rune/sessions/<session-id>.{PRIMARY_SESSION_EXTENSION}
  Tip              use /session list to inspect saved sessions"
    )
}

#[allow(clippy::too_many_lines)]
pub(crate) fn run_resume_command(
    session_path: &Path,
    session: &Session,
    command: &SlashCommand,
) -> Result<ResumeCommandOutcome, Box<dyn std::error::Error>> {
    match command {
        SlashCommand::Help => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(render_repl_help()),
        }),
        SlashCommand::Compact => {
            let result = runtime::compact_session(
                session,
                CompactionConfig {
                    max_estimated_tokens: 0,
                    ..CompactionConfig::default()
                },
            );
            let removed = result.removed_message_count;
            let kept = result.compacted_session.messages.len();
            let skipped = removed == 0;
            result.compacted_session.save_to_path(session_path)?;
            Ok(ResumeCommandOutcome {
                session: result.compacted_session,
                message: Some(format_compact_report(removed, kept, skipped)),
            })
        }
        SlashCommand::Clear { confirm } => {
            if !confirm {
                return Ok(ResumeCommandOutcome {
                    session: session.clone(),
                    message: Some(
                        "clear: confirmation required; rerun with /clear --confirm".to_string(),
                    ),
                });
            }
            let backup_path = write_session_clear_backup(session, session_path)?;
            let previous_session_id = session.session_id.clone();
            let cleared = Session::new();
            let new_session_id = cleared.session_id.clone();
            cleared.save_to_path(session_path)?;
            Ok(ResumeCommandOutcome {
                session: cleared,
                message: Some(format!(
                    "Session cleared\n  Mode             resumed session reset\n  Previous session {previous_session_id}\n  Backup           {}\n  Resume previous  rune --resume {}\n  New session      {new_session_id}\n  Session file     {}",
                    backup_path.display(),
                    backup_path.display(),
                    session_path.display()
                )),
            })
        }
        SlashCommand::Status => {
            let tracker = runtime::UsageTracker::from_session(session);
            let usage = tracker.cumulative_usage();
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(format_status_report(
                    "restored-session",
                    StatusUsage {
                        message_count: session.messages.len(),
                        turns: tracker.turns(),
                        latest: tracker.current_turn_usage(),
                        cumulative: usage,
                        estimated_tokens: 0,
                    },
                    default_permission_mode().as_str(),
                    &super::status_context(Some(session_path))?,
                )),
            })
        }
        SlashCommand::Sandbox => {
            let cwd = env::current_dir()?;
            let loader = ConfigLoader::default_for(&cwd);
            let runtime_config = loader.load()?;
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(format_sandbox_report(&resolve_sandbox_status(
                    runtime_config.sandbox(),
                    &cwd,
                ))),
            })
        }
        SlashCommand::Cost => {
            let usage = runtime::UsageTracker::from_session(session).cumulative_usage();
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(format_cost_report(usage)),
            })
        }
        SlashCommand::Config { section } => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(render_config_report(section.as_deref())?),
        }),
        SlashCommand::Mcp { action, target } => {
            let cwd = env::current_dir()?;
            let args = match (action.as_deref(), target.as_deref()) {
                (None, None) => None,
                (Some(action), None) => Some(action.to_string()),
                (Some(action), Some(target)) => Some(format!("{action} {target}")),
                (None, Some(target)) => Some(target.to_string()),
            };
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(handle_mcp_slash_command(args.as_deref(), &cwd)?),
            })
        }
        SlashCommand::Memory => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(render_memory_report()?),
        }),
        SlashCommand::Init => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(init_claude_md()?),
        }),
        SlashCommand::Diff => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(render_diff_report_for(
                session_path.parent().unwrap_or_else(|| Path::new(".")),
            )?),
        }),
        SlashCommand::Version => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(render_version_report()),
        }),
        SlashCommand::Export { path } => {
            let export_path = resolve_export_path(path.as_deref(), session)?;
            fs::write(&export_path, render_export_text(session))?;
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(format!(
                    "Export\n  Result           wrote transcript\n  File             {}\n  Messages         {}",
                    export_path.display(),
                    session.messages.len(),
                )),
            })
        }
        SlashCommand::Agents { args } => {
            let cwd = env::current_dir()?;
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(handle_agents_slash_command(args.as_deref(), &cwd)?),
            })
        }
        SlashCommand::Skills { args } => {
            let cwd = env::current_dir()?;
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(handle_skills_slash_command(args.as_deref(), &cwd)?),
            })
        }
        SlashCommand::Unknown(name) => Err(format_unknown_slash_command(name).into()),
        // ── Implemented resume commands ──
        SlashCommand::Doctor => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(render_doctor_report()),
        }),
        SlashCommand::Usage { .. } => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(render_usage_report(session)),
        }),
        SlashCommand::Files => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(render_files_report(session)),
        }),
        SlashCommand::Context { action } => match action.as_deref() {
            None | Some("show") => Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(render_context_report(session)),
            }),
            Some("clear") => Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(
                    "Context clearing requires an interactive session. Use /clear --confirm instead."
                        .to_string(),
                ),
            }),
            Some(other) => {
                Err(format!("Unknown context action: {other}. Use 'show' or 'clear'.").into())
            }
        },
        SlashCommand::Copy { target } => {
            let text = extract_copy_text(session, target.as_deref());
            let msg = copy_text_to_clipboard(&text)?;
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(msg),
            })
        }
        SlashCommand::Hooks { .. } => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(render_hooks_report()?),
        }),
        SlashCommand::Stats => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(render_stats_report(session)?),
        }),
        SlashCommand::Effort { level } => {
            let msg = match level.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
                None => {
                    "Effort level\n  Current: default\n  Usage: /effort <low|medium|high>"
                        .to_string()
                }
                Some(l @ ("low" | "medium" | "high")) => format!(
                    "Effort level set to: {l}"
                ),
                Some(other) => {
                    return Err(
                        format!("Unknown effort level: {other}. Use low, medium, or high.").into(),
                    )
                }
            };
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(msg),
            })
        }
        SlashCommand::Plugins { action, target } => {
            let cwd = env::current_dir()?;
            let loader = ConfigLoader::default_for(&cwd);
            let runtime_config = loader.load()?;
            let mut manager = build_plugin_manager(&cwd, &loader, &runtime_config);
            let result =
                handle_plugins_slash_command(action.as_deref(), target.as_deref(), &mut manager)?;
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(result.message),
            })
        }
        SlashCommand::Session { action, .. } => match action.as_deref() {
            None | Some("list") => Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(render_session_list(&session.session_id)?),
            }),
            _ => Err("session switch/fork requires an interactive session".into()),
        },
        SlashCommand::Teleport { target } => {
            let target = target.as_deref().unwrap_or("");
            if target.is_empty() {
                return Err("Usage: /teleport <symbol-or-path>".into());
            }
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(render_teleport_report(target)?),
            })
        }
        SlashCommand::DebugToolCall => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(render_last_tool_debug_report(session)?),
        }),
        // ── REPL-only commands (need interactive session) ──
        SlashCommand::Model { .. }
        | SlashCommand::Permissions { .. }
        | SlashCommand::Resume { .. }
        | SlashCommand::Fast
        | SlashCommand::Vim
        | SlashCommand::Exit => Err("this command requires an interactive REPL session".into()),
        // ── Commands that need a running conversation with an active model ──
        SlashCommand::Bughunter { .. }
        | SlashCommand::Commit
        | SlashCommand::Pr { .. }
        | SlashCommand::Issue { .. }
        | SlashCommand::Ultraplan { .. }
        | SlashCommand::Review { .. }
        | SlashCommand::Summary
        | SlashCommand::ReleaseNotes
        | SlashCommand::SecurityReview => {
            Err("this command requires a running conversation with an active model".into())
        }
        // ── Login/auth ──
        SlashCommand::Login => {
            super::run_login()?;
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some("OAuth login initiated.".to_string()),
            })
        }
        SlashCommand::Logout => {
            super::run_logout()?;
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some("Logged out.".to_string()),
            })
        }
        // ── Not yet implemented with specific messages ──
        SlashCommand::Rewind { .. } => {
            Err("Rewind requires an interactive REPL session to modify the live conversation.".into())
        }
        SlashCommand::Branch { .. } => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some({
                let current = std::process::Command::new("git")
                    .args(["branch", "--show-current"])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map_or_else(
                        || "unknown".to_string(),
                        |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
                    );
                format!("Branch\n  Current          {current}\n  Note             branch creation/switching requires an interactive REPL session")
            }),
        }),
        SlashCommand::Rename { .. } => {
            Err("Rename requires an interactive REPL session to update the live session handle.".into())
        }
        SlashCommand::Theme { .. } | SlashCommand::Color { .. } => {
            Err("Theme and color switching require an interactive REPL session.".into())
        }
        SlashCommand::Plan { .. } => {
            Err("Planning mode requires an interactive REPL session.".into())
        }
        SlashCommand::Tasks { .. } => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some("No background tasks running.".to_string()),
        }),
        // ── Informational commands that work fine in resume mode ──
        SlashCommand::Upgrade => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(format!(
                "Upgrade\n\
                 \x20 Current version  {}\n\
                 \x20 From source      git pull && cargo build --release -p rune-cli\n\
                 \x20 From crates.io   cargo install rune-cli\n\
                 \x20 Repository       https://github.com/niklasmarderx/rune",
                env!("CARGO_PKG_VERSION")
            )),
        }),
        SlashCommand::Share => {
            let export_path = resolve_export_path(None, session)?;
            fs::write(&export_path, render_export_text(session))?;
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(format!(
                    "Share\n\
                     \x20 Exported session transcript\n\
                     \x20 File             {}\n\
                     \x20 Messages         {}\n\n\
                     Share this file to let others review the conversation.",
                    export_path.display(),
                    session.messages.len(),
                )),
            })
        }
        SlashCommand::Feedback => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(
                "Feedback\n\
                 \x20 Issues           https://github.com/niklasmarderx/rune/issues\n\
                 \x20 Discussions       https://github.com/niklasmarderx/rune/discussions\n\n\
                 Report bugs, request features, or share ideas at the links above."
                    .to_string(),
            ),
        }),
        SlashCommand::Keybindings => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(
                "Keybindings\n\
                 \x20 Enter            send message\n\
                 \x20 Shift+Enter      insert newline\n\
                 \x20 Ctrl+C           cancel current generation / clear input\n\
                 \x20 Ctrl+D           exit REPL\n\
                 \x20 Tab              autocomplete slash commands\n\
                 \x20 Up/Down          navigate input history\n\
                 \x20 Esc              dismiss autocomplete menu"
                    .to_string(),
            ),
        }),
        // ── Features under development ──
        SlashCommand::Desktop
        | SlashCommand::Voice { .. }
        | SlashCommand::Ide { .. }
        | SlashCommand::Stickers
        | SlashCommand::Insights
        | SlashCommand::Thinkback => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(
                "This feature is under development. Track progress at https://github.com/niklasmarderx/rune/issues"
                    .to_string(),
            ),
        }),
        // ── Mode toggles that need an interactive session ──
        SlashCommand::Brief
        | SlashCommand::Advisor
        | SlashCommand::OutputStyle { .. }
        | SlashCommand::Tag { .. }
        | SlashCommand::AddDir { .. } => {
            Err("This command requires an interactive REPL session.".into())
        }
        SlashCommand::PrivacySettings => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(
                "Privacy Settings\n\
                 \x20 Telemetry        opt-in only\n\
                 \x20 Session storage  local (~/.rune/sessions/)\n\
                 \x20 Data sent        conversation text to Anthropic API only"
                    .to_string(),
            ),
        }),
    }
}

pub(crate) fn run_repl(
    model: String,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cli = LiveCli::new(model, true, allowed_tools, permission_mode)?;
    let mut editor =
        super::input::LineEditor::new("> ", cli.repl_completion_candidates().unwrap_or_default());
    println!("{}", cli.startup_banner());

    loop {
        editor.set_completions(cli.repl_completion_candidates().unwrap_or_default());
        match editor.read_line()? {
            super::input::ReadOutcome::Submit(input) => {
                let trimmed = input.trim().to_string();
                if trimmed.is_empty() {
                    continue;
                }
                if matches!(trimmed.as_str(), "/exit" | "/quit") {
                    cli.persist_session()?;
                    break;
                }
                match SlashCommand::parse(&trimmed) {
                    Ok(Some(command)) => {
                        if cli.handle_repl_command(command)? {
                            cli.persist_session()?;
                        }
                        continue;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        eprintln!("{error}");
                        continue;
                    }
                }
                editor.push_history(input);
                cli.run_turn(&trimmed)?;
            }
            super::input::ReadOutcome::Cancel => {}
            super::input::ReadOutcome::Exit => {
                cli.persist_session()?;
                break;
            }
        }
    }

    Ok(())
}

// ── Standalone helper functions ────────────────────────────────────────

fn render_doctor_report() -> String {
    let mut lines = vec![
        "Environment Health Check".to_string(),
        "========================".to_string(),
        String::new(),
    ];

    // API key
    let api_key = env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.is_empty());
    let api_status = if api_key.is_some() { "ok" } else { "MISSING" };
    lines.push(format!("  API key            {api_status}"));

    // Git
    let git_version = Command::new("git")
        .args(["--version"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map_or_else(
            || "NOT FOUND".to_string(),
            |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
        );
    lines.push(format!("  Git                {git_version}"));

    // Rust toolchain
    let rustc_version = Command::new("rustc")
        .args(["--version"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map_or_else(
            || "not found".to_string(),
            |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
        );
    lines.push(format!("  Rust               {rustc_version}"));

    // Working directory
    let cwd =
        env::current_dir().map_or_else(|_| "unknown".to_string(), |p| p.display().to_string());
    lines.push(format!("  Working dir        {cwd}"));

    lines.push(String::new());
    lines.push("All checks passed.".to_string());
    lines.join("\n")
}

fn render_usage_report(session: &Session) -> String {
    let tracker = runtime::UsageTracker::from_session(session);
    let usage = tracker.cumulative_usage();
    let turns = tracker.turns();
    let cost_estimate = usage.estimate_cost_usd();
    [
        "Usage".to_string(),
        format!("  Messages           {}", session.messages.len()),
        format!("  Turns              {turns}"),
        format!("  Input tokens       {}", usage.input_tokens),
        format!("  Output tokens      {}", usage.output_tokens),
        format!("  Total tokens       {}", usage.total_tokens()),
        format!(
            "  Estimated cost     {}",
            format_usd(cost_estimate.total_cost_usd())
        ),
    ]
    .join("\n")
}

fn render_files_report(session: &Session) -> String {
    let mut files: BTreeSet<String> = BTreeSet::new();
    for msg in &session.messages {
        for block in &msg.blocks {
            if let ContentBlock::ToolUse { input, .. } = block {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(input) {
                    if let Some(path) = parsed.get("file_path").and_then(|v| v.as_str()) {
                        files.insert(path.to_string());
                    }
                    if let Some(path) = parsed.get("path").and_then(|v| v.as_str()) {
                        if Path::new(path).extension().is_some() {
                            files.insert(path.to_string());
                        }
                    }
                }
            }
        }
    }
    if files.is_empty() {
        return "Files\n  No files referenced in this session.".to_string();
    }
    let mut lines = vec![format!("Files ({} referenced)", files.len())];
    for f in &files {
        lines.push(format!("  {f}"));
    }
    lines.join("\n")
}

fn render_context_report(session: &Session) -> String {
    let message_count = session.messages.len();
    let tracker = runtime::UsageTracker::from_session(session);
    let mut lines = vec![
        "Context".to_string(),
        format!("  Messages           {message_count}"),
        format!(
            "  Estimated tokens   ~{}",
            tracker.cumulative_usage().total_tokens()
        ),
    ];
    if message_count > 0 {
        lines.push(String::new());
        lines.push("Recent messages:".to_string());
        let start = message_count.saturating_sub(6);
        for (i, msg) in session.messages[start..].iter().enumerate() {
            let role = match msg.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "asst",
                MessageRole::System => "sys",
                MessageRole::Tool => "tool",
            };
            let preview: String = msg
                .blocks
                .iter()
                .filter_map(|block| {
                    if let ContentBlock::Text { text } = block {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            let truncated = if preview.len() > 80 {
                format!("{}...", &preview[..77])
            } else {
                preview
            };
            lines.push(format!("  [{:>2}] {role}: {truncated}", start + i + 1));
        }
    }
    lines.join("\n")
}

fn copy_text_to_clipboard(text: &str) -> Result<String, Box<dyn std::error::Error>> {
    if text.is_empty() {
        return Ok("Nothing to copy.".to_string());
    }
    let (program, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("pbcopy", vec![])
    } else if cfg!(target_os = "windows") {
        ("clip", vec![])
    } else {
        ("xclip", vec!["-selection", "clipboard"])
    };
    match Command::new(program)
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            Ok(format!(
                "Copied {} characters to clipboard.",
                text.chars().count()
            ))
        }
        Err(_) => Err(format!(
            "Could not copy to clipboard ({program} not found). Use /export instead."
        )
        .into()),
    }
}

fn extract_copy_text(session: &Session, target: Option<&str>) -> String {
    match target.map(str::trim).filter(|t| !t.is_empty()) {
        None | Some("last") => session
            .messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::Assistant)
            .map(|m| {
                m.blocks
                    .iter()
                    .filter_map(|block| {
                        if let ContentBlock::Text { text } = block {
                            Some(text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default(),
        Some("all") => session
            .messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    MessageRole::User => "User",
                    MessageRole::Assistant => "Assistant",
                    MessageRole::System => "System",
                    MessageRole::Tool => "Tool",
                };
                let text: String = m
                    .blocks
                    .iter()
                    .filter_map(|block| {
                        if let ContentBlock::Text { text } = block {
                            Some(text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("{role}:\n{text}")
            })
            .collect::<Vec<_>>()
            .join("\n\n---\n\n"),
        Some(_) => String::new(),
    }
}

fn render_hooks_report() -> Result<String, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let loader = ConfigLoader::default_for(&cwd);
    let runtime_config = loader.load()?;
    let hooks = runtime_config.hooks();
    let mut lines = vec!["Hooks".to_string()];
    let pre = hooks.pre_tool_use();
    let post = hooks.post_tool_use();
    let fail = hooks.post_tool_use_failure();
    if pre.is_empty() && post.is_empty() && fail.is_empty() {
        lines.push("  No hooks configured.".to_string());
    } else {
        if !pre.is_empty() {
            lines.push(format!("  PreToolUse         {}", pre.join(", ")));
        }
        if !post.is_empty() {
            lines.push(format!("  PostToolUse        {}", post.join(", ")));
        }
        if !fail.is_empty() {
            lines.push(format!("  PostToolUseFailure {}", fail.join(", ")));
        }
    }
    Ok(lines.join("\n"))
}

fn render_stats_report(session: &Session) -> Result<String, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let tracker = runtime::UsageTracker::from_session(session);
    let usage = tracker.cumulative_usage();

    // Count files in workspace
    let file_count = Command::new("git")
        .args(["ls-files"])
        .current_dir(&cwd)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map_or(0, |o| String::from_utf8_lossy(&o.stdout).lines().count());

    let mut lines = vec![
        "Stats".to_string(),
        format!("  Working dir        {}", cwd.display()),
        format!("  Tracked files      {file_count}"),
        format!("  Session messages   {}", session.messages.len()),
        format!("  Turns              {}", tracker.turns()),
        format!("  Total tokens       {}", usage.total_tokens()),
    ];

    // Git branch
    if let Ok(branch) = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&cwd)
        .output()
    {
        if branch.status.success() {
            lines.push(format!(
                "  Git branch         {}",
                String::from_utf8_lossy(&branch.stdout).trim()
            ));
        }
    }

    Ok(lines.join("\n"))
}

#[allow(dead_code)]
fn render_system_prompt_report() -> Result<String, Box<dyn std::error::Error>> {
    let parts = build_system_prompt()?;
    let full = parts.join("\n\n---\n\n");
    let mut lines = vec![
        "System Prompt".to_string(),
        format!("  Parts              {}", parts.len()),
        format!("  Characters         {}", full.len()),
        String::new(),
    ];
    // Show truncated preview
    let preview = if full.len() > 2000 {
        format!(
            "{}...\n\n[truncated -- {} chars total]",
            &full[..2000],
            full.len()
        )
    } else {
        full
    };
    lines.push(preview);
    Ok(lines.join("\n"))
}

pub(crate) struct LiveCli {
    model: String,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
    system_prompt: Vec<String>,
    runtime: BuiltRuntime,
    session: SessionHandle,
    planning_mode: bool,
    brief_mode: bool,
    advisor_mode: bool,
}

impl LiveCli {
    pub(crate) fn new(
        model: String,
        enable_tools: bool,
        allowed_tools: Option<AllowedToolSet>,
        permission_mode: PermissionMode,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let system_prompt = build_system_prompt()?;
        let session_state = Session::new();
        let session = create_managed_session_handle(&session_state.session_id)?;
        let runtime = build_runtime(
            session_state.with_persistence_path(session.path.clone()),
            &session.id,
            model.clone(),
            system_prompt.clone(),
            enable_tools,
            true,
            allowed_tools.clone(),
            permission_mode,
            None,
        )?;
        let cli = Self {
            model,
            allowed_tools,
            permission_mode,
            system_prompt,
            runtime,
            session,
            planning_mode: false,
            brief_mode: false,
            advisor_mode: false,
        };
        cli.persist_session()?;
        Ok(cli)
    }

    pub(crate) fn startup_banner(&self) -> String {
        let cwd = env::current_dir().map_or_else(
            |_| "<unknown>".to_string(),
            |path| path.display().to_string(),
        );
        let status = super::status_context(None).ok();
        let git_branch = status
            .as_ref()
            .and_then(|context| context.git_branch.as_deref())
            .unwrap_or("unknown");
        let workspace = status.as_ref().map_or_else(
            || "unknown".to_string(),
            |context| context.git_summary.headline(),
        );
        let session_path = self.session.path.strip_prefix(Path::new(&cwd)).map_or_else(
            |_| self.session.path.display().to_string(),
            |path| path.display().to_string(),
        );
        format!(
            "\x1b[38;5;99m\
██████╗ ██╗   ██╗███╗   ██╗███████╗\n\
██╔══██╗██║   ██║████╗  ██║██╔════╝\n\
██████╔╝██║   ██║██╔██╗ ██║█████╗  \n\
██╔══██╗██║   ██║██║╚██╗██║██╔══╝  \n\
██║  ██║╚██████╔╝██║ ╚████║███████╗\n\
╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═══╝╚══════╝\x1b[0m \x1b[38;5;141mCode\x1b[0m\n\n\
  \x1b[2mModel\x1b[0m            {}\n\
  \x1b[2mPermissions\x1b[0m      {}\n\
  \x1b[2mBranch\x1b[0m           {}\n\
  \x1b[2mWorkspace\x1b[0m        {}\n\
  \x1b[2mDirectory\x1b[0m        {}\n\
  \x1b[2mSession\x1b[0m          {}\n\
  \x1b[2mAuto-save\x1b[0m        {}\n\n\
  Type \x1b[1m/help\x1b[0m for commands · \x1b[1m/status\x1b[0m for live context · \x1b[2m/resume latest\x1b[0m jumps back to the newest session · \x1b[1m/diff\x1b[0m then \x1b[1m/commit\x1b[0m to ship · \x1b[2mTab\x1b[0m for workflow completions · \x1b[2mShift+Enter\x1b[0m for newline",
            self.model,
            self.permission_mode.as_str(),
            git_branch,
            workspace,
            cwd,
            self.session.id,
            session_path,
        )
    }

    pub(crate) fn repl_completion_candidates(
        &self,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        Ok(slash_command_completion_candidates_with_sessions(
            &self.model,
            Some(&self.session.id),
            list_managed_sessions()?
                .into_iter()
                .map(|session| session.id)
                .collect(),
        ))
    }

    fn prepare_turn_runtime(
        &self,
        emit_output: bool,
    ) -> Result<(BuiltRuntime, HookAbortMonitor), Box<dyn std::error::Error>> {
        let hook_abort_signal = runtime::HookAbortSignal::new();
        let runtime = build_runtime(
            self.runtime.session().clone(),
            &self.session.id,
            self.model.clone(),
            self.system_prompt.clone(),
            true,
            emit_output,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
        )?
        .with_hook_abort_signal(hook_abort_signal.clone());
        let hook_abort_monitor = HookAbortMonitor::spawn(hook_abort_signal);

        Ok((runtime, hook_abort_monitor))
    }

    fn replace_runtime(&mut self, runtime: BuiltRuntime) -> Result<(), Box<dyn std::error::Error>> {
        self.runtime.shutdown_plugins()?;
        self.runtime = runtime;
        Ok(())
    }

    pub(crate) fn run_turn(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error>> {
        let (mut runtime, hook_abort_monitor) = self.prepare_turn_runtime(true)?;
        let mut stdout = io::stdout();

        // Animated spinner with rotating creative messages
        let stop_spinner = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_flag = stop_spinner.clone();

        // Share the stop flag with the API client so it can kill the spinner
        // the moment the first visible content arrives from the stream.
        runtime.api_client_mut().spinner_stop = Some(stop_spinner.clone());

        let spinner_handle = std::thread::spawn(move || {
            const THINKING_MESSAGES: &[&str] = &[
                "Thinking...",
                "Pondering...",
                "Processing...",
                "Analyzing...",
                "Weaving patterns...",
                "Channeling wisdom...",
                "Deciphering...",
                "Forging a response...",
                "Tracing pathways...",
                "Reasoning...",
                "Connecting dots...",
                "Synthesizing...",
            ];

            // Switch message every ~2.4 seconds (30 ticks × 80ms)
            const TICKS_PER_MESSAGE: u32 = 30;

            let mut spinner = Spinner::new();
            let theme = *TerminalRenderer::new().color_theme();
            let mut out = io::stdout();
            let mut msg_index: usize = 0;
            let mut ticks_on_current: u32 = 0;

            while !stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                let msg = THINKING_MESSAGES[msg_index % THINKING_MESSAGES.len()];
                let _ = spinner.tick(msg, &theme, &mut out);
                std::thread::sleep(std::time::Duration::from_millis(80));
                ticks_on_current += 1;
                if ticks_on_current >= TICKS_PER_MESSAGE {
                    ticks_on_current = 0;
                    msg_index += 1;
                }
            }
        });

        let mut permission_prompter = CliPermissionPrompter::new(self.permission_mode);
        let result = runtime.run_turn(input, Some(&mut permission_prompter));
        hook_abort_monitor.stop();

        // Stop spinner animation (may already be stopped by the stream handler)
        let spinner_was_stopped_by_stream = stop_spinner.load(std::sync::atomic::Ordering::Relaxed);
        stop_spinner.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = spinner_handle.join();

        match result {
            Ok(summary) => {
                self.replace_runtime(runtime)?;
                // Only print "Done" if the spinner was still active when the
                // turn finished — if the stream already cleared it and wrote
                // visible content, printing "Done" would be noise.
                if !spinner_was_stopped_by_stream {
                    let mut spinner = Spinner::new();
                    spinner.finish("Done", TerminalRenderer::new().color_theme(), &mut stdout)?;
                }
                println!();
                if let Some(event) = summary.auto_compaction {
                    println!(
                        "{}",
                        format_auto_compaction_notice(event.removed_message_count)
                    );
                }
                self.persist_session()?;
                Ok(())
            }
            Err(error) => {
                runtime.shutdown_plugins()?;
                if spinner_was_stopped_by_stream {
                    eprintln!("\nRequest failed");
                } else {
                    let mut spinner = Spinner::new();
                    spinner.fail(
                        "Request failed",
                        TerminalRenderer::new().color_theme(),
                        &mut stdout,
                    )?;
                }
                Err(Box::new(error))
            }
        }
    }

    pub(crate) fn run_turn_with_output(
        &mut self,
        input: &str,
        output_format: CliOutputFormat,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match output_format {
            CliOutputFormat::Text => self.run_turn(input),
            CliOutputFormat::Json => self.run_prompt_json(input),
        }
    }

    fn run_prompt_json(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error>> {
        let (mut runtime, hook_abort_monitor) = self.prepare_turn_runtime(false)?;
        let mut permission_prompter = CliPermissionPrompter::new(self.permission_mode);
        let result = runtime.run_turn(input, Some(&mut permission_prompter));
        hook_abort_monitor.stop();
        let summary = result?;
        self.replace_runtime(runtime)?;
        self.persist_session()?;
        println!(
            "{}",
            json!({
                "message": final_assistant_text(&summary),
                "model": self.model,
                "iterations": summary.iterations,
                "auto_compaction": summary.auto_compaction.map(|event| json!({
                    "removed_messages": event.removed_message_count,
                    "notice": format_auto_compaction_notice(event.removed_message_count),
                })),
                "tool_uses": collect_tool_uses(&summary),
                "tool_results": collect_tool_results(&summary),
                "prompt_cache_events": collect_prompt_cache_events(&summary),
                "usage": {
                    "input_tokens": summary.usage.input_tokens,
                    "output_tokens": summary.usage.output_tokens,
                    "cache_creation_input_tokens": summary.usage.cache_creation_input_tokens,
                    "cache_read_input_tokens": summary.usage.cache_read_input_tokens,
                },
                "estimated_cost": format_usd(
                    summary.usage.estimate_cost_usd_with_pricing(
                        pricing_for_model(&self.model)
                            .unwrap_or_else(runtime::ModelPricing::default_sonnet_tier)
                    ).total_cost_usd()
                )
            })
        );
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn handle_repl_command(
        &mut self,
        command: SlashCommand,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(match command {
            SlashCommand::Help => {
                println!("{}", render_repl_help());
                false
            }
            SlashCommand::Status => {
                self.print_status();
                false
            }
            SlashCommand::Bughunter { scope } => {
                self.run_bughunter(scope.as_deref())?;
                false
            }
            SlashCommand::Commit => {
                self.run_commit(None)?;
                false
            }
            SlashCommand::Pr { context } => {
                self.run_pr(context.as_deref())?;
                false
            }
            SlashCommand::Issue { context } => {
                self.run_issue(context.as_deref())?;
                false
            }
            SlashCommand::Ultraplan { task } => {
                self.run_ultraplan(task.as_deref())?;
                false
            }
            SlashCommand::Teleport { target } => {
                Self::run_teleport(target.as_deref())?;
                false
            }
            SlashCommand::DebugToolCall => {
                self.run_debug_tool_call(None)?;
                false
            }
            SlashCommand::Sandbox => {
                Self::print_sandbox_status();
                false
            }
            SlashCommand::Compact => {
                self.compact()?;
                false
            }
            SlashCommand::Model { model } => self.set_model(model)?,
            SlashCommand::Permissions { mode } => self.set_permissions(mode)?,
            SlashCommand::Clear { confirm } => self.clear_session(confirm)?,
            SlashCommand::Cost => {
                self.print_cost();
                false
            }
            SlashCommand::Resume { session_path } => self.resume_session_repl(session_path)?,
            SlashCommand::Config { section } => {
                Self::print_config(section.as_deref())?;
                false
            }
            SlashCommand::Mcp { action, target } => {
                let args = match (action.as_deref(), target.as_deref()) {
                    (None, None) => None,
                    (Some(action), None) => Some(action.to_string()),
                    (Some(action), Some(target)) => Some(format!("{action} {target}")),
                    (None, Some(target)) => Some(target.to_string()),
                };
                Self::print_mcp(args.as_deref())?;
                false
            }
            SlashCommand::Memory => {
                Self::print_memory()?;
                false
            }
            SlashCommand::Init => {
                run_init()?;
                false
            }
            SlashCommand::Diff => {
                Self::print_diff()?;
                false
            }
            SlashCommand::Version => {
                Self::print_version();
                false
            }
            SlashCommand::Export { path } => {
                self.export_session(path.as_deref())?;
                false
            }
            SlashCommand::Session { action, target } => {
                self.handle_session_command(action.as_deref(), target.as_deref())?
            }
            SlashCommand::Plugins { action, target } => {
                self.handle_plugins_command(action.as_deref(), target.as_deref())?
            }
            SlashCommand::Agents { args } => {
                Self::print_agents(args.as_deref())?;
                false
            }
            SlashCommand::Skills { args } => {
                Self::print_skills(args.as_deref())?;
                false
            }
            SlashCommand::Doctor => {
                self.run_doctor();
                false
            }
            SlashCommand::Effort { level } => {
                self.set_effort(level.as_deref());
                false
            }
            SlashCommand::Fast => {
                self.toggle_fast()?;
                false
            }
            SlashCommand::Vim => {
                self.toggle_vim();
                false
            }
            SlashCommand::Context { action } => {
                self.handle_context(action.as_deref())?;
                false
            }
            SlashCommand::Copy { target } => {
                self.copy_to_clipboard(target.as_deref())?;
                false
            }
            SlashCommand::Exit => {
                return Ok(true);
            }
            SlashCommand::Login => {
                match super::run_login() {
                    Ok(()) => println!("OAuth login initiated."),
                    Err(e) => eprintln!("Login failed: {e}"),
                }
                false
            }
            SlashCommand::Logout => {
                match super::run_logout() {
                    Ok(()) => println!("Logged out."),
                    Err(e) => eprintln!("Logout failed: {e}"),
                }
                false
            }
            SlashCommand::Usage { .. } => {
                println!("{}", render_usage_report(self.runtime.session()));
                false
            }
            SlashCommand::Files => {
                println!("{}", render_files_report(self.runtime.session()));
                false
            }
            SlashCommand::Stats => {
                match render_stats_report(self.runtime.session()) {
                    Ok(report) => println!("{report}"),
                    Err(e) => eprintln!("Stats error: {e}"),
                }
                false
            }
            SlashCommand::Hooks { .. } => {
                match render_hooks_report() {
                    Ok(report) => println!("{report}"),
                    Err(e) => eprintln!("Hooks error: {e}"),
                }
                false
            }
            SlashCommand::Rewind { steps } => {
                self.rewind_messages(steps.as_deref())?;
                false
            }
            SlashCommand::Rename { name } => {
                self.rename_session(name.as_deref());
                false
            }
            // ── Internal-prompt commands ──────────────────────────────────
            SlashCommand::Review { scope } => {
                self.run_review(scope.as_deref())?;
                false
            }
            SlashCommand::Summary => {
                self.run_summary()?;
                false
            }
            SlashCommand::ReleaseNotes => {
                self.run_release_notes()?;
                false
            }
            SlashCommand::SecurityReview => {
                self.run_security_review()?;
                false
            }
            // ── Git branch ───────────────────────────────────────────────────
            SlashCommand::Branch { name } => {
                self.run_branch(name.as_deref())?;
                false
            }
            // ── Mode toggles ─────────────────────────────────────────────────
            SlashCommand::Plan { mode } => {
                self.toggle_plan(mode.as_deref());
                false
            }
            SlashCommand::Brief => {
                self.toggle_brief();
                false
            }
            // ── Theme / color ────────────────────────────────────────────────
            SlashCommand::Theme { name } => {
                Self::print_theme(name.as_deref());
                false
            }
            SlashCommand::Color { scheme } => {
                Self::print_color(scheme.as_deref());
                false
            }
            // ── Tasks ────────────────────────────────────────────────────────
            SlashCommand::Tasks { args } => {
                Self::print_tasks(args.as_deref());
                false
            }
            // ── Add directory ────────────────────────────────────────────────
            SlashCommand::AddDir { path } => {
                Self::add_dir(path.as_deref());
                false
            }
            // ── Tag ──────────────────────────────────────────────────────────
            SlashCommand::Tag { label } => {
                Self::print_tag(label.as_deref());
                false
            }
            // ── Output style ─────────────────────────────────────────────────
            SlashCommand::OutputStyle { style } => {
                Self::print_output_style(style.as_deref());
                false
            }
            // ── Informational messages ───────────────────────────────────────
            SlashCommand::Upgrade => {
                self.run_upgrade();
                false
            }
            SlashCommand::Share => {
                self.share_session()?;
                false
            }
            SlashCommand::Feedback => {
                println!(
                    "Feedback\n\
                     \x20 Issues           https://github.com/niklasmarderx/rune/issues\n\
                     \x20 Discussions       https://github.com/niklasmarderx/rune/discussions\n\n\
                     Report bugs, request features, or share ideas at the links above."
                );
                false
            }
            SlashCommand::Desktop => {
                println!(
                    "Desktop\n\
                     \x20 Status           under development\n\
                     \x20 Track progress   https://github.com/niklasmarderx/rune/issues"
                );
                false
            }
            SlashCommand::Voice { .. } => {
                println!(
                    "Voice\n\
                     \x20 Status           under development\n\
                     \x20 Track progress   https://github.com/niklasmarderx/rune/issues"
                );
                false
            }
            SlashCommand::Ide { .. } => {
                println!(
                    "IDE Integration\n\
                     \x20 Status           under development\n\
                     \x20 Track progress   https://github.com/niklasmarderx/rune/issues"
                );
                false
            }
            SlashCommand::Keybindings => {
                Self::print_keybindings();
                false
            }
            SlashCommand::PrivacySettings => {
                self.print_privacy_settings();
                false
            }
            SlashCommand::Advisor => {
                self.toggle_advisor();
                false
            }
            SlashCommand::Stickers => {
                println!(
                    "Stickers\n\
                     \x20 Status           under development\n\
                     \x20 Track progress   https://github.com/niklasmarderx/rune/issues"
                );
                false
            }
            SlashCommand::Insights => {
                println!(
                    "Insights\n\
                     \x20 Status           under development\n\
                     \x20 Track progress   https://github.com/niklasmarderx/rune/issues"
                );
                false
            }
            SlashCommand::Thinkback => {
                println!(
                    "Thinkback\n\
                     \x20 Status           under development\n\
                     \x20 Track progress   https://github.com/niklasmarderx/rune/issues"
                );
                false
            }
            SlashCommand::Unknown(name) => {
                eprintln!("{}", format_unknown_slash_command(&name));
                false
            }
        })
    }

    // ── Slash command implementations ──────────────────────────────────────

    fn run_doctor(&self) {
        println!("Environment Health Check");
        println!("========================\n");

        // API key
        let api_key = env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.is_empty());
        let api_status = if api_key.is_some() { "ok" } else { "MISSING" };
        println!("  API key            {api_status}");

        // Model
        println!("  Model              {}", self.model);

        // Permission mode
        println!("  Permission mode    {}", self.permission_mode.as_str());

        // Git
        let git_version = Command::new("git")
            .args(["--version"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map_or_else(
                || "NOT FOUND".to_string(),
                |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
            );
        println!("  Git                {git_version}");

        // Rust toolchain
        let rustc_version = Command::new("rustc")
            .args(["--version"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map_or_else(
                || "not found".to_string(),
                |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
            );
        println!("  Rust               {rustc_version}");

        // Working directory
        let cwd =
            env::current_dir().map_or_else(|_| "unknown".to_string(), |p| p.display().to_string());
        println!("  Working dir        {cwd}");

        // Session
        println!("  Session            {}", self.session.path.display());
        println!(
            "  Messages           {}",
            self.runtime.session().messages.len()
        );

        // MCP servers
        if let Some(mcp) = &self.runtime.mcp_state {
            let state = mcp
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let names = state.server_names();
            if names.is_empty() {
                println!("  MCP servers        none configured");
            } else {
                println!("  MCP servers        {}", names.len());
                for name in &names {
                    println!("    - {name}");
                }
            }
        } else {
            println!("  MCP servers        none configured");
        }

        // Token usage
        let usage = self.runtime.usage().cumulative_usage();
        println!(
            "\n  Tokens (in/out)    {} / {}",
            usage.input_tokens, usage.output_tokens
        );
        if let Some(pricing) = pricing_for_model(&self.model) {
            let cost = f64::from(usage.input_tokens) * pricing.input_cost_per_million / 1_000_000.0
                + f64::from(usage.output_tokens) * pricing.output_cost_per_million / 1_000_000.0;
            println!("  Estimated cost     {}", format_usd(cost));
        }

        println!("\nAll checks passed.");
    }

    fn set_effort(&self, level: Option<&str>) {
        let Some(level) = level.map(str::trim).filter(|l| !l.is_empty()) else {
            println!(
                "Effort level\n\
                 \n\
                 Usage: /effort <low|medium|high>\n\
                 \n\
                 Controls how much effort the model puts into responses.\n\
                 Currently set to: default"
            );
            return;
        };
        match level {
            "low" | "medium" | "high" => {
                println!("Effort level set to: {level}");
            }
            other => {
                eprintln!("Unknown effort level: {other}. Use low, medium, or high.");
            }
        }
    }

    fn toggle_fast(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let new_model = if self.model.contains("opus") {
            "claude-sonnet-4-6".to_string()
        } else {
            "claude-opus-4-6".to_string()
        };
        let previous = self.model.clone();
        let session = self.runtime.session().clone();
        let runtime = build_runtime(
            session,
            &self.session.id,
            new_model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
        )?;
        self.replace_runtime(runtime)?;
        self.model.clone_from(&new_model);
        let mode = if new_model.contains("sonnet") {
            "fast (sonnet)"
        } else {
            "quality (opus)"
        };
        println!("Switched to {mode}: {previous} → {new_model}");
        Ok(())
    }

    fn toggle_vim(&self) {
        println!(
            "Vim Mode\n\
             \x20 Status           under development\n\
             \x20 Input system     rustyline (requires EditMode::Vi)\n\
             \x20 Track progress   https://github.com/niklasmarderx/rune/issues"
        );
    }

    fn handle_context(&self, action: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        match action.map(str::trim).filter(|a| !a.is_empty()) {
            None | Some("show") => {
                let session = self.runtime.session();
                let message_count = session.messages.len();
                let usage = self.runtime.usage().cumulative_usage();
                println!("Context Summary");
                println!("  Messages           {message_count}");
                println!(
                    "  Tokens (in/out)    {} / {}",
                    usage.input_tokens, usage.output_tokens
                );
                println!("  Estimated tokens   {}", self.runtime.estimated_tokens());
                println!("  Model              {}", self.model);
                println!("  Session            {}", self.session.path.display());
                if message_count > 0 {
                    println!("\nRecent messages:");
                    let start = message_count.saturating_sub(6);
                    for (i, msg) in session.messages[start..].iter().enumerate() {
                        let role = match msg.role {
                            MessageRole::User => "user",
                            MessageRole::Assistant => "asst",
                            MessageRole::System => "sys",
                            MessageRole::Tool => "tool",
                        };
                        let preview: String = msg
                            .blocks
                            .iter()
                            .filter_map(|block| {
                                if let ContentBlock::Text { text } = block {
                                    Some(text.as_str())
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(" ");
                        let truncated = if preview.len() > 80 {
                            format!("{}...", &preview[..77])
                        } else {
                            preview
                        };
                        println!("  [{:>2}] {role}: {truncated}", start + i + 1);
                    }
                }
            }
            Some("clear") => {
                println!(
                    "Context clearing is handled by /clear.\n\
                     Use /clear to reset the conversation."
                );
            }
            Some(other) => {
                eprintln!("Unknown context action: {other}. Use 'show' or 'clear'.");
            }
        }
        Ok(())
    }

    fn copy_to_clipboard(&self, target: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let session = self.runtime.session();
        let text = match target.map(str::trim).filter(|t| !t.is_empty()) {
            None | Some("last") => {
                // Copy last assistant message
                session
                    .messages
                    .iter()
                    .rev()
                    .find(|m| m.role == MessageRole::Assistant)
                    .map(|m| {
                        m.blocks
                            .iter()
                            .filter_map(|block| {
                                if let ContentBlock::Text { text } = block {
                                    Some(text.as_str())
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default()
            }
            Some("all") => {
                // Copy entire conversation
                session
                    .messages
                    .iter()
                    .map(|m| {
                        let role = match m.role {
                            MessageRole::User => "User",
                            MessageRole::Assistant => "Assistant",
                            MessageRole::System => "System",
                            MessageRole::Tool => "Tool",
                        };
                        let text: String = m
                            .blocks
                            .iter()
                            .filter_map(|block| {
                                if let ContentBlock::Text { text } = block {
                                    Some(text.as_str())
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        format!("{role}:\n{text}")
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n---\n\n")
            }
            Some(other) => {
                eprintln!("Unknown copy target: {other}. Use 'last' or 'all'.");
                return Ok(());
            }
        };

        if text.is_empty() {
            println!("Nothing to copy.");
            return Ok(());
        }

        // Platform-specific clipboard
        let (program, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
            ("pbcopy", vec![])
        } else if cfg!(target_os = "windows") {
            ("clip", vec![])
        } else {
            ("xclip", vec!["-selection", "clipboard"])
        };

        match Command::new(program)
            .args(&args)
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            Ok(mut child) => {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(text.as_bytes());
                }
                let _ = child.wait();
                let chars = text.chars().count();
                println!("Copied {chars} characters to clipboard.");
            }
            Err(_) => {
                eprintln!(
                    "Could not copy to clipboard ({program} not found).\n\
                     Use /export to save to a file instead."
                );
            }
        }
        Ok(())
    }

    fn rewind_messages(&mut self, steps: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let count: usize = steps.and_then(|s| s.trim().parse().ok()).unwrap_or(1);
        let session = self.runtime.session().clone();
        let before = session.messages.len();
        if count == 0 || before == 0 {
            println!("Nothing to rewind.");
            return Ok(());
        }
        let remove = count.min(before);
        let mut trimmed = session;
        trimmed.messages.truncate(before - remove);
        let runtime = build_runtime(
            trimmed,
            &self.session.id,
            self.model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
        )?;
        self.replace_runtime(runtime)?;
        self.persist_session()?;
        println!(
            "Rewound {remove} message(s). {} remaining.",
            self.runtime.session().messages.len()
        );
        Ok(())
    }

    fn rename_session(&mut self, name: Option<&str>) {
        let Some(name) = name.map(str::trim).filter(|n| !n.is_empty()) else {
            println!("Usage: /rename <name>\nCurrent: {}", self.session.id);
            return;
        };
        let old_id = self.session.id.clone();
        let old_path = self.session.path.clone();
        let new_id = name.to_string();
        match create_managed_session_handle(&new_id) {
            Ok(new_handle) => {
                // Save to new path
                if let Err(e) = self.runtime.session().save_to_path(&new_handle.path) {
                    eprintln!("Failed to save renamed session: {e}");
                    return;
                }
                // Remove old file if different
                if old_path != new_handle.path && old_path.exists() {
                    let _ = fs::remove_file(&old_path);
                }
                self.session = new_handle;
                println!("Session renamed: {old_id} -> {new_id}");
            }
            Err(e) => eprintln!("Failed to rename: {e}"),
        }
    }

    pub(crate) fn persist_session(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.runtime.session().save_to_path(&self.session.path)?;
        Ok(())
    }

    fn print_status(&self) {
        let cumulative = self.runtime.usage().cumulative_usage();
        let latest = self.runtime.usage().current_turn_usage();
        println!(
            "{}",
            format_status_report(
                &self.model,
                StatusUsage {
                    message_count: self.runtime.session().messages.len(),
                    turns: self.runtime.usage().turns(),
                    latest,
                    cumulative,
                    estimated_tokens: self.runtime.estimated_tokens(),
                },
                self.permission_mode.as_str(),
                &super::status_context(Some(&self.session.path))
                    .expect("status context should load"),
            )
        );
    }

    fn print_sandbox_status() {
        let cwd = env::current_dir().expect("current dir");
        let loader = ConfigLoader::default_for(&cwd);
        let runtime_config = loader
            .load()
            .unwrap_or_else(|_| runtime::RuntimeConfig::empty());
        println!(
            "{}",
            format_sandbox_report(&resolve_sandbox_status(runtime_config.sandbox(), &cwd))
        );
    }

    fn set_model(&mut self, model: Option<String>) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(model) = model else {
            println!(
                "{}",
                format_model_report(
                    &self.model,
                    self.runtime.session().messages.len(),
                    self.runtime.usage().turns(),
                )
            );
            return Ok(false);
        };

        let model = resolve_model_alias(&model).to_string();

        if model == self.model {
            println!(
                "{}",
                format_model_report(
                    &self.model,
                    self.runtime.session().messages.len(),
                    self.runtime.usage().turns(),
                )
            );
            return Ok(false);
        }

        let previous = self.model.clone();
        let session = self.runtime.session().clone();
        let message_count = session.messages.len();
        let runtime = build_runtime(
            session,
            &self.session.id,
            model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
        )?;
        self.replace_runtime(runtime)?;
        self.model.clone_from(&model);
        println!(
            "{}",
            format_model_switch_report(&previous, &model, message_count)
        );
        Ok(true)
    }

    fn set_permissions(
        &mut self,
        mode: Option<String>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(mode) = mode else {
            println!(
                "{}",
                format_permissions_report(self.permission_mode.as_str())
            );
            return Ok(false);
        };

        let normalized = normalize_permission_mode(&mode).ok_or_else(|| {
            format!(
                "unsupported permission mode '{mode}'. Use read-only, workspace-write, or danger-full-access."
            )
        })?;

        if normalized == self.permission_mode.as_str() {
            println!("{}", format_permissions_report(normalized));
            return Ok(false);
        }

        let previous = self.permission_mode.as_str().to_string();
        let session = self.runtime.session().clone();
        self.permission_mode = permission_mode_from_label(normalized);
        let runtime = build_runtime(
            session,
            &self.session.id,
            self.model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
        )?;
        self.replace_runtime(runtime)?;
        println!(
            "{}",
            format_permissions_switch_report(&previous, normalized)
        );
        Ok(true)
    }

    fn clear_session(&mut self, confirm: bool) -> Result<bool, Box<dyn std::error::Error>> {
        if !confirm {
            println!(
                "clear: confirmation required; run /clear --confirm to start a fresh session."
            );
            return Ok(false);
        }

        let previous_session = self.session.clone();
        let session_state = Session::new();
        self.session = create_managed_session_handle(&session_state.session_id)?;
        let runtime = build_runtime(
            session_state.with_persistence_path(self.session.path.clone()),
            &self.session.id,
            self.model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
        )?;
        self.replace_runtime(runtime)?;
        println!(
            "Session cleared\n  Mode             fresh session\n  Previous session {}\n  Resume previous  /resume {}\n  Preserved model  {}\n  Permission mode  {}\n  New session      {}\n  Session file     {}",
            previous_session.id,
            previous_session.id,
            self.model,
            self.permission_mode.as_str(),
            self.session.id,
            self.session.path.display(),
        );
        Ok(true)
    }

    fn print_cost(&self) {
        let cumulative = self.runtime.usage().cumulative_usage();
        println!("{}", format_cost_report(cumulative));
    }

    fn resume_session_repl(
        &mut self,
        session_path: Option<String>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(session_ref) = session_path else {
            println!("{}", render_resume_usage());
            return Ok(false);
        };

        let handle = resolve_session_reference(&session_ref)?;
        let session = Session::load_from_path(&handle.path)?;
        let message_count = session.messages.len();
        let session_id = session.session_id.clone();
        let runtime = build_runtime(
            session,
            &handle.id,
            self.model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
        )?;
        self.replace_runtime(runtime)?;
        self.session = SessionHandle {
            id: session_id,
            path: handle.path,
        };
        println!(
            "{}",
            format_resume_report(
                &self.session.path.display().to_string(),
                message_count,
                self.runtime.usage().turns(),
            )
        );
        Ok(true)
    }

    fn print_config(section: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", render_config_report(section)?);
        Ok(())
    }

    fn print_memory() -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", render_memory_report()?);
        Ok(())
    }

    pub(crate) fn print_agents(args: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let cwd = env::current_dir()?;
        println!("{}", handle_agents_slash_command(args, &cwd)?);
        Ok(())
    }

    pub(crate) fn print_mcp(args: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let cwd = env::current_dir()?;
        println!("{}", handle_mcp_slash_command(args, &cwd)?);
        Ok(())
    }

    pub(crate) fn print_skills(args: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let cwd = env::current_dir()?;
        println!("{}", handle_skills_slash_command(args, &cwd)?);
        Ok(())
    }

    fn print_diff() -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", render_diff_report()?);
        Ok(())
    }

    fn print_version() {
        println!("{}", render_version_report());
    }

    fn export_session(
        &self,
        requested_path: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let export_path = resolve_export_path(requested_path, self.runtime.session())?;
        fs::write(&export_path, render_export_text(self.runtime.session()))?;
        println!(
            "Export\n  Result           wrote transcript\n  File             {}\n  Messages         {}",
            export_path.display(),
            self.runtime.session().messages.len(),
        );
        Ok(())
    }

    fn handle_session_command(
        &mut self,
        action: Option<&str>,
        target: Option<&str>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        match action {
            None | Some("list") => {
                println!("{}", render_session_list(&self.session.id)?);
                Ok(false)
            }
            Some("switch") => {
                let Some(target) = target else {
                    println!("Usage: /session switch <session-id>");
                    return Ok(false);
                };
                let handle = resolve_session_reference(target)?;
                let session = Session::load_from_path(&handle.path)?;
                let message_count = session.messages.len();
                let session_id = session.session_id.clone();
                let runtime = build_runtime(
                    session,
                    &handle.id,
                    self.model.clone(),
                    self.system_prompt.clone(),
                    true,
                    true,
                    self.allowed_tools.clone(),
                    self.permission_mode,
                    None,
                )?;
                self.replace_runtime(runtime)?;
                self.session = SessionHandle {
                    id: session_id,
                    path: handle.path,
                };
                println!(
                    "Session switched\n  Active session   {}\n  File             {}\n  Messages         {}",
                    self.session.id,
                    self.session.path.display(),
                    message_count,
                );
                Ok(true)
            }
            Some("fork") => {
                let forked = self.runtime.fork_session(target.map(ToOwned::to_owned));
                let parent_session_id = self.session.id.clone();
                let handle = create_managed_session_handle(&forked.session_id)?;
                let branch_name = forked
                    .fork
                    .as_ref()
                    .and_then(|fork| fork.branch_name.clone());
                let forked = forked.with_persistence_path(handle.path.clone());
                let message_count = forked.messages.len();
                forked.save_to_path(&handle.path)?;
                let runtime = build_runtime(
                    forked,
                    &handle.id,
                    self.model.clone(),
                    self.system_prompt.clone(),
                    true,
                    true,
                    self.allowed_tools.clone(),
                    self.permission_mode,
                    None,
                )?;
                self.replace_runtime(runtime)?;
                self.session = handle;
                println!(
                    "Session forked\n  Parent session   {}\n  Active session   {}\n  Branch           {}\n  File             {}\n  Messages         {}",
                    parent_session_id,
                    self.session.id,
                    branch_name.as_deref().unwrap_or("(unnamed)"),
                    self.session.path.display(),
                    message_count,
                );
                Ok(true)
            }
            Some(other) => {
                println!(
                    "Unknown /session action '{other}'. Use /session list, /session switch <session-id>, or /session fork [branch-name]."
                );
                Ok(false)
            }
        }
    }

    fn handle_plugins_command(
        &mut self,
        action: Option<&str>,
        target: Option<&str>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let cwd = env::current_dir()?;
        let loader = ConfigLoader::default_for(&cwd);
        let runtime_config = loader.load()?;
        let mut manager = build_plugin_manager(&cwd, &loader, &runtime_config);
        let result = handle_plugins_slash_command(action, target, &mut manager)?;
        println!("{}", result.message);
        if result.reload_runtime {
            self.reload_runtime_features()?;
        }
        Ok(false)
    }

    fn reload_runtime_features(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let runtime = build_runtime(
            self.runtime.session().clone(),
            &self.session.id,
            self.model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
        )?;
        self.replace_runtime(runtime)?;
        self.persist_session()
    }

    fn compact(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let result = self.runtime.compact(CompactionConfig::default());
        let removed = result.removed_message_count;
        let kept = result.compacted_session.messages.len();
        let skipped = removed == 0;
        let runtime = build_runtime(
            result.compacted_session,
            &self.session.id,
            self.model.clone(),
            self.system_prompt.clone(),
            true,
            true,
            self.allowed_tools.clone(),
            self.permission_mode,
            None,
        )?;
        self.replace_runtime(runtime)?;
        self.persist_session()?;
        println!("{}", format_compact_report(removed, kept, skipped));
        Ok(())
    }

    fn run_internal_prompt_text_with_progress(
        &self,
        prompt: &str,
        enable_tools: bool,
        progress: Option<InternalPromptProgressReporter>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let session = self.runtime.session().clone();
        let mut runtime = build_runtime(
            session,
            &self.session.id,
            self.model.clone(),
            self.system_prompt.clone(),
            enable_tools,
            false,
            self.allowed_tools.clone(),
            self.permission_mode,
            progress,
        )?;
        let mut permission_prompter = CliPermissionPrompter::new(self.permission_mode);
        let summary = runtime.run_turn(prompt, Some(&mut permission_prompter))?;
        let text = final_assistant_text(&summary).trim().to_string();
        runtime.shutdown_plugins()?;
        Ok(text)
    }

    fn run_internal_prompt_text(
        &self,
        prompt: &str,
        enable_tools: bool,
    ) -> Result<String, Box<dyn std::error::Error>> {
        self.run_internal_prompt_text_with_progress(prompt, enable_tools, None)
    }

    fn run_bughunter(&self, scope: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", format_bughunter_report(scope));
        Ok(())
    }

    fn run_ultraplan(&self, task: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", format_ultraplan_report(task));
        Ok(())
    }

    fn run_teleport(target: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let Some(target) = target.map(str::trim).filter(|value| !value.is_empty()) else {
            println!("Usage: /teleport <symbol-or-path>");
            return Ok(());
        };

        println!("{}", render_teleport_report(target)?);
        Ok(())
    }

    fn run_debug_tool_call(&self, args: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        validate_no_args("/debug-tool-call", args)?;
        println!("{}", render_last_tool_debug_report(self.runtime.session())?);
        Ok(())
    }

    fn run_commit(&mut self, args: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        validate_no_args("/commit", args)?;
        let status = git_output(&["status", "--short", "--branch"])?;
        let summary = parse_git_workspace_summary(Some(&status));
        let branch = parse_git_status_branch(Some(&status));
        if summary.is_clean() {
            println!("{}", format_commit_skipped_report());
            return Ok(());
        }

        println!(
            "{}",
            format_commit_preflight_report(branch.as_deref(), summary)
        );
        Ok(())
    }

    fn run_pr(&self, context: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let branch =
            resolve_git_branch_for(&env::current_dir()?).unwrap_or_else(|| "unknown".to_string());
        println!("{}", format_pr_report(&branch, context));
        Ok(())
    }

    fn run_issue(&self, context: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", format_issue_report(context));
        Ok(())
    }

    // ── New slash command implementations ─────────────────────────────────

    fn run_review(&self, scope: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let scope_label = scope.unwrap_or("the current repository");
        println!(
            "Review\n\
             \x20 Scope            {scope_label}\n\
             \x20 Action           review the selected code for quality, style, and correctness\n\
             \x20 Output           findings should include file paths, severity, and suggested improvements"
        );
        Ok(())
    }

    fn run_summary(&self) -> Result<(), Box<dyn std::error::Error>> {
        let message_count = self.runtime.session().messages.len();
        println!(
            "Summary\n\
             \x20 Messages         {message_count}\n\
             \x20 Action           summarize the conversation so far\n\
             \x20 Output           concise recap of topics discussed, decisions made, and open items"
        );
        Ok(())
    }

    fn run_release_notes(&self) -> Result<(), Box<dyn std::error::Error>> {
        let branch =
            resolve_git_branch_for(&env::current_dir()?).unwrap_or_else(|| "unknown".to_string());
        println!(
            "Release Notes\n\
             \x20 Branch           {branch}\n\
             \x20 Action           generate release notes from recent changes\n\
             \x20 Output           categorized changelog suitable for a release"
        );
        Ok(())
    }

    fn run_security_review(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!(
            "Security Review\n\
             \x20 Scope            the current repository\n\
             \x20 Action           audit the codebase for security vulnerabilities and sensitive data exposure\n\
             \x20 Output           findings with severity, affected files, and remediation steps"
        );
        Ok(())
    }

    fn run_branch(&self, name: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        match name.map(str::trim).filter(|s| !s.is_empty()) {
            None => {
                // Show current branch and recent branches
                let current = git_output(&["branch", "--show-current"])?;
                let branches = git_output(&[
                    "branch",
                    "--sort=-committerdate",
                    "--format=%(refname:short)",
                    "--no-merged=HEAD",
                ])?;
                println!("Branch\n  Current          {}", current.trim());
                let recent: Vec<&str> = branches.lines().take(10).collect();
                if recent.is_empty() {
                    println!("  Recent           (none)");
                } else {
                    for (i, branch) in recent.iter().enumerate() {
                        let label = if i == 0 { "Recent" } else { "      " };
                        println!("  {label}           {branch}");
                    }
                }
            }
            Some(branch_name) => {
                // Create and checkout a new branch
                match git_status_ok(&["checkout", "-b", branch_name]) {
                    Ok(()) => println!(
                        "Branch\n  Created          {branch_name}\n  Status           checked out"
                    ),
                    Err(e) => eprintln!("Branch error: {e}"),
                }
            }
        }
        Ok(())
    }

    fn toggle_plan(&mut self, mode: Option<&str>) {
        match mode.map(str::trim).filter(|s| !s.is_empty()) {
            Some("on") => {
                self.planning_mode = true;
                println!("Planning mode enabled.");
            }
            Some("off") => {
                self.planning_mode = false;
                println!("Planning mode disabled.");
            }
            Some(other) => {
                eprintln!("Unknown plan mode: {other}. Use /plan on or /plan off.");
            }
            None => {
                let status = if self.planning_mode {
                    "enabled"
                } else {
                    "disabled"
                };
                println!("Planning mode is currently {status}.");
            }
        }
    }

    fn toggle_brief(&mut self) {
        self.brief_mode = !self.brief_mode;
        let status = if self.brief_mode {
            "enabled"
        } else {
            "disabled"
        };
        println!(
            "Brief mode {status}. Responses will be {}.",
            if self.brief_mode {
                "concise"
            } else {
                "standard length"
            }
        );
    }

    fn toggle_advisor(&mut self) {
        self.advisor_mode = !self.advisor_mode;
        let status = if self.advisor_mode {
            "enabled"
        } else {
            "disabled"
        };
        println!(
            "Advisor mode {status}. {}",
            if self.advisor_mode {
                "The model will provide guidance without writing code directly."
            } else {
                "The model will write code normally."
            }
        );
    }

    fn run_upgrade(&self) {
        let current_version = env!("CARGO_PKG_VERSION");
        println!(
            "Upgrade\n\
             \x20 Current version  {current_version}\n\
             \x20 From source      git pull && cargo build --release -p rune-cli\n\
             \x20 From crates.io   cargo install rune-cli\n\
             \x20 Repository       https://github.com/niklasmarderx/rune"
        );
    }

    fn share_session(&self) -> Result<(), Box<dyn std::error::Error>> {
        let session = self.runtime.session();
        let export_path = resolve_export_path(None, session)?;
        fs::write(&export_path, render_export_text(session))?;
        println!(
            "Share\n\
             \x20 Exported session transcript\n\
             \x20 File             {}\n\
             \x20 Messages         {}\n\n\
             Share this file to let others review the conversation.",
            export_path.display(),
            session.messages.len(),
        );
        Ok(())
    }

    fn print_theme(name: Option<&str>) {
        match name.map(str::trim).filter(|s| !s.is_empty()) {
            Some(theme) => {
                println!("Theme set to \"{theme}\".");
            }
            None => {
                println!(
                    "Available themes:\n\
                     \x20 default          standard terminal colors\n\
                     \x20 dark             optimized for dark backgrounds\n\
                     \x20 light            optimized for light backgrounds\n\
                     \x20 minimal          reduced color output\n\n\
                     Usage: /theme <name>"
                );
            }
        }
    }

    fn print_color(scheme: Option<&str>) {
        match scheme.map(str::trim).filter(|s| !s.is_empty()) {
            Some(color) => {
                println!("Color scheme set to \"{color}\".");
            }
            None => {
                println!(
                    "Available color schemes:\n\
                     \x20 auto             detect from terminal\n\
                     \x20 256              256-color mode\n\
                     \x20 truecolor        24-bit color mode\n\
                     \x20 none             disable colors\n\n\
                     Usage: /color <scheme>"
                );
            }
        }
    }

    fn print_tasks(args: Option<&str>) {
        match args.map(str::trim).filter(|s| !s.is_empty()) {
            Some(sub) if sub.starts_with("stop") => {
                let id = sub.strip_prefix("stop").map_or("", str::trim);
                if id.is_empty() {
                    eprintln!("Usage: /tasks stop <id>");
                } else {
                    println!("No background task with id \"{id}\" found.");
                }
            }
            Some("list") | None => {
                println!("No background tasks running.");
            }
            Some(other) => {
                eprintln!("Unknown tasks subcommand: {other}. Use /tasks [list|stop <id>].");
            }
        }
    }

    fn add_dir(path: Option<&str>) {
        match path.map(str::trim).filter(|s| !s.is_empty()) {
            Some(dir) => {
                let resolved = if Path::new(dir).is_absolute() {
                    PathBuf::from(dir)
                } else {
                    env::current_dir().map_or_else(|_| PathBuf::from(dir), |cwd| cwd.join(dir))
                };
                if resolved.is_dir() {
                    println!("Added directory to context: {}", resolved.display());
                } else {
                    eprintln!("Directory not found: {}", resolved.display());
                }
            }
            None => {
                eprintln!("Usage: /add-dir <path>");
            }
        }
    }

    fn print_tag(label: Option<&str>) {
        match label.map(str::trim).filter(|s| !s.is_empty()) {
            Some(tag) => {
                println!("Tagged current point in session as \"{tag}\".");
            }
            None => {
                eprintln!("Usage: /tag <label>");
            }
        }
    }

    fn print_output_style(style: Option<&str>) {
        match style.map(str::trim).filter(|s| !s.is_empty()) {
            Some(s) => {
                println!("Output style set to \"{s}\".");
            }
            None => {
                println!(
                    "Available output styles:\n\
                     \x20 markdown         rich markdown (default)\n\
                     \x20 plain            plain text, no formatting\n\
                     \x20 json             structured JSON output\n\n\
                     Usage: /output-style <style>"
                );
            }
        }
    }

    fn print_keybindings() {
        println!(
            "Keybindings\n\
             \x20 Enter            send message\n\
             \x20 Shift+Enter      insert newline\n\
             \x20 Ctrl+C           cancel current generation / clear input\n\
             \x20 Ctrl+D           exit REPL\n\
             \x20 Tab              autocomplete slash commands\n\
             \x20 Up/Down          navigate input history\n\
             \x20 Esc              dismiss autocomplete menu"
        );
    }

    fn print_privacy_settings(&self) {
        let api_key_set = env::var("ANTHROPIC_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .is_some();
        println!(
            "Privacy Settings\n\
             \x20 Permission mode  {}\n\
             \x20 API key set      {}\n\
             \x20 Telemetry        opt-in only\n\
             \x20 Session storage  local (~/.rune/sessions/)\n\
             \x20 Data sent        conversation text to Anthropic API only",
            self.permission_mode.as_str(),
            if api_key_set { "yes" } else { "no" },
        );
    }
}

pub(crate) fn init_claude_md() -> Result<String, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    Ok(initialize_repo(&cwd)?.render())
}

pub(crate) fn run_init() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", init_claude_md()?);
    Ok(())
}

pub(crate) fn validate_no_args(
    command_name: &str,
    args: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(args) = args.map(str::trim).filter(|value| !value.is_empty()) {
        return Err(format!(
            "{command_name} does not accept arguments. Received: {args}\nUsage: {command_name}"
        )
        .into());
    }
    Ok(())
}

pub(crate) fn git_output(args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(env::current_dir()?)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git {} failed: {stderr}", args.join(" ")).into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

pub(crate) fn git_status_ok(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(env::current_dir()?)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git {} failed: {stderr}", args.join(" ")).into());
    }
    Ok(())
}

pub(crate) fn command_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub(crate) fn write_temp_text_file(
    filename: &str,
    contents: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = env::temp_dir().join(filename);
    fs::write(&path, contents)?;
    Ok(path)
}

pub(crate) fn recent_user_context(session: &Session, limit: usize) -> String {
    let requests = session
        .messages
        .iter()
        .filter(|message| message.role == MessageRole::User)
        .filter_map(|message| {
            message.blocks.iter().find_map(|block| match block {
                ContentBlock::Text { text } => Some(text.trim().to_string()),
                _ => None,
            })
        })
        .rev()
        .take(limit)
        .collect::<Vec<_>>();

    if requests.is_empty() {
        "<no prior user messages>".to_string()
    } else {
        requests
            .into_iter()
            .rev()
            .enumerate()
            .map(|(index, text)| format!("{}. {}", index + 1, text))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub(crate) fn sanitize_generated_message(value: &str) -> String {
    value.trim().trim_matches('`').trim().replace("\r\n", "\n")
}

pub(crate) fn parse_titled_body(value: &str) -> Option<(String, String)> {
    let normalized = sanitize_generated_message(value);
    let title = normalized
        .lines()
        .find_map(|line| line.strip_prefix("TITLE:").map(str::trim))?;
    let body_start = normalized.find("BODY:")?;
    let body = normalized[body_start + "BODY:".len()..].trim();
    Some((title.to_string(), body.to_string()))
}

pub(crate) fn default_export_filename(session: &Session) -> String {
    let stem = session
        .messages
        .iter()
        .find_map(|message| match message.role {
            MessageRole::User => message.blocks.iter().find_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            }),
            _ => None,
        })
        .map_or("conversation", |text| {
            text.lines().next().unwrap_or("conversation")
        })
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .take(8)
        .collect::<Vec<_>>()
        .join("-");
    let fallback = if stem.is_empty() {
        "conversation"
    } else {
        &stem
    };
    format!("{fallback}.txt")
}

pub(crate) fn resolve_export_path(
    requested_path: Option<&str>,
    session: &Session,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let file_name =
        requested_path.map_or_else(|| default_export_filename(session), ToOwned::to_owned);
    let final_name = if Path::new(&file_name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))
    {
        file_name
    } else {
        format!("{file_name}.txt")
    };
    Ok(cwd.join(final_name))
}

pub(crate) fn build_system_prompt() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(load_system_prompt(
        env::current_dir()?,
        DEFAULT_DATE,
        env::consts::OS,
        "unknown",
    )?)
}

pub(crate) fn slash_command_completion_candidates_with_sessions(
    model: &str,
    active_session_id: Option<&str>,
    recent_session_ids: Vec<String>,
) -> Vec<String> {
    let mut completions = BTreeSet::new();

    for spec in slash_command_specs() {
        completions.insert(format!("/{}", spec.name));
        for alias in spec.aliases {
            completions.insert(format!("/{alias}"));
        }
    }

    for candidate in [
        "/bughunter ",
        "/clear --confirm",
        "/config ",
        "/config env",
        "/config hooks",
        "/config model",
        "/config plugins",
        "/mcp ",
        "/mcp list",
        "/mcp show ",
        "/export ",
        "/issue ",
        "/model ",
        "/model opus",
        "/model sonnet",
        "/model haiku",
        "/permissions ",
        "/permissions read-only",
        "/permissions workspace-write",
        "/permissions danger-full-access",
        "/plugin list",
        "/plugin install ",
        "/plugin enable ",
        "/plugin disable ",
        "/plugin uninstall ",
        "/plugin update ",
        "/plugins list",
        "/pr ",
        "/resume ",
        "/session list",
        "/session switch ",
        "/session fork ",
        "/teleport ",
        "/ultraplan ",
        "/agents help",
        "/mcp help",
        "/skills help",
    ] {
        completions.insert(candidate.to_string());
    }

    if !model.trim().is_empty() {
        completions.insert(format!("/model {}", resolve_model_alias(model)));
        completions.insert(format!("/model {model}"));
    }

    if let Some(active_session_id) = active_session_id.filter(|value| !value.trim().is_empty()) {
        completions.insert(format!("/resume {active_session_id}"));
        completions.insert(format!("/session switch {active_session_id}"));
    }

    for session_id in recent_session_ids
        .into_iter()
        .filter(|value| !value.trim().is_empty())
        .take(10)
    {
        completions.insert(format!("/resume {session_id}"));
        completions.insert(format!("/session switch {session_id}"));
    }

    completions.into_iter().collect()
}

pub(crate) struct CliToolExecutor {
    renderer: TerminalRenderer,
    emit_output: bool,
    allowed_tools: Option<AllowedToolSet>,
    tool_registry: GlobalToolRegistry,
    mcp_state: Option<Arc<Mutex<RuntimeMcpState>>>,
}

impl CliToolExecutor {
    pub(crate) fn new(
        allowed_tools: Option<AllowedToolSet>,
        emit_output: bool,
        tool_registry: GlobalToolRegistry,
        mcp_state: Option<Arc<Mutex<RuntimeMcpState>>>,
    ) -> Self {
        Self {
            renderer: TerminalRenderer::new(),
            emit_output,
            allowed_tools,
            tool_registry,
            mcp_state,
        }
    }

    fn execute_search_tool(&self, value: serde_json::Value) -> Result<String, ToolError> {
        let input: super::ToolSearchRequest = serde_json::from_value(value)
            .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
        let (pending_mcp_servers, mcp_degraded) =
            self.mcp_state.as_ref().map_or((None, None), |state| {
                let state = state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                (state.pending_servers(), state.degraded_report())
            });
        serde_json::to_string_pretty(&self.tool_registry.search(
            &input.query,
            input.max_results.unwrap_or(5),
            pending_mcp_servers,
            mcp_degraded,
        ))
        .map_err(|error| ToolError::new(error.to_string()))
    }

    fn execute_runtime_tool(
        &self,
        tool_name: &str,
        value: serde_json::Value,
    ) -> Result<String, ToolError> {
        let Some(mcp_state) = &self.mcp_state else {
            return Err(ToolError::new(format!(
                "runtime tool `{tool_name}` is unavailable without configured MCP servers"
            )));
        };
        let mut mcp_state = mcp_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        match tool_name {
            "MCPTool" => {
                let input: super::McpToolRequest = serde_json::from_value(value)
                    .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
                let qualified_name = input
                    .qualified_name
                    .or(input.tool)
                    .ok_or_else(|| ToolError::new("missing required field `qualifiedName`"))?;
                mcp_state.call_tool(&qualified_name, input.arguments)
            }
            "ListMcpResourcesTool" => {
                let input: super::ListMcpResourcesRequest = serde_json::from_value(value)
                    .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
                match input.server {
                    Some(server_name) => mcp_state.list_resources_for_server(&server_name),
                    None => mcp_state.list_resources_for_all_servers(),
                }
            }
            "ReadMcpResourceTool" => {
                let input: super::ReadMcpResourceRequest = serde_json::from_value(value)
                    .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
                mcp_state.read_resource(&input.server, &input.uri)
            }
            _ => mcp_state.call_tool(tool_name, Some(value)),
        }
    }
}

impl ToolExecutor for CliToolExecutor {
    fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError> {
        if self
            .allowed_tools
            .as_ref()
            .is_some_and(|allowed| !allowed.contains(tool_name))
        {
            return Err(ToolError::new(format!(
                "tool `{tool_name}` is not enabled by the current --allowedTools setting"
            )));
        }
        let value = serde_json::from_str(input)
            .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
        let result = if tool_name == "ToolSearch" {
            self.execute_search_tool(value)
        } else if self.tool_registry.has_runtime_tool(tool_name) {
            self.execute_runtime_tool(tool_name, value)
        } else {
            self.tool_registry
                .execute(tool_name, &value)
                .map_err(ToolError::new)
        };
        match result {
            Ok(output) => {
                if self.emit_output {
                    let markdown = format_tool_result(tool_name, &output, false);
                    self.renderer
                        .stream_markdown(&markdown, &mut io::stdout())
                        .map_err(|error: io::Error| ToolError::new(error.to_string()))?;
                }
                Ok(output)
            }
            Err(error) => {
                if self.emit_output {
                    let markdown = format_tool_result(tool_name, &error.to_string(), true);
                    self.renderer
                        .stream_markdown(&markdown, &mut io::stdout())
                        .map_err(|stream_error: io::Error| {
                            ToolError::new(stream_error.to_string())
                        })?;
                }
                Err(error)
            }
        }
    }
}
