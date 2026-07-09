#!/usr/bin/env python3
"""Generate docs/assets/hero.svg — the README hero motion graphic.

The SVG is a *designed presentation of real output*: every terminal line is read verbatim
from the committed render snapshot (tests/snapshots/demo_cli__demo_no_color.snap), so the
hero cannot drift from what `amberfork demo` actually prints. Regenerate after any snapshot
change:

    python3 docs/assets/gen_hero.py

Motion follows DESIGN.md: minimal-functional, with exactly ONE expressive beat — the amber
ignites at the fork and flows down the divergent path. Three sequential captions walk the
story (noise absorbed -> fork found -> the cost). Pure CSS animation (no JS — GitHub strips
scripts from README images); `prefers-reduced-motion` shows the final lit state, static.
"""

from pathlib import Path
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parents[2]
SNAPSHOT = ROOT / "crates/amberfork-cli/tests/snapshots/demo_cli__demo_no_color.snap"
OUT = ROOT / "docs/assets/hero.svg"

# ---- DESIGN.md tokens -----------------------------------------------------------------
BG = "#0A0A0B"
SURFACE = "#121214"
HAIRLINE = "#26262B"
TEXT = "#E6E6E9"
MUTED = "#8A8A93"
FAINT = "#55555C"
AMBER = "#FF7A1A"

FONT = '"Monaspace Neon", ui-monospace, "SF Mono", Menlo, "Cascadia Mono", Consolas, monospace'
FONT_SIZE = 15
LINE_H = 21

# ---- panel geometry --------------------------------------------------------------------
W = 960
PAD_X = 20
TITLEBAR_H = 36
FIRST_BASELINE = TITLEBAR_H + 30
CAPTION_BAR_H = 42

# ---- the walkthrough (row indices are into the snapshot's line list) --------------------
COMMAND = "amberfork demo"
CAPTIONS = [
    "1/3 · noise absorbed — B's rate-limit retry is a [log-move], not the fork",
    "2/3 · the fork — same search hits, but B fetched the ARCHIVED v1 policy",
    "3/3 · the cost — payments.refund never runs; a valid refund is denied",
]
FOOTER_NOTE = "terminal output, verbatim"


def snapshot_lines() -> list[str]:
    """The rendered demo output: snapshot body between the insta header and the trailer."""
    raw = SNAPSHOT.read_text().split("---\n", 2)[2]
    return raw.rstrip("\n").split("\n")


def classify(lines: list[str]) -> list[tuple[str, str]]:
    """Tag each snapshot line with its render role, mirroring render.rs's Role rules."""
    fork_row = next(i for i, l in enumerate(lines) if l.startswith("⑂"))
    # The fork block = the ⑂ line plus its indented continuation lines (A/B content).
    block_end = fork_row + 1
    while block_end < len(lines) and lines[block_end].startswith("      "):
        block_end += 1
    roles = []
    for i, line in enumerate(lines):
        if not line.strip():
            roles.append(("blank", line))
        elif i == len(lines) - 1:  # the hand-off hint
            roles.append(("hint", line))
        elif fork_row <= i < block_end:
            roles.append(("fork", line))
        elif i > fork_row:
            roles.append(("downstream", line))
        else:
            roles.append(("spine", line))
    return roles


def text_el(y: int, cls: str, content: str, extra: str = "") -> str:
    return (
        f'<text x="{PAD_X}" y="{y}" class="{cls}" xml:space="preserve"{extra}>'
        f"{content}</text>"
    )


def build() -> str:
    lines = snapshot_lines()
    roles = classify(lines)

    rows = []  # (baseline_y, svg fragment) — row 0 is the typed command
    y = FIRST_BASELINE
    prompt = escape("> ")
    cursor = '<tspan class="cursor">▍</tspan>'
    rows.append(
        f'<text x="{PAD_X}" y="{y}" class="t cmd" xml:space="preserve">'
        f'<tspan class="faint">{prompt}</tspan>{escape(COMMAND)}{cursor}</text>'
    )

    # Downstream rows split prefix|body so only the marker gutter carries amber (DR4).
    # render.rs: prefix = "{gutter} step {idx}  {marker}  " -> 14 chars at idx width 2.
    prefix_len = 14
    ignite_group = 0
    row_geom = {}  # snapshot line index -> baseline y
    for i, (role, line) in enumerate(roles):
        y += LINE_H
        row_geom[i] = y
        if role == "blank":
            continue
        if role == "spine":
            rows.append(text_el(y, "t muted appear", escape(line)))
        elif role == "hint":
            rows.append(text_el(y, "t faint appear", escape(line)))
        elif role == "fork":
            rows.append(text_el(y, "t appear ignite-fork", escape(line)))
        elif role == "downstream":
            ignite_group += 1
            pre, body = line[:prefix_len], line[prefix_len:]
            rows.append(
                f'<text x="{PAD_X}" y="{y}" class="t appear" xml:space="preserve">'
                f'<tspan class="ignite-mark d{ignite_group}">{escape(pre)}</tspan>'
                f'<tspan class="ignite-body d{ignite_group}">{escape(body)}</tspan></text>'
            )
    body_bottom = y + 12
    caption_top = body_bottom
    height = caption_top + CAPTION_BAR_H

    # Highlight rects: beat A = the log-move row; beat C = model-move + final answer rows.
    log_row = next(i for i, (_, l) in enumerate(roles) if "[log-move]" in l)
    model_row = next(i for i, (_, l) in enumerate(roles) if "[model-move]" in l)
    answer_row = len(roles) - 2  # last content row before the hint
    def hl(idx: int, beat: str) -> str:
        return (
            f'<rect x="8" y="{row_geom[idx] - FONT_SIZE}" width="{W - 16}" height="{LINE_H}"'
            f' rx="3" class="hl {beat}"/>'
        )
    highlights = [hl(log_row, "beatA"), hl(model_row, "beatC"), hl(answer_row, "beatC")]

    captions = [
        f'<text x="{PAD_X}" y="{caption_top + 26}" class="cap beat{b}" xml:space="preserve">'
        f"{escape(text)}</text>"
        for b, text in zip("ABC", CAPTIONS)
    ]

    n_type = len(COMMAND) + 2  # prompt reveals on the first steps
    css = f"""
  text {{ font-family: {FONT}; font-size: {FONT_SIZE}px; font-variant-numeric: tabular-nums; }}
  .t {{ fill: {TEXT}; }}
  .muted {{ fill: {MUTED}; }}
  .faint {{ fill: {FAINT}; }}
  .cap {{ fill: {MUTED}; font-size: 12.5px; }}
  .note {{ fill: {FAINT}; font-size: 12px; }}
  .title {{ fill: {FAINT}; font-size: 12px; }}

  /* One 24s master timeline; every animated element carries the same duration so the loop
     stays phase-locked (no animation-delay: it would de-sync on iterations 2+). */
  .cmd     {{ clip-path: inset(0 100% 0 0); animation: typing 24s steps({n_type}, end) infinite; }}
  .cursor  {{ fill: {TEXT}; opacity: 0; animation: cursor 24s steps(1, end) infinite; }}
  .appear  {{ animation: appear 24s linear infinite; }}
  .ignite-fork {{ fill: {AMBER}; animation: igniteFork 24s linear infinite; }}
  .ignite-mark {{ fill: {AMBER}; }}
  .ignite-body {{ fill: {TEXT}; }}
  .hl {{ fill: {TEXT}; opacity: 0; }}
  .cap {{ opacity: 0; }}
  .beatA {{ animation: beatA 24s linear infinite; }}
  .beatC {{ animation: beatC 24s linear infinite; }}
  text.cap.beatB {{ animation: beatB 24s linear infinite; }}
  .fadeall {{ animation: fadeall 24s linear infinite; }}
"""
    # The downstream sweep: each gutter group ignites a beat after the fork block.
    for g in range(1, ignite_group + 1):
        start = 32.5 + g * 0.9
        css += (
            f"  tspan.ignite-mark.d{g} {{ animation: igniteMark{g} 24s linear infinite; }}\n"
            f"  tspan.ignite-body.d{g} {{ animation: igniteBody{g} 24s linear infinite; }}\n"
            f"  @keyframes igniteMark{g} {{ 0%, {start:.1f}% {{ fill: {MUTED}; }}"
            f" {start + 0.8:.1f}%, 100% {{ fill: {AMBER}; }} }}\n"
            f"  @keyframes igniteBody{g} {{ 0%, {start:.1f}% {{ fill: {MUTED}; }}"
            f" {start + 0.8:.1f}%, 100% {{ fill: {TEXT}; }} }}\n"
        )

    css += f"""
  @keyframes typing  {{ 0% {{ clip-path: inset(0 100% 0 0); }} 1.2% {{ clip-path: inset(0 100% 0 0); }} 6% {{ clip-path: inset(0 -2px 0 0); }} 100% {{ clip-path: inset(0 -2px 0 0); }} }}
  @keyframes cursor  {{ 0%, 6% {{ opacity: 0; }} 6.1%, 7.5% {{ opacity: 1; }} 7.6%, 9% {{ opacity: 0; }} 9.1%, 10.5% {{ opacity: 1; }} 10.6%, 100% {{ opacity: 0; }} }}
  @keyframes appear  {{ 0%, 7% {{ opacity: 0; }} 8.2%, 100% {{ opacity: 1; }} }}
  @keyframes igniteFork {{ 0%, 31.5% {{ fill: {MUTED}; }} 33% {{ fill: {AMBER}; }} 100% {{ fill: {AMBER}; }} }}
  @keyframes beatA   {{ 0%, 12% {{ opacity: 0; }} 13.5% {{ opacity: 1; }} 27.5% {{ opacity: 1; }} 29%, 100% {{ opacity: 0; }} }}
  @keyframes beatB   {{ 0%, 33.5% {{ opacity: 0; }} 35% {{ opacity: 1; }} 55% {{ opacity: 1; }} 56.5%, 100% {{ opacity: 0; }} }}
  @keyframes beatC   {{ 0%, 59% {{ opacity: 0; }} 60.5% {{ opacity: 1; }} 78% {{ opacity: 1; }} 79.5%, 100% {{ opacity: 0; }} }}
  @keyframes fadeall {{ 0% {{ opacity: 0; }} 1% {{ opacity: 1; }} 96.5% {{ opacity: 1; }} 99.5%, 100% {{ opacity: 0; }} }}

  /* Highlight rows glow at 6%, never full amber-chrome (color stays reserved for divergence). */
  rect.hl.beatA {{ animation-name: hlA; }}
  rect.hl.beatC {{ animation-name: hlC; }}
  @keyframes hlA {{ 0%, 12% {{ opacity: 0; }} 13.5% {{ opacity: 0.06; }} 27.5% {{ opacity: 0.06; }} 29%, 100% {{ opacity: 0; }} }}
  @keyframes hlC {{ 0%, 59% {{ opacity: 0; }} 60.5% {{ opacity: 0.06; }} 78% {{ opacity: 0.06; }} 79.5%, 100% {{ opacity: 0; }} }}

  @media (prefers-reduced-motion: reduce) {{
    * {{ animation: none !important; }}
    .cmd {{ clip-path: none; }}
  }}
"""

    dots = "".join(
        f'<circle cx="{x}" cy="{TITLEBAR_H / 2}" r="5.5" fill="{HAIRLINE}"/>' for x in (22, 40, 58)
    )
    body = "\n".join(highlights + rows + captions)
    note = escape(FOOTER_NOTE)
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {height}" width="{W}" height="{height}" role="img" aria-label="amberfork demo: two agent runs aligned in a terminal; a rate-limit retry is absorbed as a log-move and the step where the failing run fetched a stale policy glows amber as the fork">
<style>{css}</style>
<rect x="0.5" y="0.5" width="{W - 1}" height="{height - 1}" rx="8" fill="{BG}" stroke="{HAIRLINE}"/>
{dots}
<text x="{W / 2}" y="{TITLEBAR_H / 2 + 4}" text-anchor="middle" class="title">amberfork demo</text>
<line x1="0" y1="{TITLEBAR_H}" x2="{W}" y2="{TITLEBAR_H}" stroke="{HAIRLINE}"/>
<g class="fadeall">
{body}
</g>
<line x1="0" y1="{caption_top}" x2="{W}" y2="{caption_top}" stroke="{HAIRLINE}"/>
<text x="{W - PAD_X}" y="{caption_top + 26}" text-anchor="end" class="note">{note}</text>
</svg>
"""


if __name__ == "__main__":
    OUT.write_text(build())
    print(f"wrote {OUT.relative_to(ROOT)} ({OUT.stat().st_size} bytes)")
