# Rune Code Usage

This guide covers the current Rust workspace under `rust/` and the `rune` CLI binary.

## Prerequisites

- Rust toolchain with `cargo`
- One of:
  - `ANTHROPIC_API_KEY` for direct API access
  - `rune login` for OAuth-based auth
- Optional: `ANTHROPIC_BASE_URL` when targeting a proxy or local service

## Build the workspace

```bash
cd rust
cargo build --workspace
```

The CLI binary is available at `rust/target/debug/rune` after a debug build.

## Quick start

### Interactive REPL

```bash
cd rust
./target/debug/rune
```

### One-shot prompt

```bash
cd rust
./target/debug/rune prompt "summarize this repository"
```

### Shorthand prompt mode

```bash
cd rust
./target/debug/rune "explain rust/crates/runtime/src/lib.rs"
```

### JSON output for scripting

```bash
cd rust
./target/debug/rune --output-format json prompt "status"
```

## Model and permission controls

```bash
cd rust
./target/debug/rune --model sonnet prompt "review this diff"
./target/debug/rune --permission-mode read-only prompt "summarize Cargo.toml"
./target/debug/rune --permission-mode workspace-write prompt "update README.md"
./target/debug/rune --allowedTools read,glob "inspect the runtime crate"
```

Supported permission modes:

- `read-only`
- `workspace-write`
- `danger-full-access`

Model aliases currently supported by the CLI:

- `opus` → `claude-opus-4-6`
- `sonnet` → `claude-sonnet-4-6`
- `haiku` → `claude-haiku-4-5-20251213`

## Authentication

### API key

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

### OAuth

```bash
cd rust
./target/debug/rune login
./target/debug/rune logout
```

## Common operational commands

```bash
cd rust
./target/debug/rune status
./target/debug/rune sandbox
./target/debug/rune agents
./target/debug/rune mcp
./target/debug/rune skills
./target/debug/rune system-prompt --cwd .. --date 2026-04-04
```

## Session management

REPL turns are persisted under `.rune/sessions/` in the current workspace.

```bash
cd rust
./target/debug/rune --resume latest
./target/debug/rune --resume latest /status /diff
```

Useful interactive commands include `/help`, `/status`, `/cost`, `/config`, `/session`, `/model`, `/permissions`, and `/export`.

## Config file resolution order

Runtime config is loaded in this order, with later entries overriding earlier ones:

1. `~/.rune.json`
2. `~/.config/rune/settings.json`
3. `<repo>/.rune.json`
4. `<repo>/.rune/settings.json`
5. `<repo>/.rune/settings.local.json`

## Mock parity harness

The workspace includes a deterministic Anthropic-compatible mock service and parity harness.

```bash
cd rust
./scripts/run_mock_parity_harness.sh
```

Manual mock service startup:

```bash
cd rust
cargo run -p mock-anthropic-service -- --bind 127.0.0.1:0
```

## Verification

```bash
cd rust
cargo test --workspace
```

## Workspace overview

Current Rust crates:

- `api`
- `commands`
- `compat-harness`
- `mock-anthropic-service`
- `plugins`
- `runtime`
- `rusty-claude-cli`
- `telemetry`
- `tools`
