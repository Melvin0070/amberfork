# Design System — amberfork

> Source of truth for all visual and UI decisions. Created by /design-consultation, 2026-06-30.
> Grounded in research (Linear, Vercel, Raycast, difftastic, Warp, Railway) and the locked
> architecture in `design-run-diff-debugger.md`.

## North star

**"Sameness recedes. Divergence glows."** Two agent runs are compared; exactly one thing
matters — where they forked. Everything the runs agree on is muted gray context. Only the
fork and its divergent path downstream carry saturated color. You don't read the diff; you
see where it broke. Every decision below serves this one principle.

Memorable-thing: *"I instantly saw what changed — the fork was the only thing glowing."*

## Product Context
- **What this is:** A local, all-Rust developer tool that diffs two AI-agent run trajectories, finds the fork point, and attributes the regression.
- **Who it's for:** Strong engineers debugging non-deterministic agents.
- **Space/industry:** Developer tools / AI observability + debugging. Peers: Linear, Vercel, Raycast, difftastic, Warp, Sentry/Datadog trace views.
- **Project type:** Local dev tool with an embedded web app UI (Leptos, DOM + SVG). Data-dense instrument, not a marketing site.

## Aesthetic Direction
- **Direction:** Industrial / utilitarian × brutally minimal — an instrument panel for agent runs.
- **Decoration level:** minimal → intentional (hairline borders, subtle dotted-grid canvas, NO gradients, NO drop shadows).
- **Mood:** Precise, calm, dark, engineer-made. Quiet until the fork.
- **Reference sites:** linear.app, vercel.com, difftastic.wilfred.me.uk, warp.dev (avoid Railway's gradient-heavy treatment).

## Typography
Mono-forward. All faces are free (OFL) so distribution stays a clean single binary.
- **Display/Hero:** Monaspace Xenon (slab mono) — distinctive, engineer-coded headings.
- **Body/UI prose:** General Sans — clean grotesk; mono is tiring for longer prose.
- **UI/Labels:** Monaspace Neon (short labels) / General Sans (sentences).
- **Data/Tables/Steps:** Monaspace Neon — MUST use tabular-nums for timing/cost/confidence/tokens.
- **Code/diff:** Monaspace Neon.
- **Loading:** Self-host via @fontsource-style local files (offline single binary; no CDN at runtime). Fallbacks: `ui-monospace, monospace` and `ui-sans-serif, system-ui`.
- **Scale (rem, 16px base):** caption 0.75 · small 0.8125 · body 0.875 · ui 0.9375 · h3 1.125 · h2 1.5 · h1 1.875. Tight tracking on display (-0.02em).

## Color
- **Approach:** restrained. Neutrals carry everything; ONE saturated accent reserved for divergence.
- **Divergence accent (the ONLY scarce color):** `#FF7A1A` electric amber — the fork node and the divergent path downstream. Never used for chrome, buttons-at-rest, or decoration.
- **Diff semantics (ONLY inside the content-diff pane):** removed `#FF5C5C` (bg `#FF5C5C14`) · added `#46D39A` (bg `#46D39A14`). Kept visually distinct from the amber fork accent on purpose.
- **Neutrals (dark, default):** bg `#0A0A0B` · surface `#121214` · raised `#17171B` · hairline `#26262B` · text `#E6E6E9` · muted `#8A8A93` · faint `#55555C`.
- **Semantic (sparing):** success `#46D39A` · error `#FF5C5C` · warning `#F5A623` · info `#5AA2FF`.
- **Light mode:** redesign surfaces (bg `#F7F7F5`, surface `#FFFFFF`, hairline `#E2E2DD`, text `#16161A`, muted `#6B6B72`); reduce accent saturation (`#E0570B`), diff to `#C7382F` / `#1E9E6A`. Dark is the primary/default theme.

## Spacing
- **Base unit:** 4px.
- **Density:** compact (data instrument).
- **Scale:** 2xs(2) xs(4) sm(8) md(12) lg(16) xl(24) 2xl(32) 3xl(48).

## Layout
- **Approach:** grid-disciplined.
- **Core screen (fork-diff):** 3 panes — left rail (run selection: A=good / B=bad, corpus), center DAG canvas, right attribution + content-diff pane; time-travel scrubber docked at the bottom.
- **Synchronized spine:** the two run-DAGs are laid out on a SHARED vertical timeline so identical steps sit at the same y; a divergence visibly breaks the alignment. Layout is computed server-side in Rust.
- **Grid:** rail 188px · attribution 320px · DAG canvas flexes. Dotted-grid background on the canvas (22px).
- **Max content width:** app is full-bleed; marketing/docs pages cap at ~1180px.
- **Border radius:** tight/precise — sm 4px · md 6px · lg 8px · pill 999px (badges only). No uniform bubble-radius.

## Motion
- **Approach:** minimal-functional, with exactly ONE expressive beat.
- **The one beat:** on load and on scrub, the amber IGNITES at the fork and flows down the divergent path. Nothing else animates expressively.
- **Easing:** enter ease-out · exit ease-in · move ease-in-out.
- **Duration:** micro 50-100ms · short 150-250ms · medium 250-400ms (fork ignition) · long avoided.

## Rendering constraints (hard requirements)
- DOM + SVG only for the DAG and content (NO canvas/wgpu): text must be selectable, copyable, and accessible (screen-reader). This is why wgpu was dropped during eng review.
- All numeric columns: tabular-nums.
- Color is a signal, not decoration: never syntax-rainbow the whole trace — that buries the fork.
- Divergence is NEVER signaled by color alone: the fork carries a redundant non-color cue (a `⑂ FORK` label + a distinct stroke/line style) so the signal survives grayscale and red-green colorblindness (~8% of the target male-engineer users). "Color is a signal" means "not ONLY color." (plan-design-review 2026-07-05)
- The divergent path uses ONE uniform amber (fork + every downstream divergent step); no dimmer-amber propagation tint — one scarce accent, no "dimmer = less important" ambiguity.

## Anti-slop (never do, specific to this tool)
- No purple/violet gradients; no gradients as default accent (the Railway trap).
- No drop-shadow elevation — use hairline borders + lightness steps.
- Don't color everything; saturation is reserved for divergence.
- Don't center the DAGs; anchor them on the shared left timeline spine.
- No Inter / Roboto / Space Grotesk / system-ui as primary type.
- No bubble-radius; keep radii tight.

## Terminal rendering (first-class surface)

The CLI's human-readable output is a peer of the web UI, not a fallback — it is the v1 hero
surface (README GIF, CI logs, SSH sessions). Same north star: sameness recedes, divergence glows.

- **Sync steps:** dim gray, one summary line each. The eye skates over them.
- **The fork:** `⑂ FORK` gutter marker + amber. Amber = truecolor `#FF7A1A` where supported,
  ANSI-256 color `208` fallback, bold as last resort. The `⑂` glyph and gutter markers carry the
  signal without color — `NO_COLOR`, `--no-color`, and piped (non-TTY) output stay fully legible.
  Structure, not color, is the contract (same rule as DR2).
- **Divergent path:** uniform amber step markers downstream of the fork (DR4: one amber, no
  intensity grading).
- **Field-level diff (only inside the fork block):** `-`/`+` lines in red/green — same containment
  rule as the content-diff pane; red/green never appear anywhere else in terminal output.
- **Layout:** side-by-side A|B columns at ≥120 cols; unified single column with `A`/`B` gutter
  tags below that. Hard-wrap long content; never rely on horizontal scroll. Step indices, timings,
  costs: tabular-aligned.
- **Machine output:** non-TTY drops ANSI but keeps identical structure; `--json` (the `DiffResult`
  schema) is the machine contract, never parsed styled text.

Example (unified layout, grayscale-safe):

```
  step 09  ·  llm    planner        "summarize findings"             [sync]
  step 10  ·  tool   web.search     q="Q2 refunds policy"            [sync]
⑂ step 11  ✗  tool   lookup_order   A: order_id="8841"               [FORK · conf 0.86]
                                    B: name="J. Smith"
             - order_id: "8841"
             + name: "J. Smith"
  step 12  ✗  llm    planner        paths diverge downstream         [model-move]
```

## Single-run view (gateway, not a viewer product)

`amberfork open <run>` renders ONE run on the same spine — gray, no diff panes, no amber. Its only
job is killing the dead-end first-contact state ("need a second run to diff") so the tool is
worth opening before the day a regression hands you a pair. Scope-capped by design: no search, no
filtering, no analytics — the diff view minus run B, nothing more. (Single-run *viewing* is
commoditized; this is a gateway, not a viewer product.)

## Decisions Log
| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-06-30 | Initial design system created | /design-consultation; research-grounded; serves "sameness recedes, divergence glows" |
| 2026-06-30 | DOM/SVG over wgpu | Debugger content is text that must be selectable/accessible (eng-review Issue 7) |
| 2026-06-30 | Amber `#FF7A1A` as the sole divergence accent | Reads as "here's where it broke" without error-red panic; distinct from diff red/green |
| 2026-06-30 | Monaspace (Xenon/Neon) + General Sans | Free/OFL, engineer-coded, mono-forward; prose stays readable via General Sans |
| 2026-07-05 | No-divergence "converged" state | An amber-fork tool needs a designed answer when nothing forks: gray spine + "identical through N steps" + confidence (plan-design-review DR1) |
| 2026-07-05 | Fork redundancy beyond color | Colorblind + grayscale safety: `⑂ FORK` label + distinct stroke/line, not amber alone (plan-design-review DR2) |
| 2026-07-05 | Uniform amber divergent path | Fork + all downstream divergent steps share one amber; dropped the dimmer-propagation tint (plan-design-review DR4) |
| 2026-07-05 | Attribution reading order | fork → move-typed alignment → field diff → confidence → counterfactual → cause (plan-design-review DR5, approved via mockup) |
| 2026-07-05 | New components: move-typed chips, confidence meter, counterfactual row | Ratified into the system; full spec deferred to T40 (plan-design-review) |
| 2026-07-07 | Terminal render is a first-class surface | v1 hero (GIF/CI/SSH); de-risks the Leptos bet; resolves Phase-1 GIF depending on Phase-2 UI (panel review) |
| 2026-07-07 | Single-run view, scope-capped | Kills the dead-end one-trace empty state; a gateway to the diff, not a viewer product (panel review) |
