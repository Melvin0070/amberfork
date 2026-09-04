# the credibility release

amberfork aligns two AI-agent run traces — one that worked, one that didn't — with a real
sequence-alignment algorithm, localizes the exact step they forked at, and (on a run you captured
yourself) can work toward confirming that step caused the failure by replaying it with the fork
patched. Local, deterministic, offline. No account, no API key required for the core path.

Every milestone before this one built the tool. This one tested it — specifically, tested the
claims the tool makes about itself, against real data, published whichever way the numbers fell.
If you're deciding whether to trust this project, [`docs/writeup.md`](docs/writeup.md) is the
one page to read; this is the shorter version.

## What's new since v0.9.1

**The missing baseline finally exists, and it wasn't kind.** The project's own headline number
(91% fork localization within 3 steps, on controlled/injected forks) had never been checked against
the obvious question: *does a frontier LLM just asked to find the bug do better?* It does — three of
five judge arms beat the engine on real natural-failure pairs, with intervals that exclude zero.
Published as measured, not softened. The mitigating half of that same result: no arm, including the
judges, clears random guessing by much either — which points at the corpus more than any one method.

**A fourth attempt at natural-fork evidence, built from scratch this time.** Every earlier attempt
at finding a natural (non-injected) fork ran into a defect in someone else's dataset. This one
removes that excuse: a real local model drives a real tool-using agent through this project's own
unmodified recording path, with a known, engineered cause. Result: a tie with the simplest possible
baseline — and a real, useful diagnosis of *why*, not a shrug.

**`--verify`'s other half of the pitch, measured for the first time.** Confirming a fork's cause by
counterfactual replay had a single hand-written test and no rate. Now it has one: 87% confirmation
across 15 real, independently-forked runs, zero indeterminate verdicts. The two cases that didn't
confirm are the most interesting data in the table — read the writeup for why.

**Two documentation-drift issues closed with the reasoning kept, not just the fix** — including a
deliberate decision *not* to build a metric that looked easy but would have quietly compared amberfork
against a number it wasn't actually measuring.

**`--judge local` now defaults to a model that can actually narrate a fork** (the old default was
too small to produce useful output). Five release targets, a real CI gate recipe, repo metadata.

## What this release does not claim

- The 91% localization number is on **controlled, injected** forks — real logs, a real spliced
  divergence, honestly labeled as constructed. It is not a claim about natural failures.
- On the one natural corpus with an LLM-judge comparison, **amberfork loses to the judges** on the
  metric that matters. Read [`docs/writeup.md`](docs/writeup.md) for the full, unflattering table.
- `--verify`'s 87% confirmation rate has no independently-known ground truth behind it — it's a
  measure of how often the mechanism reaches a decisive verdict on real sampling-variance forks, not
  a precision score against a known cause. Why that number doesn't exist yet is in the writeup too.
- On two runs that share no common vocabulary, amberfork today reports a fork with **high confidence
  while being wrong**. The fix is designed, not shipped — the single highest-priority item on the
  open roadmap.

## Try it

```sh
cargo install amberfork
amberfork serve --demo
```

Point it at your own traces from there: [the trace format](docs/trace-format.md),
[a worked example](docs/run-on-your-own-agent.md). Every number in this release reproduces
offline, from a committed file, with no API key — commands for each are in
[`docs/writeup.md`](docs/writeup.md) and [`README.md`](README.md).

Full technical detail, every dead end included: [`docs/notebook.md`](docs/notebook.md), now 80
dated entries. Full change list: [`CHANGELOG.md`](CHANGELOG.md).
