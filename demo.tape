# Rune Demo — Rust CLI Agent
# Generate with: vhs demo.tape

Output demo.gif

Set Shell "zsh"
Set FontSize 15
Set Width 960
Set Height 540
Set Theme "Catppuccin Mocha"
Set TypingSpeed 30ms
Set Padding 20

# ── Version ───────────────────────────────────────────────
Sleep 300ms
Type "rune --version"
Enter
Sleep 1.5s

# ── Help overview ─────────────────────────────────────────
Type "rune --help"
Enter
Sleep 3s

# ── Workspace status ──────────────────────────────────────
Type "rune status"
Enter
Sleep 3s

# ── System prompt inspection ──────────────────────────────
Type "rune system-prompt 2>&1 | head -15"
Enter
Sleep 2.5s

# ── MCP servers ───────────────────────────────────────────
Type "rune mcp"
Enter
Sleep 2s

# ── Agents ────────────────────────────────────────────────
Type "rune agents"
Enter
Sleep 2s

# ── Skills ────────────────────────────────────────────────
Type "rune skills"
Enter
Sleep 2s

# ── Sandbox check ─────────────────────────────────────────
Type "rune sandbox"
Enter
Sleep 2.5s

# ── Tagline ───────────────────────────────────────────────
Sleep 500ms
Type "echo '⚡ rune — Built with Rust. Powered by Claude.'"
Enter
Sleep 2.5s
