# Rune Code

<p align="center">
  <strong>A high-performance AI coding CLI built in Rust</strong>
</p>

---

## What is Rune?

Rune is an interactive AI coding assistant that runs in your terminal. It connects to LLM providers (Anthropic, OpenAI, XAI, or any LiteLLM-compatible proxy) and gives you a powerful REPL with built-in tools for reading, writing, searching, and executing code.

### Features

- Interactive REPL with tab completion and streaming responses
- One-shot prompt mode for scripting
- Built-in tools: bash, file ops, glob, grep, web search/fetch, agent, todo
- Multi-provider support: Anthropic, OpenAI, XAI, LiteLLM
- Permission system: read-only, workspace-write, danger-full-access
- MCP server integration
- LSP integration for code intelligence
- Session persistence and resume
- Extended thinking (thinking blocks)
- Cost tracking and usage display
- Slash commands: /help, /status, /cost, /config, /model, /diff, /export, and more

## Quickstart

### Build

```bash
cd rust
cargo build --workspace
```

### Run

```bash
# Interactive REPL
./target/debug/rune

# One-shot prompt
./target/debug/rune prompt "summarize this repository"

# Shorthand
./target/debug/rune "explain src/main.rs"
```

### Install globally

```bash
cd rust
cargo install --path crates/rusty-claude-cli
rune
```

## Configuration

### Authentication

Set one of these environment variables:

```bash
# Direct Anthropic API
export ANTHROPIC_API_KEY="sk-ant-..."

# LiteLLM proxy (recommended)
export LITELLM_API_KEY="your-key"
export LITELLM_BASE_URL="http://localhost:4000"

# Or use OAuth
rune login
```

### Model & Permission Controls

```bash
rune --model sonnet prompt "review this diff"
rune --permission-mode read-only prompt "summarize Cargo.toml"
```

Model aliases: `opus`, `sonnet`, `haiku`

### Config File Hierarchy

Config is loaded in this order (later overrides earlier):

1. `~/.rune.json`
2. `~/.config/rune/settings.json`
3. `<repo>/.rune.json`
4. `<repo>/.rune/settings.json`
5. `<repo>/.rune/settings.local.json`

### Instruction Files

Rune reads project-specific instructions from:
- `RUNE.md`
- `RUNE.local.md`
- `.rune/RUNE.md`
- `.rune/instructions.md`

## Repository Layout

```text
.
├── rust/                          # Rust workspace (9 crates)
│   ├── crates/
│   │   ├── api/                   # HTTP client, SSE streaming, providers
│   │   ├── commands/              # Slash command registry
│   │   ├── runtime/               # Core: agentic loop, sessions, permissions, MCP
│   │   ├── rusty-claude-cli/      # CLI binary, REPL, rendering
│   │   ├── tools/                 # Built-in tool implementations
│   │   ├── plugins/               # Plugin system
│   │   ├── telemetry/             # Session tracing, cost tracking
│   │   ├── mock-anthropic-service/# Mock API for testing
│   │   └── compat-harness/        # Manifest extraction
│   └── Cargo.toml
├── src/                           # Python reference workspace
├── tests/                         # Python tests
└── RUNE.md                        # Project instructions
```

## Session Management

Sessions auto-save to `.rune/sessions/`. Resume with:

```bash
rune --resume latest
```

## Verification

```bash
cd rust
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## License

MIT
