# Rune Code

<p align="center">
<pre align="center">
  ██████╗ ██╗   ██╗███╗   ██╗███████╗
  ██╔══██╗██║   ██║████╗  ██║██╔════╝
██████╔╝██║   ██║██╔██╗ ██║█████╗
██╔══██╗██║   ██║██║╚██╗██║██╔══╝
  ██║  ██║╚██████╔╝██║ ╚████║███████╗
           ╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═══╝╚══════╝   Code ᚱ
</pre>
</p>

<p align="center">
  <strong>A high-performance AI coding CLI built in Rust</strong><br/>
  <em>Multi-provider &middot; Permission-first &middot; Extensible &middot; Fast</em>
</p>


<p align="center">
  <a href="#quickstart">Quickstart</a> &middot;
  <a href="#features">Features</a> &middot;
  <a href="#tui-frontend">TUI</a> &middot;
  <a href="#configuration">Configuration</a> &middot;
  <a href="#architecture">Architecture</a> &middot;
  <a href="#roadmap">Roadmap</a>
</p>

<p align="center">
  <a href="https://niklasmarderx.github.io/rune/"><strong>🌐 Rune Code — Project Website</strong></a>

</p>

<p align="center">
  <img src="assets/demo.gif" alt="Rune Code Demo" width="800"/>
</p>

---

## Why Rune?

Most AI coding tools lock you into a single provider and a single workflow. Rune is different:

- **Multi-provider from day one** — Anthropic, OpenAI, XAI, or any LiteLLM-compatible proxy. Bring your own API or route through your company's gateway.
- **Permission-first** — Three modes (`read-only`, `workspace-write`, `danger-full-access`) so you control exactly what the AI can touch.
- **Built in Rust** — Fast startup, low memory, no Node.js/Python runtime needed. ~63K lines across 10 crates.
- **Two frontends** — Interactive REPL (rustyline) and a full TUI (ratatui) with markdown rendering, tool activity panel, and streaming.
- **Extensible** — MCP servers, plugins, hooks, custom slash commands, and instruction files.

---

## Quickstart

### Prerequisites

- Rust toolchain (`rustup`)
- One of: `ANTHROPIC_API_KEY`, `LITELLM_API_KEY`, `OPENAI_API_KEY`, or OAuth

### Build & Run

```bash
git clone https://github.com/niklasmarderx/rune.git
cd rune/rust
cargo build --workspace

# Interactive REPL
./target/debug/rune

# TUI frontend (ratatui-based)
./target/debug/rune --tui
# Or directly:
./target/debug/rune-tui

# One-shot prompt
./target/debug/rune prompt "explain this codebase"

# JSON output for automation
./target/debug/rune --output-format json prompt "summarize README.md"
```

### Install globally

```bash
cargo install --path crates/rusty-claude-cli
rune    # available everywhere
```

---

## Features

| Feature | Status |
|---------|--------|
| Anthropic API + streaming | ✅ |
| Multi-provider (Anthropic, xAI/Grok, OpenAI-compat, LiteLLM) | ✅ |
| OAuth login/logout | ✅ |
| Interactive REPL (rustyline, tab completion) | ✅ |
| TUI frontend (ratatui, markdown, streaming) | ✅ |
| 15+ built-in tools | ✅ |
| 40+ slash commands | ✅ |
| Sub-agent orchestration | ✅ |
| RUNE.md / project memory | ✅ |
| Config file hierarchy | ✅ |
| Permission system (3 modes) | ✅ |
| MCP server lifecycle | ✅ |
| Session persistence + resume | ✅ |
| Extended thinking | ✅ |
| Cost tracking + usage display | ✅ |
| Git integration | ✅ |
| Markdown terminal rendering | ✅ |
| Model aliases (opus/sonnet/haiku) | ✅ |
| Hooks (PreToolUse/PostToolUse) | ✅ |
| Plugin system | ✅ |
| Vim mode | ✅ |
| Advisor mode (read-only suggestions) | ✅ |
| Reasoning effort control | ✅ |

### Built-in Tools

| Tool | Description | Permission |
|------|-------------|------------|
| `bash` | Execute shell commands | danger-full-access |
| `read_file` | Read files from workspace | read-only |
| `write_file` | Write/create files | workspace-write |
| `edit_file` | Patch files with targeted edits | workspace-write |
| `glob_search` | Find files by pattern | read-only |
| `grep_search` | Regex search file contents | read-only |
| `WebFetch` | Fetch and analyze URLs | read-only |
| `WebSearch` | Search the web | read-only |
| `TodoWrite` | Structured task tracking | workspace-write |
| `NotebookEdit` | Edit Jupyter notebooks | workspace-write |
| `Agent` | Launch sub-agent tasks | varies |
| `Skill` | Load local skill definitions | read-only |

### Slash Commands

**Session & Navigation**

| Command | Description |
|---------|-------------|
| `/help` | Show categorized help |
| `/status` | Session status (model, tokens, cost) |
| `/cost` | Cost breakdown |
| `/version` | Version info |
| `/clear` | Clear conversation |
| `/compact` | Compact conversation history |
| `/exit` | Exit |
| `/resume [id]` | Resume a saved conversation |
| `/session [action]` | Manage sessions |
| `/export [path]` | Export conversation |

**Model & Configuration**

| Command | Description |
|---------|-------------|
| `/model [name]` | Show or switch model |
| `/permissions [mode]` | Show or switch permission mode |
| `/config [section]` | Show config |
| `/memory` | Show RUNE.md contents |
| `/effort [level]` | Set reasoning effort |
| `/fast` | Toggle fast/quality mode |
| `/vim` | Toggle vim keybindings |
| `/advisor` | Toggle advisor mode |
| `/brief` | Toggle brief output |

**Development Tools**

| Command | Description |
|---------|-------------|
| `/diff` | Show git diff |
| `/commit` | Generate commit message |
| `/pr [context]` | Draft a pull request |
| `/review [scope]` | Code review |
| `/doctor` | Run diagnostics |
| `/init` | Initialize project config |
| `/context [action]` | Manage context files |
| `/copy [target]` | Copy to clipboard |
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

### Multi-Provider Support

| Provider | Auth | Env Vars |
|----------|------|----------|
| **Anthropic** | API Key or OAuth | `ANTHROPIC_API_KEY`, `ANTHROPIC_BASE_URL` |
| **LiteLLM** | API Key | `LITELLM_API_KEY`, `LITELLM_BASE_URL` |
| **OpenAI** | API Key | `OPENAI_API_KEY`, `OPENAI_BASE_URL` |
| **XAI (Grok)** | API Key | `XAI_API_KEY`, `XAI_BASE_URL` |

LiteLLM takes priority when `LITELLM_API_KEY` is set — any model name (including `claude-*`) routes through your proxy.

### Model Aliases

```bash
rune --model opus     # claude-opus-4-6
rune --model sonnet   # claude-sonnet-4-6
rune --model haiku    # claude-haiku-4-5
```

### Session Management

Sessions auto-save to `.rune/sessions/` and can be resumed:

```bash
rune --resume latest              # Resume most recent session
rune --resume session-abc123      # Resume by ID
rune --resume latest /status      # Resume and run commands
```

### Permission Modes

```bash
rune --permission-mode read-only           # Can only read files
rune --permission-mode workspace-write     # Can read + write files
rune --permission-mode danger-full-access  # Full system access
```

---

## TUI Frontend

The `rune-tui` crate provides a full ratatui-based terminal UI as an alternative to the REPL:

```bash
rune --tui
# or
rune-tui
```

- **4-panel layout**: status bar, scrollable conversation, tool activity, multi-line input
- **Markdown rendering**: headings, bold/italic, code blocks, lists, blockquotes
- **Live streaming**: text deltas appear in real-time with spinner animation
- **Tool activity panel**: running/succeeded/failed tool invocations
- **Auto-scroll**: follows new content, Shift+Up/Down and PgUp/PgDn to scroll back
- **Multi-line input**: Shift+Enter for newlines, dynamic input height
- **Input history**: Up/Down arrows navigate previous inputs
- **Status bar**: model, token counts, cache stats, cost, git branch, elapsed time

---

## Configuration

### Instruction Files

Rune reads project-specific instructions from (in order):

1. `RUNE.md` — Project root instructions
2. `RUNE.local.md` — Local overrides (gitignored)
3. `.rune/RUNE.md` — Nested instructions
4. `.rune/instructions.md` — Alternative location

Create a starter file:
```bash
rune init
```

### Config File Hierarchy

Config is loaded in this order (later overrides earlier):

| Priority | Path | Scope |
|----------|------|-------|
| 1 | `~/.rune.json` | User global |
| 2 | `~/.config/rune/settings.json` | User global (XDG) |
| 3 | `<repo>/.rune.json` | Project shared |
| 4 | `<repo>/.rune/settings.json` | Project shared |
| 5 | `<repo>/.rune/settings.local.json` | Project local (gitignored) |

### MCP Server Integration

Configure MCP servers in `.rune/settings.json`:

```json
{
  "mcpServers": {
    "my-server": {
      "command": "uvx",
      "args": ["my-mcp-server"],
      "env": { "API_KEY": "..." }
    }
  }
}
```

### Environment Variables

| Variable | Purpose |
|----------|---------|
| `RUNE_CONFIG_HOME` | Override config directory |
| `RUNE_AUTO_COMPACT_INPUT_TOKENS` | Auto-compaction threshold |
| `RUNE_REMOTE` | Enable remote session mode |

---

## CLI Reference

```
rune [OPTIONS] [COMMAND]

Options:
  --model MODEL                    Override the active model
  --dangerously-skip-permissions   Skip all permission checks
  --permission-mode MODE           read-only, workspace-write, or danger-full-access
  --allowedTools TOOLS             Restrict enabled tools
  --output-format FORMAT           Non-interactive output (text or json)
  --resume SESSION                 Re-open a saved session
  --tui                            Launch the TUI frontend
  --version, -V                    Print version

Commands:
  prompt <text>      One-shot prompt (non-interactive)
  login              Authenticate via OAuth
  logout             Clear stored credentials
  init               Initialize project config
  status             Show workspace status
  sandbox            Show sandbox isolation status
  agents             Inspect agent definitions
  mcp                Inspect configured MCP servers
  skills             Inspect installed skills
  system-prompt      Render the assembled system prompt
```

---

## Architecture

Rune is a Rust workspace with 10 specialized crates:

```
rust/crates/
├── api/                   # HTTP client, SSE streaming, multi-provider routing
├── commands/              # Slash command registry (40+ commands) + help text
├── runtime/               # Core: agentic loop, sessions, permissions, MCP, config, OAuth
├── rusty-claude-cli/      # CLI binary, REPL, terminal rendering, tab completion
├── rune-tui/              # TUI frontend (ratatui + crossterm)
├── tools/                 # Built-in tool implementations (15+ tools)
├── plugins/               # Plugin system, hook integration, bundled plugins
├── telemetry/             # Session tracing, cost tracking
├── mock-anthropic-service/# Deterministic mock API for testing
└── compat-harness/        # Upstream manifest extraction
```

### Key Design Decisions

- **Frontend-agnostic runtime**: `ConversationRuntime<C: ApiClient, T: ToolExecutor>` — both REPL and TUI share the same agentic loop
- **State machine-first**: Worker states, MCP phases, and session lifecycle are all explicit state machines
- **Events over logs**: Typed lane events instead of text scraping
- **Recovery before escalation**: Automatic retry for known failure modes
- **Config hierarchy**: 5-level config resolution with clear override semantics

---

## Development

### Verification

```bash
cd rust
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### Mock Parity Harness

Deterministic Anthropic-compatible mock service for end-to-end testing:

```bash
cd rust
./scripts/run_mock_parity_harness.sh
```

Coverage: streaming text, file read/write roundtrips, tool use, permission prompts, plugin tools.

### Upstream Updates

This project tracks [ultraworkers/claw-code](https://github.com/ultraworkers/claw-code) for upstream improvements:

```bash
./scripts/rune-upstream-check.sh           # See new upstream commits
./scripts/rune-upstream-check.sh --detail  # With file changes
```

---

## Roadmap

See [ROADMAP.md](ROADMAP.md) for the full 5-phase plan:

1. **Reliable Worker Boot** — State machine lifecycle, trust resolution
2. **Event-Native Integration** — Lane events, failure taxonomy
3. **Branch/Test Awareness** — Auto-recovery, green-ness contract
4. **Agent-First Task Execution** — Task packets, policy engine
5. **Plugin & MCP Maturity** — First-class lifecycle contracts

---

## Stats

- **~63K lines** of Rust
- **10 crates** in workspace
- **15+ built-in tools**
- **40+ slash commands**
- **Default model:** `claude-opus-4-6`

---

## License

MIT
