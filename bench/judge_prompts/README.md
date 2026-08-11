# Frozen LLM-judge baseline prompts (issue #46)

These three templates are the LLM-judge baseline arms' entire instruction surface. They are
**frozen** under BENCHMARK.md rules 2 and 10, the same discipline `bench/params.toml` lives
under: every published judge number names the sha256 of the exact bytes below, so any edit — a
word, a newline, this sentence's neighbours in a template — is a new revision that must be
reported alongside the old number (rule 3), never swapped in.

Registered in `docs/notebook.md` entry 069, **before** any judge code or number existed.

| File | sha256 |
|---|---|
| `judge_single.md` | `e622edfd84bc2e15974b9e2ac94474fe047f41385a2d4bef64732fafaaec6e61` |
| `judge_paired.md` | `ce7515e888ffde2c54e61b0d5fcd90a9c27c29776532bdb4479e2fb1d1e9d942` |
| `judge_stepwise.md` | `d0c13e46c41e225510f0c6c23d1dfba9bdbd95530758a1ef2c83cc8a44bbc209` |

Verify: `shasum -a 256 bench/judge_prompts/*.md`. This README is **not** hashed — it documents the
contract, it is not part of it.

## Why the prompts live here and not in the crate

`amberfork-judge`'s `Judge` trait cannot serve as this baseline. It is a *narration* interface by
construction: `Explanation::fork_index` is set by the caller from the `DiffResult`, and
`prompt::build` explicitly forbids the model from naming a step index, so a narration judge
structurally *cannot* mis-localize — which is exactly the guardrail that makes it useless as a
localizer. The baseline needs the opposite: a model that reports a step index, which is then
scored against gold with no grounding guard in the way. Slice A2 builds that as a separate
interface; these prompts are its frozen input, and living in `bench/` keeps them where the
protocol can hash them rather than where a refactor can quietly reword them.

## Placeholders

Substitution is literal text replacement, no escaping, no template engine.

| Token | Meaning |
|---|---|
| `{{FAILING_STEPS}}` | The failing run, rendered per the step contract below |
| `{{REFERENCE_STEPS}}` | The reference (passing) run, same rendering |
| `{{PREFIX_STEPS}}` | The failing run's steps `0..=CANDIDATE_INDEX`, same rendering |
| `{{FAILING_LAST_INDEX}}` | `failing.steps.len() - 1`, decimal |
| `{{CANDIDATE_INDEX}}` | The step under test in `judge_stepwise`, decimal |

## Step rendering contract

One step per line, `\n`-joined, in run order:

```
#<idx> [<kind> · <name>] inputs: <preview> | outputs: <preview>
```

- `<idx>` is the 0-based index into *that run's* `steps`. Indices are per-run; the paired prompt
  says so in words because the two runs are numbered independently.
- `<kind>` is the `StepKind` debug name; `<name>` the step's `name` verbatim.
- An absent payload renders as `(none)`.
- **Payload cap: 600 characters per field**, as head 400 + tail 200 joined by
  `…[N chars elided]…` where `N` is the exact count removed. Head-and-tail rather than head-only
  because a tool result's error or answer usually sits at its end, and the call's shape at its
  start. Measured on the corpus (notebook 069): the cap holds the largest `judge-paired` prompt
  to ~64k tokens, so no provider silently truncates a prompt out from under the protocol.
- Character counts are `char`s (Unicode scalar values), not bytes — the same unit
  `amberfork-judge`'s existing preview cap uses.

## Answer parsing

- Take the **last** `{...}` JSON object in the response; parse it; read `step` (single, paired) or
  `decisive` (stepwise).
- `step` must be an integer in `0..failing.steps.len()`. Out of range, wrong type, absent, or no
  parseable JSON object at all → **parse failure**.
- A parse failure is **scored as a miss**, not an exclusion, and is counted and reported
  separately. It is a property of the arm — a judge that cannot follow its own output contract is
  worse at the task, not un-evaluable.
- A *transport* failure (network, 5xx, rate limit) is retried up to 3 times with backoff; if it
  still fails the pair becomes an exclusion for that arm under rule 4, tabulated with its reason.
  Transport ≠ parse: one is our infrastructure failing, the other is the method failing.
- `judge_stepwise` runs candidates `0, 1, 2, …` and stops at the first `true`, which is the
  prediction. All-`false` over the whole run is **no prediction**, scored as a miss.

## Cassettes

Cassette key = sha256 of canonical JSON over `{provider, model, arm, prompt_sha256, rendered_prompt_sha256, temperature, max_output_tokens}`.

The cassette file stores that key, the model id, the arm, the response text, and the run date —
and **never the prompt text**. That is not a size optimisation: TRAIL prompts embed verbatim GAIA
questions, which BENCHMARK.md's data rules say must never land in this repo. Hashing the rendered
prompt keeps the cache exact while keeping the questions out.

CI stays network-free: cassette-only is the default, and a live call requires both an explicit
flag and an API key in the environment.
