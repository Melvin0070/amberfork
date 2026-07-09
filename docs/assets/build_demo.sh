#!/usr/bin/env bash
# Build the README hero GIF. Pipeline: VHS renders lossless layered PNG frames -> we flatten
# (text layer + cursor overlay on the theme background) and rotate the sequence so the poster
# frame (what GitHub shows before autoplay) is the finished amber-fork result, not a blank
# prompt -> gifski encodes. gifski (libimagequant + temporal dithering) renders anti-aliased
# glyphs on #0A0A0B noticeably cleaner than VHS's built-in ffmpeg palettegen.
#
# Run from anywhere after: cargo build --release -p amberfork-cli
set -euo pipefail

cd "$(dirname "$0")/../.."   # repo root
ASSETS="docs/assets"
FRAMES="$ASSETS/.frames"
ROT="$ASSETS/.rot"
OUT="$ASSETS/demo.gif"
BG="#0A0A0B"                 # DESIGN.md terminal background
FPS=30

command -v vhs    >/dev/null || { echo "need vhs: brew install vhs"; exit 1; }
command -v gifski >/dev/null || { echo "need gifski: brew install gifski"; exit 1; }
[ -x target/release/amberfork ] || { echo "build first: cargo build --release -p amberfork-cli"; exit 1; }

rm -rf "$FRAMES" "$ROT"
vhs "$ASSETS/demo.tape"

# Flatten every frame (text + cursor over the background), find the loop-start frame R = the
# single biggest screen change (the output printing in one burst), and write the frames out in
# rotated order [R..N] ++ [1..R-1] so frame 0 of the GIF is the finished result.
mkdir -p "$ROT"
python3 - "$FRAMES" "$ROT" "$BG" <<'PY'
import sys, glob, os
from PIL import Image, ImageChops, ImageStat

frames_dir, rot_dir, bg_hex = sys.argv[1], sys.argv[2], sys.argv[3]
bg = tuple(int(bg_hex[i:i+2], 16) for i in (1, 3, 5)) + (255,)

texts   = sorted(glob.glob(os.path.join(frames_dir, "frame-text-*.png")))
cursors = sorted(glob.glob(os.path.join(frames_dir, "frame-cursor-*.png")))
assert texts and len(texts) == len(cursors), "expected matching text/cursor frame layers"

flat, prev, R, best = [], None, 1, -1.0
for i, (t, c) in enumerate(zip(texts, cursors)):
    base = Image.new("RGBA", Image.open(t).size, bg)
    base.alpha_composite(Image.open(t).convert("RGBA"))
    base.alpha_composite(Image.open(c).convert("RGBA"))
    rgb = base.convert("RGB")
    flat.append(rgb)
    g = rgb.convert("L")
    if prev is not None:
        d = ImageStat.Stat(ImageChops.difference(g, prev)).mean[0]
        if d > best:
            best, R = d, i        # i = the frame that first shows the full result
    prev = g

order = list(range(R, len(flat))) + list(range(0, R))
for seq, idx in enumerate(order):
    flat[idx].save(os.path.join(rot_dir, f"{seq:06d}.png"))
print(f"frames: {len(flat)}   loop starts at frame {R + 1} (the result burst)")
PY

gifski --fps "$FPS" --quality 100 -o "$OUT" "$ROT"/*.png

rm -rf "$FRAMES" "$ROT"
echo "wrote $OUT ($(du -h "$OUT" | cut -f1))"
