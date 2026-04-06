# ᚱ Rune Code — Rust Implementation

A high-performance Rust rewrite of the Rune Code CLI agent harness. Built for speed, safety, and native tool execution.

For a task-oriented guide with copy/paste examples, see [`../USAGE.md`](../USAGE.md).

## Quick Start

```bash
# Inspect available commands
cd rust/
cargo run -p rusty-claude-cli -- --help

# Build the workspace
cargo build --workspace

# Run the interactive REPL
cargo run -p rusty-claude-cli -- --model claude-opus-4-6

# Launch the TUI (ratatui-based terminal UI)
cargo run -p rune-tui
# Or via the CLI flag:
cargo run -p rusty-claude-cli -- --tui

# One-shot prompt
cargo run -p rusty-claude-cli -- prompt "explain this codebase"

# JSON output for automation
cargo run -p rusty-claude-cli -- --output-format json prompt "summarize src/main.rs"
```

## Configuration

Set your API credentials:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
# Or use a proxy
export ANTHROPIC_BASE_URL="https://your-proxy.com"
```

Or authenticate via OAuth and let the CLI persist credentials locally:

```bash
cargo run -p rusty-claude-cli -- login
```

## Mock parity harness

The workspace includes a deterministic Anthropic-compatible mock service and a clean-environment CLI harness for end-to-end parity checks.

```bash
cd rust/

# Run the scripted clean-environment harness
./scripts/run_mock_parity_harness.sh

# Or start the mock service manually for ad hoc CLI runs
cargo run -p mock-anthropic-service -- --bind 127.0.0.1:0
```

Harness coverage:

- `streaming_text`
- `read_file_roundtrip`
- `grep_chunk_assembly`
- `write_file_allowed`
- `write_file_denied`
- `multi_tool_turn_roundtrip`
- `bash_stdout_roundtrip`
- `bash_permission_prompt_approved`
- `bash_permission_prompt_denied`
- `plugin_tool_roundtrip`

Primary artifacts:

- `crates/mock-anthropic-service/` — reusable mock Anthropic-compatible service
- `crates/rusty-claude-cli/tests/mock_parity_harness.rs` — clean-env CLI harness
- `scripts/run_mock_parity_harness.sh` — reproducible wrapper
- `scripts/run_mock_parity_diff.py` — scenario checklist + PARITY mapping runner
- `mock_parity_scenarios.json` — scenario-to-PARITY manifest

## Features

| Feature | Status |
|---------|--------|
| Anthropic API + streaming | ✅ |
| Multi-provider support (Anthropic, xAI/Grok, OpenAI-compatible, LiteLLM) | ✅ |
| OAuth login/logout | ✅ |
| Interactive REPL (rustyline) | ✅ |
| TUI frontend (ratatui) | ✅ |
| Tool system (bash, read, write, edit, grep, glob) | ✅ |
| Web tools (search, fetch) | ✅ |
| Sub-agent orchestration | ✅ |
| Todo tracking | ✅ |
| Notebook editing | ✅ |
| CLAUDE.md / project memory | ✅ |
| Config file hierarchy (.claude.json) | ✅ |
| Permission system | ✅ |
| MCP server lifecycle | ✅ |
| Session persistence + resume | ✅ |
| Extended thinking (thinking blocks) | ✅ |
| Cost tracking + usage display | ✅ |
| Git integration | ✅ |
| Markdown terminal rendering (ANSI) | ✅ |
| Model aliases (opus/sonnet/haiku) | ✅ |
| Slash commands (40+ commands) | ✅ |
| Hooks (PreToolUse/PostToolUse) | ✅ |
| Plugin system (registry + hook integration) | ✅ |
| Vim mode | ✅ |
| Advisor mode (read-only suggestions) | ✅ |
| Reasoning effort control | ✅ |
| Tab completion (commands, models, sessions) | ✅ |
| Skills registry | 📋 Planned |

## TUI Frontend

The `rune-tui` crate provides a full-featured terminal UI built with ratatui:

- **4-panel layout**: status bar, scrollable conversation, tool activity, multi-line input
- **Markdown rendering**: headings, bold/italic, code blocks, lists, blockquotes via pulldown-cmark
- **Session persistence**: runtime carries state across turns (no re-initialization)
- **Live streaming**: text deltas appear in real-time with spinner animation
- **Tool activity panel**: shows running/succeeded/failed tool invocations
- **Auto-scroll**: follows new content, Shift+Up/Down and PgUp/PgDn to scroll back
- **Multi-line input**: Shift+Enter for newlines, dynamic input height
- **Input history**: Up/Down arrows navigate previous inputs
- **Status bar**: model, token counts, cache stats, cost, git branch, elapsed time
- **Slash commands**: /help, /status, /clear, /exit

## Model Aliases

Short names resolve to the latest model versions:

| Alias | Resolves To |
|-------|------------|
| `opus` | `claude-opus-4-6` |
| `sonnet` | `claude-sonnet-4-6` |
| `haiku` | `claude-haiku-4-5-20251213` |

## CLI Flags

```
rune [OPTIONS] [COMMAND]

Options:
  --model MODEL                    Override the active model
  --dangerously-skip-permissions   Skip all permission checks
  --permission-mode MODE           Set read-only, workspace-write, or danger-full-access
  --allowedTools TOOLS             Restrict enabled tools
  --output-format FORMAT           Non-interactive output format (text or json)
  --resume SESSION                 Re-open a saved session or inspect it with slash commands
  --tui                            Launch the TUI frontend instead of the REPL
  --version, -V                    Print version and build information locally

Commands:
  prompt <text>      One-shot prompt (non-interactive)
  login              Authenticate via OAuth
  logout             Clear stored credentials
  init               Initialize project config
  status             Show the current workspace status snapshot
  sandbox            Show the current sandbox isolation snapshot
  agents             Inspect agent definitions
  mcp                Inspect configured MCP servers
  skills             Inspect installed skills
  system-prompt      Render the assembled system prompt
```

For the current canonical help text, run `cargo run -p rusty-claude-cli -- --help`.

## Slash Commands (REPL)

Tab completion expands slash commands, model aliases, permission modes, and recent session IDs.

**Session & Navigation**
| Command | Description |
|---------|-------------|
| `/help` | Show categorized help |
| `/status` | Show session status (model, tokens, cost) |
| `/cost` | Show cost breakdown |
| `/version` | Show version |
| `/clear` | Clear conversation |
| `/compact` | Compact conversation history |
| `/exit` | Exit the REPL |
| `/resume [id]` | Resume a saved conversation |
| `/session [action]` | Manage sessions (list, switch, delete) |
| `/export [path]` | Export conversation |

**Model & Configuration**
| Command | Description |
|---------|-------------|
| `/model [name]` | Show or switch model |
| `/permissions [mode]` | Show or switch permission mode |
| `/config [section]` | Show config (env, hooks, model) |
| `/memory` | Show CLAUDE.md contents |
| `/effort [level]` | Set reasoning effort (low/medium/high) |
| `/fast` | Toggle fast/quality mode |
| `/vim` | Toggle vim keybindings |
| `/advisor` | Toggle advisor mode (read-only suggestions) |
| `/brief` | Toggle brief output mode |

**Development Tools**
| Command | Description |
|---------|-------------|
| `/diff` | Show git diff |
| `/commit` | Generate a commit message |
| `/pr [context]` | Draft a pull request |
| `/review [scope]` | Code review |
| `/doctor` | Run diagnostics |
| `/init` | Initialize project config |
| `/context [action]` | Manage context files |
| `/copy [target]` | Copy last response to clipboard |
| `/bughunter [scope]` | Automated bug hunting |
| `/ultraplan [task]` | Deep planning mode |

**Plugin & MCP**
| Command | Description |
|---------|-------------|
| `/mcp [action]` | Manage MCP servers |
| `/plugins [action]` | Manage plugins |
| `/hooks [args]` | Manage hooks |
| `/agents [args]` | Inspect agent definitions |
| `/skills [args]` | Inspect installed skills |

See [`../USAGE.md`](../USAGE.md) for examples covering interactive use, JSON automation, sessions, permissions, and the mock parity harness.

## Workspace Layout

```
rust/
├── Cargo.toml              # Workspace root
├── Cargo.lock
└── crates/
    ├── api/                # Anthropic API client + SSE streaming + multi-provider
    ├── commands/           # Shared slash-command registry (40+ commands)
    ├── compat-harness/     # TS manifest extraction harness
    ├── mock-anthropic-service/ # Deterministic local Anthropic-compatible mock
    ├── plugins/            # Plugin registry, hook integration, bundled plugins
    ├── rune-tui/           # TUI frontend (ratatui + crossterm)
    ├── runtime/            # Session, config, permissions, MCP, prompts, OAuth
    ├── rusty-claude-cli/   # Main CLI binary — REPL + one-shot modes
    ├── telemetry/          # Session tracing and usage telemetry types
    └── tools/              # Built-in tool implementations (15+ tools)
```

### Crate Responsibilities

- **api** — HTTP client, SSE stream parser, request/response types, auth (API key + OAuth bearer), multi-provider routing (Anthropic, xAI/Grok, OpenAI-compatible endpoints, LiteLLM)
- **commands** — Slash command definitions, parsing, and categorized help text generation
- **compat-harness** — Extracts tool/prompt manifests from upstream TS source
- **mock-anthropic-service** — Deterministic `/v1/messages` mock for CLI parity tests and local harness runs
- **plugins** — Plugin metadata, registries, hook integration surfaces, bundled plugin support
- **rune-tui** — ratatui-based TUI frontend with markdown rendering, tool activity panel, and streaming support
- **runtime** — `ConversationRuntime` agentic loop, `ConfigLoader` hierarchy, `Session` persistence, permission policy, MCP client, system prompt assembly, usage tracking, OAuth token management
- **rusty-claude-cli** — REPL (rustyline), one-shot prompt, streaming display, tool call rendering, CLI argument parsing, tab completion
- **telemetry** — Session trace events and supporting telemetry payloads
- **tools** — Tool specs + execution: Bash, ReadFile, WriteFile, EditFile, GlobSearch, GrepSearch, WebSearch, WebFetch, Agent, TodoWrite, NotebookEdit, Skill, ToolSearch, REPL runtimes

## Architecture

The runtime is frontend-agnostic via traits:

```rust
// Any frontend implements these two traits:
trait ApiClient: Send       // Handles streaming API communication
trait ToolExecutor: Send    // Handles tool execution + permission prompts

// The runtime works with any frontend:
ConversationRuntime<C: ApiClient, T: ToolExecutor>
```

This design allows both the REPL and TUI frontends to share the same agentic loop, session persistence, and tool system.

## Stats

- **~63K lines** of Rust
- **10 crates** in workspace
- **Binary name:** `rune` (CLI), `rune-tui` (TUI)
- **Default model:** `claude-opus-4-6`
- **Default permissions:** `danger-full-access`

## License

See repository root.
