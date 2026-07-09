# README hero GIF — the amber-fork moment on the bundled pair (issue #5 slice 2).
#
# The GIF is a build artifact. Regenerate it (VHS → lossless frames → gifski) from the repo root:
#   cargo build --release -p amberfork-cli
#   ./docs/assets/build_demo.sh
#
# Why gifski and not VHS's built-in GIF encoder: gifski (libimagequant + temporal dithering)
# renders anti-aliased glyphs on the #0A0A0B background noticeably cleaner. build_demo.sh also
# rotates the frame order so the poster frame (what GitHub shows before autoplay) is the finished
# result — the amber fork — not a blank prompt.
#
# Theme = DESIGN.md dark neutrals (bg #0A0A0B, text #E6E6E9); truecolor is forced so the
# divergence accent renders as the exact design-system amber #FF7A1A. Font = Monaspace Neon
# (the design system's data/code face; brew install --cask font-monaspace).

# Lossless PNG frames (VHS emits frame-text + frame-cursor layers); build_demo.sh flattens and
# re-encodes them with gifski. A frames directory is a complete Output on its own.
Output docs/assets/.frames/

Set Shell "bash"
Set FontFamily "Monaspace Neon"
Set FontSize 15
Set Width 1180
Set Height 470
Set Padding 24
Set Framerate 30
Set TypingSpeed 55ms
Set Theme { "background": "#0A0A0B", "foreground": "#E6E6E9", "cursor": "#E6E6E9", "black": "#121214", "brightBlack": "#55555C", "white": "#E6E6E9", "brightWhite": "#E6E6E9", "red": "#FF5C5C", "brightRed": "#FF5C5C", "green": "#46D39A", "brightGreen": "#46D39A", "yellow": "#F5A623", "brightYellow": "#FF7A1A", "blue": "#5AA2FF", "brightBlue": "#5AA2FF", "magenta": "#8A8A93", "brightMagenta": "#8A8A93", "cyan": "#5AA2FF", "brightCyan": "#5AA2FF" }

Env COLORTERM "truecolor"

# Put the release binary on PATH out of frame, so the recording shows the command a user types.
Hide
Type `export PATH="$PWD/target/release:$PATH" && clear`
Enter
Show

# Clean prompt → type the command → run. The output prints in one burst (the CLI's real behavior);
# that burst frame is where build_demo.sh sets the loop start, so the poster is the finished result.
Sleep 500ms
Type "amberfork demo"
Sleep 500ms
Enter
Sleep 300ms
# Hold on the result long enough to read the fork block, then the GIF loops back to this frame.
Sleep 4s
