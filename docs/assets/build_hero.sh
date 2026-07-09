#!/usr/bin/env bash
# Build the README hero GIF from the designed animation (hero.html).
#
# Pipeline: headless Chrome renders hero.html's deterministic timeline to lossless 2x PNG frames
# (render.mjs) -> the frames are rotated so the GIF's poster is the fully-lit "divergence" payoff,
# not the empty build-up -> gifski downscales 2x->1x and encodes with temporal dithering.
#
# Requires: node, gifski (brew install gifski), and Google Chrome. playwright-core is installed
# into a scratch dir on the fly, so nothing but hero.gif is written under docs/assets.
set -euo pipefail

cd "$(dirname "$0")"          # docs/assets
FPS=50
WIDTH=1200                    # output width (frames render at 2x = 2400, gifski downscales)
QUALITY=80                    # gifski quality; the flat palette stays crisp this low
OUT="hero.gif"

command -v node   >/dev/null || { echo "need node"; exit 1; }
command -v gifski >/dev/null || { echo "need gifski: brew install gifski"; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cp render.mjs "$WORK/"
( cd "$WORK" && npm init -y >/dev/null 2>&1 && npm i --no-save playwright-core >/dev/null 2>&1 )

INFO="$(node "$WORK/render.mjs" "$PWD/hero.html" "$WORK/frames" "$FPS" | grep '^RENDER ' | cut -d' ' -f2-)"
N=$(node -e "process.stdout.write(String(JSON.parse(process.argv[1]).frames))" "$INFO")
POSTER=$(node -e "process.stdout.write(String(JSON.parse(process.argv[1]).poster))" "$INFO")
echo "rendered $N frames · poster=frame $POSTER"

# Rotate [POSTER..N-1] ++ [0..POSTER-1] so frame 0 is the payoff; loop wrap stays seamless.
mkdir -p "$WORK/rot"; seq=0
for i in $(seq "$POSTER" $((N-1))) $(seq 0 $((POSTER-1))); do
  ln -sf "$WORK/frames/$(printf 'f_%05d.png' "$i")" "$(printf '%s/rot/%06d.png' "$WORK" "$seq")"
  seq=$((seq+1))
done

gifski --fps "$FPS" --width "$WIDTH" --quality "$QUALITY" -o "$OUT" "$WORK"/rot/*.png
echo "wrote $OUT ($(du -h "$OUT" | cut -f1))"
