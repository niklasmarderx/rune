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
  <a href="#configuration">Configuration</a> &middot;
  <a href="#architecture">Architecture</a> &middot;
  <a href="#roadmap">Roadmap</a>
</p>

---

## Why Rune?

Most AI coding tools lock you into a single provider and a single workflow. Rune is different:

- **Multi-provider from day one** — Anthropic, OpenAI, XAI, or any LiteLLM-compatible proxy. Bring your own API or route through your company's gateway.
- **Permission-first** — Three modes (`read-only`, `workspace-write`, `danger-full-access`) so you control exactly what the AI can touch.
- **Built in Rust** — Fast startup, low memory, no Node.js/Python runtime needed.
- **Extensible** — MCP servers, plugins, custom slash commands, and instruction files.

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

# One-shot prompt
./target/debug/rune prompt "explain this codebase"

# Shorthand
./target/debug/rune "summarize README.md"
```

### Install globally

```bash
cargo install --path crates/rusty-claude-cli
rune    # available everywhere
```

---

## Features

### Interactive REPL

Full-featured terminal REPL with tab completion, streaming responses, extended thinking, and slash commands.

```
$ rune
██████╗ ██╗   ██╗███╗   ██╗███████╗
██╔══██╗██║   ██║████╗  ██║██╔════╝
██████╔╝██║   ██║██╔██╗ ██║█████╗
██╔══██╗██║   ██║██║╚██╗██║██╔══╝
██║  ██║╚██████╔╝██║ ╚████║███████╗
╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═══╝╚══════╝ Code ᚱ

Model            claude-opus-4-6
Permissions      danger-full-access
Branch           main
> _
```

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
| `Agent` | Launch sub-agent tasks | varies |
| `Skill` | Load local skill definitions | read-only |

### Slash Commands

```
/help          Show all commands
/status        Live context (git, workspace, session)
/cost          Token usage and cost breakdown
/config        Inspect loaded configuration
/model         View or switch model
/permissions   View or change permission mode
/diff          Show uncommitted changes
/export        Export session to file
/compact       Compress conversation history
/resume        Resume a previous session
/init          Create starter RUNE.md
/memory        Inspect loaded instruction files
```

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

### JSON Output

For scripting and automation:

```bash
rune --output-format json prompt "list all TODO items"
```

---

## Configuration

### Config File Hierarchy

Config is loaded in this order (later overrides earlier):

| Priority | Path | Scope |
|----------|------|-------|
| 1 | `~/.rune.json` | User global |
| 2 | `~/.config/rune/settings.json` | User global (XDG) |
| 3 | `<repo>/.rune.json` | Project shared |
| 4 | `<repo>/.rune/settings.json` | Project shared |
| 5 | `<repo>/.rune/settings.local.json` | Project local (gitignored) |

### Environment Variables

| Variable | Purpose |
|----------|---------|
| `RUNE_CONFIG_HOME` | Override config directory |
| `RUNE_AUTO_COMPACT_INPUT_TOKENS` | Auto-compaction threshold |
| `RUNE_REMOTE` | Enable remote session mode |

---

## Architecture

Rune is a Rust workspace with 9 specialized crates:

```
rust/crates/
├── api/                   # HTTP client, SSE streaming, multi-provider
├── commands/              # Slash command registry + help text
├── runtime/               # Core: agentic loop, sessions, permissions, MCP, config
├── rusty-claude-cli/      # CLI binary, REPL, terminal rendering
├── tools/                 # Built-in tool implementations
├── plugins/               # Plugin system + hooks
├── telemetry/             # Session tracing, cost tracking
├── mock-anthropic-service/# Deterministic mock API for testing
└── compat-harness/        # Upstream manifest extraction
```

### Key Design Decisions

- **State machine-first**: Worker states, MCP phases, and session lifecycle are all explicit state machines
- **Events over logs**: Typed lane events instead of text scraping
- **Recovery before escalation**: Automatic retry for known failure modes
- **Config hierarchy**: 5-level config resolution with clear override semantics

---

## Roadmap

See [ROADMAP.md](ROADMAP.md) for the full 5-phase plan:

1. **Reliable Worker Boot** — State machine lifecycle, trust resolution
2. **Event-Native Integration** — Lane events, failure taxonomy
3. **Branch/Test Awareness** — Auto-recovery, green-ness contract
4. **Agent-First Task Execution** — Task packets, policy engine
5. **Plugin & MCP Maturity** — First-class lifecycle contracts

---

## Development

### Verification

```bash
cd rust
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### Upstream Updates

This project tracks [ultraworkers/claw-code](https://github.com/ultraworkers/claw-code) for upstream improvements:

```bash
./scripts/rune-upstream-check.sh           # See new upstream commits
./scripts/rune-upstream-check.sh --detail  # With file changes
git cherry-pick <hash>                     # Adopt specific commits
```

---

## License

MIT
