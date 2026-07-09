# README hero GIF — the amber-fork moment on the bundled pair (issue #5 slice 2).
#
# Regenerate from the repo root (the GIF is a build artifact of this tape):
#   cargo build --release -p amberfork-cli
#   vhs docs/assets/demo.tape
#
# Theme = DESIGN.md dark neutrals (bg #0A0A0B, text #E6E6E9); truecolor is forced so the
# divergence accent renders as the exact design-system amber #FF7A1A. Font = Monaspace Neon
# (the design system's data/code face; brew install --cask font-monaspace).

Output docs/assets/demo.gif

Set Shell "bash"
Set FontFamily "Monaspace Neon"
Set FontSize 15
Set Width 1180
Set Height 470
Set Padding 24
Set Theme { "background": "#0A0A0B", "foreground": "#E6E6E9", "cursor": "#E6E6E9", "black": "#121214", "brightBlack": "#55555C", "white": "#E6E6E9", "brightWhite": "#E6E6E9", "red": "#FF5C5C", "brightRed": "#FF5C5C", "green": "#46D39A", "brightGreen": "#46D39A", "yellow": "#F5A623", "brightYellow": "#FF7A1A", "blue": "#5AA2FF", "brightBlue": "#5AA2FF", "magenta": "#8A8A93", "brightMagenta": "#8A8A93", "cyan": "#5AA2FF", "brightCyan": "#5AA2FF" }

Env COLORTERM "truecolor"

# The recording shows the command a user actually types, so the release binary goes on PATH
# out of frame.
Hide
Type `export PATH="$PWD/target/release:$PATH" && clear`
Enter
Show

Sleep 1s
Type@70ms "amberfork demo"
Sleep 600ms
Enter
# Hold on the result long enough to read the fork block; the GIF loops.
Sleep 14s
