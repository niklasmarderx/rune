Output assets/demo.gif
Set Shell "zsh"
Set FontSize 15
Set Width 900
Set Height 550
Set Padding 16
Set Theme "Catppuccin Mocha"
Set TypingSpeed 35ms
Set PlaybackSpeed 0.5

# Wait for shell to initialize
Sleep 2s

# Show help
Type "/Users/I765354/.cargo/bin/rune --help"
Enter
Sleep 6s

# Init a project
Type "mkdir -p /tmp/demo-rune && cd /tmp/demo-rune"
Enter
Sleep 1s
Type "/Users/I765354/.cargo/bin/rune init"
Enter
Sleep 4s

# Show the generated file
Type "head -15 RUNE.md"
Enter
Sleep 4s
