# Rune Code Philosophy

## Principles

Rune Code is built on a small set of convictions about what AI-assisted coding tools should be.

### 1. Multi-provider support is a feature, not a compromise

Developers should not be locked into a single AI provider. Rune supports multiple backends so you can choose the model that fits the task, switch providers when pricing or capabilities change, and avoid vendor lock-in without changing your workflow.

### 2. Permission-first design

Every tool action that touches the filesystem, runs a process, or reaches the network should be gated by an explicit permission model. Read-only, workspace-write, and full-access modes exist so that the default is safe and the user opts in to power, not out of risk.

### 3. Extensibility through MCP and plugins

The tool surface should be open. MCP server integrations, plugin hooks, and skill registries let teams extend Rune without forking it. The core runtime stays small; capabilities grow through composition.

### 4. Machine-readable by default

An AI coding harness is used by humans and by other programs. Output formats, status commands, session state, and event streams should be structured and deterministic so that automation is a first-class use case, not an afterthought.

### 5. Honest parity tracking

When reimplementing an existing tool surface, track what works and what does not. Parity documents should be maintained and honest. Stubs should be labeled as stubs. Partial implementations should say so. Users and contributors deserve accurate expectations.

### 6. Small, reviewable changes

Large undifferentiated diffs are hard to review and easy to regress. Prefer incremental changes that can be understood, tested, and merged independently.

## What still matters

As coding intelligence becomes cheaper and more widely available, the durable differentiators are not raw code generation speed. What still matters:

- Product taste and direction
- System design and architecture decisions
- Human trust and operational stability
- Judgment about what to build next

The job of the human is not to out-type the machine. The job of the human is to decide what deserves to exist.

## Short version

Rune Code is a multi-provider, permission-first, extensible AI coding harness. It is built to be safe by default, honest about its capabilities, and open to extension.
