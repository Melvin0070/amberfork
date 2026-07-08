# amberfork: Market, Use Cases, User Stories, and Positioning

> Companion to `design-run-diff-debugger.md` (architecture) and `DESIGN.md` (visual system).
> This doc answers "who is this for, what is it good for, why would a strong engineer respect
> it, and how does it differ from what exists." It stays consistent with the reconciled Current
> State: local, all-Rust, hybrid passive+record, DOM/SVG, two equal belief pillars (reproducible
> offline eval with an honest defensible-asymmetry claim + explainable craft), full feature set
> kept.
>
> **Grounding.** Competitive facts here come from the project's research passes (memory:
> `amberfork-competitive-demand-research`, `amberfork-replay-benchmark-landscape`) and hands-on
> checks of LangSmith, Langfuse, Neatlogs, and Laminar. Where a claim is a judgment call, it
> says so. Numbers are directional, not a TAM exercise.

---

## 0. One-liner

**Point at a failing agent run. amberfork aligns it against a known-good run, ignites the exact
step where they diverged in amber, and tells you what changed. Local, all-Rust, deterministic,
no account.**

The mental model to teach: *observability shows you what one run did; amberfork shows you what
**changed** between two.*

---

## 1. The market

### The honest shape of it

Split the market into the **problem** and the **specific solution**, because demand is very
different for each.

- **The problem has heavy, proven, monetized demand.** Debugging non-deterministic agents,
  comparing runs, catching regressions is a real, funded market. ~89% of surveyed orgs have some
  agent observability; ~62% can inspect individual steps. LangSmith (commercial, LangChain) and
  Langfuse (open-source, acquired by ClickHouse Jan 2026) have serious adoption; Phoenix, Neatlogs,
  Laminar, Braintrust, Helicone all exist and have users. Teams pay real money to understand why
  their agents misbehave.
- **The specific solution (automated two-run semantic alignment + fork localization) has no
  proven standalone demand, because nobody has shipped it well.** The on-the-nose "diff two agent
  runs" framing has *flopped* as a pitch (the two literal "diff two runs" Show HNs sit at 1 point /
  0 comments; community replay OSS sits at 1-5 stars; the closest micro-competitor,
  `clay-good/agent-replay`, is a 5-star repo whose capability claims failed verification). The pain
  the tool addresses (localization, non-determinism, silent regressions) is among the *most
  validated* in the space; the *word* "diff" is builder vocabulary that has to be taught, not
  assumed.

**Net:** strong demand for the job, unproven demand for this exact tool, and the automated
fork-finding is a genuine capability gap in the tools people already pay for (they leave it manual).

### Where value pools, and who owns distribution

The adjacent lane is crowded and the distribution is owned by incumbents (LangSmith via the
LangChain ecosystem, Langfuse via open-source reach). The realistic competitive dynamic is not
"users adopt a separate local binary" but "an incumbent ships a compare-and-highlight-divergence
feature." That is an honest headwind for *adoption*, and it is exactly why this project's goal is
**respect, not adoption**. An empty corner in a crowded room is a great place to plant a flag for a
portfolio artifact and a risky place to build a business.

### What this means for amberfork's goal

amberfork is a **portfolio / craft artifact** whose stated goal is to earn strong-engineer respect;
organic usage is a welcome byproduct, not the bar. So the "market" that matters most is not a user
base, it is the **audience of strong engineers** who read the repo, the benchmark, and the writeup.
The tool must be genuinely useful (the use cases below are real), but its success is measured in
stars/shares/respect and in the quality of the engineering, not in a growth funnel.

**Realistic expectation:** a great version earns evaluator respect + a small handful of people who
run the demo, star it, and a few who feed it their own traces. Single digits to low double digits
of active users, and that is by design, not a failure.

---

## 2. Who it's for (personas)

### Primary: the Evaluator (the audience that defines success)

```
Who:       Senior/staff engineer (agent-infra or Rust systems), arriving from HN,
           a dev.to writeup, or a portfolio link.
Context:   Not debugging their own agent. Judging whether this project is technically
           impressive and worth respect / a star / a share, in 2-5 minutes.
Skeptical: ~6 shallow "git diff for agents" tools already exist; they've seen the pattern.
Converts:  when the benchmark reproduces in one clean offline command, the amber-fork GIF
           lands in one look, and the crate structure + code read as rigor, not slop.
Bounces:   when the number is asserted not backed, the GIF is buried, or the pitch is
           builder-vocab they've watched flop.
```

This persona does not need the tool. They need to believe the *work* is real. Everything in the
"impress factors" section is aimed here.

### Secondary: the Agent Builder (the person who'd actually use it)

```
Who:       AI engineer building a multi-tool agent (LangGraph / CrewAI / OpenAI Agents SDK)
           at a startup or on a team.
Context:   Their agent regressed, or fails intermittently, and they can't tell where or why.
           They have (or can capture) two runs of the same task, one good, one bad.
Tolerance: ~15-30 min; abandons if hello-world needs long docs or standing up infra.
Expects:   single binary, point at traces they already have, opinionated defaults, a fork
           they can see in one look and copy text out of.
```

This persona is real but secondary. The install path is kept warm (`amberfork demo`, `amberfork record`),
not optimized as the growth engine.

### Tertiary (occasional): the Team Lead / Reviewer

Someone who uses the amber-fork view as a **communication artifact** to show a teammate exactly
where two runs split, instead of narrating 80 log lines in Slack.

---

## 3. Use cases (the core)

Structure for each: **the situation → what they do today → what amberfork does → why it wins →
strength + honest constraint.** Rated by how solid the use case is for *this* tool.

### UC1 — Regression after a change  ★★★★★ (the strongest; build the spine here)

**Situation.** You bump the model (Sonnet to a new version to cut cost), or tweak the system
prompt, or update a tool's schema, or bump a dependency. A task that used to pass now fails, or
fails 30% of the time. You changed one thing; the agent's behavior changed somewhere non-obvious.

**Today.** You open both runs in LangSmith or Langfuse, get two step-trees side by side, and scroll
them by hand looking for where they split. On a 40-step multi-agent run, the visible failure is
often 15 steps *downstream* of the real cause, so you scroll, guess, and re-read. AGDebugger's CHI'25
study found devs read 50-100+ messages by hand for exactly this.

**What amberfork does.** `amberfork diff <bad> --against <good>`. It aligns the two trajectories with the
move-typed aligner, computes the first decisive divergence, and lights it in amber: *step 12, the
model called `search` with a truncated query; every downstream step cascaded from there.* The
field-level diff shows the exact argument that changed. Optionally, counterfactual re-execution
(record mode) confirms that fixing step 12 recovers the good outcome.

**Why it wins.** The pair comes *for free* (old version = good, new = bad), so there's no "go find
two comparable runs" friction. It maps to language engineers already use (regression, before/after,
CI). Twenty minutes of manual scrolling becomes five seconds of "the fork is here, this field
changed."

**Strength + constraint.** The killer case. Strongest because the reference is naturally available
and the framing is legible. Constraint: most valuable on long, branchy trajectories; on a 3-step
agent nobody needs a tool.

### UC2 — Flaky / non-deterministic failure  ★★★★☆

**Situation.** Same input. Passes on run A, fails on run B. No code changed. "Failed at step 4,
reran it, passed, no idea why." This is the canonical agent-debugging pain, and one of the most
validated complaints in the space (Ouyang TOSEM'24: 47-76% of agent runs non-reproducible).

**Today.** You rerun until you catch a failure, then eyeball the pass and the fail side by side, or
you give up and add a retry.

**What amberfork does.** Diff the passing run against the failing run. The fork shows the first
step where the two stochastic paths meaningfully diverged (for example: the model sampled a
different tool call, or a tool returned different data that sent the run down a bad branch).

**Why it wins.** Turns "reran it, dunno" into "they diverged at step 6, where the retrieval returned
2 fewer docs." It localizes the *branch point* of non-determinism, which no single-run viewer can do.

**Strength + constraint.** Very real, very common. Constraint: because the divergence is stochastic
rather than caused by a code change, the "fix" is often "add a guard / reduce variance" rather than
"revert a change." amberfork localizes; it doesn't always prescribe. And replay can reproduce the
recorded path but not re-derive the divergent one (stated openly: fork-finding is semantic, not
byte-exact).

### UC3 — Golden-trace regression testing / CI gate  ★★★★☆

**Situation.** You have a known-good "golden" run for a critical task. You want CI to catch when a
PR changes the agent's behavior on that task, and to tell you *where*.

**Today.** You assert on final output (brittle, misses mid-trajectory drift) or you eyeball traces
manually after the fact.

**What amberfork does.** `amberfork diff --gate <new> --against <golden>` runs in CI, exits non-zero when
a decisive divergence is attributed, and emits `--json` naming the fork step and the changed field.
A green badge means "behavior matches the reference trajectory," machine-checkable.

**Why it wins.** It's regression testing at the *trajectory* level, not just the output level, and it
lands on a known wound: the "golden-dataset maintenance tax" is one of the loudest upstream pains, so
supporting reference/consensus baselines instead of one brittle golden is directly on target.

**Strength + constraint.** Strong and legible (CI is a language everyone speaks). Constraint: needs a
success predicate (assertion / rubric / label) supplied by the user; OTel span status is not treated
as task success.

### UC4 — A/B comparison of two prompts or configs  ★★★☆☆

**Situation.** You have two prompt versions (or two model configs) for the same task. Evals tell you
A scores higher than B. They don't tell you *why the behavior differs*.

**What amberfork does.** Diff a representative A run against a B run. The fork + field diff show the
*behavioral* divergence (A took the reasoning path that called the calculator; B hallucinated the
number), which complements the eval score's "A > B" with "here's the mechanism."

**Why it wins.** Eval platforms give you the *what* (scores); amberfork gives you the *why* (the
decision point that produced the score gap).

**Strength + constraint.** Genuinely useful, but weaker than UC1-3: you have to pick "representative"
runs from a non-deterministic set, which is judgment, not a free pair. Best paired with an eval
harness, not standalone.

### UC5 — Production incident post-mortem  ★★★☆☆

**Situation.** A production agent failed on a customer task. You have the failing trace and a
successful trace of the same or a similar task.

**What amberfork does.** Diff the incident trace against the known-good trace to localize the
decisive step, then use the field diff and (if recorded) counterfactual re-run to confirm cause for
the write-up.

**Why it wins.** Turns an incident review from "read the whole trace and argue" into "here's the
fork, here's the changed field, here's the counterfactual that confirms it."

**Strength + constraint.** Real, but depends on having a comparable good trace on hand, and prod
traces often lack captured content (OTel content capture is opt-in). The tool degrades honestly to a
structural-only diff with a "run under `amberfork record` to capture arguments" nudge.

### UC6 — Explaining agent behavior to a teammate  ★★☆☆☆

**Situation.** You need to show a colleague *where* two runs diverged without narrating 80 log lines.

**What amberfork does.** The amber-fork view is a shareable, selectable, screenshot-able artifact:
"look, they were identical through step 11, then this happened."

**Strength + constraint.** A nice byproduct of the craft, not a reason anyone installs the tool.
Listed for completeness.

### Use-case summary

| # | Use case | Strength | Why | Reference pair source |
|---|----------|:--------:|-----|-----------------------|
| UC1 | Regression after a change | ★★★★★ | Pair is free; legible framing | old version vs new version |
| UC2 | Flaky / non-deterministic failure | ★★★★☆ | Localizes the branch point | a pass vs a fail |
| UC3 | Golden-trace CI gate | ★★★★☆ | Trajectory-level regression testing | golden vs candidate |
| UC4 | A/B prompt/config comparison | ★★★☆☆ | The "why" behind the eval score | representative A vs B |
| UC5 | Incident post-mortem | ★★★☆☆ | Localize + confirm cause | incident vs known-good |
| UC6 | Explaining behavior | ★★☆☆☆ | Communication artifact | any two runs |

**The zone where it's genuinely useful:** long, multi-step trajectories where the fork is upstream
of the visible failure and manual side-by-side reading is painful. That is the target case for the
demo, the benchmark fixtures, and the README examples.

---

## 4. User stories (the core)

Format: **As a [persona], I want [capability], so that [outcome]**, followed by a concrete narrative
so the value is visible, not abstract.

### US1 — The model-bump regression (UC1)

> **As** Priya, a senior AI engineer, **I want** to point at the run that broke after I upgraded the
> model **and** have the tool show me exactly which step and which argument changed versus the run
> that worked, **so that** I can fix the regression in minutes instead of scrolling two 40-step
> traces by hand.

**Narrative.** Priya swaps her agent's model to save 40% on cost. The nightly eval drops from 92% to
71% on the "refund-triage" task. She has last week's passing trace and today's failing trace. She
runs `amberfork diff today.otlp --against lastweek.otlp`. The amber fork lands on step 18: the new model
called `lookup_order` with the customer's *name* instead of the *order id*, because it summarized the
context differently three steps earlier. The field diff shows `order_id: "..." → name: "..."`. She
adds one line to the tool's arg description, re-runs, eval back to 91%. Total time: under ten minutes.
The old workflow was "reran it a few times, read a lot of logs, guessed."

### US2 — The flaky failure (UC2)

> **As** Dana, an agent builder, **I want** to diff a run that failed against a run that passed on the
> *same* input, **so that** I can see where the non-determinism actually branched instead of adding a
> blind retry.

**Narrative.** Dana's research agent fails ~1 in 4 runs with no code change. She captures one pass and
one fail. `amberfork diff fail.otlp --against pass.otlp` shows the two runs identical through step 9, then
forking at step 10: the failing run's retrieval returned 3 documents, the passing run's returned 5,
and the model, given fewer docs, skipped a verification tool. The fix isn't a retry; it's pinning the
retrieval `k` and adding a guard. She only knew *where* to look because the fork was localized.

### US3 — The CI regression gate (UC3)

> **As** Sam, a platform engineer, **I want** a CI check that fails when a PR changes my agent's
> behavior on a golden task **and** points at the diverging step, **so that** behavioral regressions
> get caught in review, not in production.

**Narrative.** Sam commits a golden trace for the "invoice-parse" task and adds
`amberfork diff --gate $CANDIDATE --against golden.otlp` to the GitHub Action. A teammate's PR tweaks a
prompt; CI goes red with `fork at step 7: tool 'extract_total' args changed (currency field dropped)`.
The teammate sees exactly what their prompt change did to behavior, fixes it, CI goes green. No one had
to read a trace by hand.

### US4 — The evaluator (primary persona)

> **As** Marcus, a skeptical staff engineer browsing HN, **I want** to verify in five minutes that
> this project is real work and not a wrapper, **so that** I decide whether to star it, share it, and
> respect the person who built it.

**Narrative.** Marcus lands on the repo. First screen: a one-liner and a GIF of an amber fork igniting
on a real divergent run in under 90 seconds. He gets it instantly. He scrolls to the benchmark table:
amberfork vs shallow-positional vs random on Who&When and TRAIL, with an honest "here's where it ties,
here's where it loses" row and a plain caveat that the two-run aligner is handed a known-good
reference the single-trajectory baselines were not. He runs `cargo run -p amberfork-bench`; the table
prints offline, no API key. He skims `crates/`: `align`, `attrib`, `bench`, clear names, tests
present. Five minutes in he stars it and posts "finally someone did the two-sequence alignment
properly, and it's honest about the privileged-reference caveat." He is the market.

### US5 — The A/B mechanism (UC4)

> **As** Raj, an ML engineer, **I want** to see *why* prompt B behaves differently from prompt A, not
> just that B scores lower, **so that** I can decide which behavior I actually want.

**Narrative.** Raj's eval says prompt A beats prompt B by 8 points. He diffs a B run against an A run.
They match until step 5, where A's phrasing led the model to call the calculator and B's led it to
estimate. Now Raj knows the score gap is "tool use vs guessing," not noise, and he ports A's phrasing
into B. The eval gave him the *what*; amberfork gave him the *why*.

### US6 — The incident review (UC5)

> **As** Lena, a tech lead, **I want** to localize and confirm the cause of a production agent failure
> against a known-good trace, **so that** my post-mortem states the root cause with evidence instead
> of a hypothesis.

**Narrative.** A customer's onboarding agent failed. Lena diffs the incident trace against a known-good
onboarding run. The fork is at step 22 (a tool timeout cascaded into a wrong branch). Because the good
run was recorded under `amberfork record`, she re-executes the sub-trajectory from step 22 with the timeout
removed; the run recovers. Her post-mortem says "confirmed cause: step 22 timeout, verified by
counterfactual re-run," not "we think it was the timeout."

### US7 — The honest bounce (anti-story, kept for calibration)

> **As** an engineer with a 4-step linear agent, **I want** to know quickly that I don't need this
> tool, **so that** I don't install something that solves a problem I don't have.

**Narrative.** The README's "where it fails" paragraph and the target-case framing tell him plainly:
if your runs are short enough to eyeball side by side, use your eyes. He respects the honesty and moves
on. That honesty is part of why US4's Marcus trusted the benchmark.

---

## 5. Impress factors (why strong engineers respect it)

Aimed at the primary persona. Each is a thing the evaluator *sees* and reads as depth.

1. **A clever algorithm applied where no one has applied it.** Move-typed affine-gap Needleman-Wunsch
   over two agent trajectories, with a typed move alphabet (sync / substitute-args / substitute-action
   / insert / delete / reorder). The prior-art sweep found *no one* uses classic two-sequence DP
   alignment with semantically-typed moves as the localization mechanism for agent runs. It's a real
   algorithm from bioinformatics, ported to an open problem, not a JSON pretty-printer.

2. **The rare algorithm-AND-design combination.** Most engineers can implement a clever algorithm *or*
   design a genuinely good interface. This does both: the aligner is real, and the amber-fork UI
   (DESIGN.md's "sameness recedes, divergence glows") is a memorable, screenshot-able identity almost
   no debugger has. That pairing is the specific thing this audience prizes.

3. **The hard part is the cost model, and it's visible.** The moat isn't "I ran NW." It's designing the
   move-type costs and the step-similarity predicate so they behave over *noisy, non-deterministic* LLM
   steps. That's genuine, iterative engineering, and the code shows it.

4. **Local, offline, deterministic, no key: an asymmetry, not a benchmark brag.** The headline claim is
   "localizes the decisive step as well as an LLM judge, but locally, explainably, deterministically,
   and reproducibly without a network or key," stated *with* the honest privileged-reference caveat.
   That verifiable asymmetry is more impressive to a skeptic than a contestable "beats SOTA," and it
   doubles as the answer to "why not just use an LLM?"

5. **Reproducible in one offline command.** `cargo run -p amberfork-bench` prints the scoring table with no
   API key, no network, cross-platform in CI with a green badge. The skeptic can *verify*, which is the
   difference between "impressive" and "asserted."

6. **Honesty as a feature.** A designed converged/empty state, a "where it fails" paragraph, a stated
   threat-to-validity on the paired protocol, and a "content: limited" degraded mode. Engineers trust
   the tool *more* because it admits its edges. Slop oversells; craft discloses.

7. **Systems craft.** All-Rust, single self-contained binary, DOM/SVG so text stays selectable and
   accessible (wgpu was deliberately dropped for exactly this reason), a clean 14-crate workspace with
   a frozen result schema as the seam. It reads as someone who cares about the whole thing, not the
   demo path.

8. **A real architectural insight.** The hybrid passive+record execution model comes from a genuine
   observation: counterfactual attribution is *impossible* against passive telemetry alone (a trace is
   a photo, not the agent), so causal claims require owning execution. Naming that and designing for it
   is the kind of thinking evaluators respect.

---

## 6. How it differs and how it's similar

### The comparison at a glance

| Tool | Category | Local / offline | Two-run **semantic** alignment | **Automated** fork localization | Deterministic (non-LLM) core | Open source | Primary audience |
|------|----------|:---:|:---:|:---:|:---:|:---:|------|
| **amberfork** | run-diff debugger | **yes** | **yes** | **yes** | **yes** | **yes** | strong engineers / agent builders |
| LangSmith | observability + eval | no (cloud) | no (side-by-side, manual) | no | no (LLM assist: Polly) | no | LangChain teams |
| Langfuse | observability + eval | yes (self-host) | no (trace tree; manual causal) | no | no | yes | LLM eng teams |
| Arize Phoenix | observability + eval | yes (local-first) | no | no | no | yes | LLM eng teams |
| Neatlogs | collab debugging | no (cloud; SDK MIT) | no | no (LLM "investigate") | no | partial | teams shipping agents |
| Laminar (lmnr) | agent obs + replay | no (cloud/OSS) | no | no | no | Apache | agent builders (funded) |
| StepFinder / AgenTracer / CausalFlow / WebStep | research | n/a | some (single-traj or run-vs-ref) | paper-level | mixed | mostly unreleased | researchers |
| vcrpy / pytest-recording | replay cassette | yes | no | no | yes (byte-level) | yes | test engineers |
| difftastic / delta | code/text diff | yes | n/a (design **reference**, not a competitor) | n/a | yes | yes | all devs |

### How it's SIMILAR (the shared ground)

- **Same problem space as the observability tools:** debugging agents, inspecting step-by-step
  trajectories, catching regressions. amberfork reads the same OTel/OpenInference traces they do.
- **Same "compare runs" intent as LangSmith/Langfuse:** the desire to look at two runs and understand
  the difference. They ship a *side-by-side* version of this.
- **Same diffing UX lineage as difftastic/delta:** structural, legible, "you see the change." That's a
  deliberate design reference, not a competitor.

### How it DIFFERS (the wedge)

1. **Automated semantic alignment vs manual eyeballing.** LangSmith and Langfuse show you two trees and
   leave *you* to find the divergence (Langfuse's own docs: "multi-step causal analysis across agent
   turns is manual"). amberfork *computes* the alignment and the fork. That is the core capability none
   of them have.
2. **Deterministic + explainable vs LLM-in-the-loop.** Neatlogs' "ask it to investigate" and LangSmith's
   Polly hand the trace to an LLM and get a probabilistic guess. amberfork's core is a deterministic
   algorithm you can reproduce and explain step by step. (An optional local judge does *semantic naming
   only*, never localization.)
3. **Local / offline / no account vs cloud SDK-to-dashboard.** Neatlogs, LangSmith, and Laminar send your
   traces to a hosted service. amberfork is a single local binary; nothing leaves your machine.
4. **Two-run fork attribution vs single-run inspection or metric dashboards.** Everyone else does
   single-run trace views (plus "compare experiments" dashboards that are metric/score-level, not
   trajectory alignment). The run-vs-run *watershed* is the thing.
5. **Craft artifact vs product/SaaS.** The others optimize adoption funnels; amberfork optimizes
   legibility, reproducibility, and taste for an audience that reads code and UI, not pricing pages.

### The sharpest objection, and the answer

> "Isn't this just LangSmith's compare view plus Polly? Or just asking an LLM to diff two traces?"

**No, and here's the one-liner:** LangSmith shows you two trees and an LLM's *guess*; amberfork
*computes* the divergence with a deterministic, explainable aligner, locally, with no key, and can
*confirm* the cause by counterfactual re-execution. The LLM version is non-deterministic, cloud-bound,
and unverifiable; amberfork is reproducible and legible. And the automated fork is a real capability the
LLM-diff and the side-by-side view both leave as manual work. (Honest caveat kept: on very short runs,
"just read them" wins, and that's in the README.)

### Closest analogs, named

- **Closest *shipping* thing:** LangSmith / Langfuse "compare runs." Same intent, manual execution, no
  automated alignment, cloud or metric-level.
- **Closest *conceptual* work:** the research line (WebStep's "bifurcation," AgenTracer's success-vs-
  failure alignment). Same idea of a divergence watershed, but these are papers, mostly with unreleased
  datasets, not local tools, and the standalone paired benchmark is thin and scoopable (do not lean on a
  "first benchmark" claim).
- **Design reference (not a competitor):** difftastic / delta. Structural diff done with taste, for code.

---

## 7. Honest limitations (this is also an impress factor)

- **You need a comparable pair.** UC1-3 hand you one for free; UC4-5 require judgment to pick
  representative runs. "Diff two runs" is worthless if you can't get two runs.
- **Short trajectories don't need it.** The value is upstream-of-visible-failure localization on long,
  branchy runs.
- **Replay can't re-derive the divergent path.** Response-cache replay reproduces the recorded path
  only; fork-finding is semantic/state-based, not byte-exact. Counterfactual re-execution needs record
  mode (owning execution).
- **The benchmark is contestable no matter how honest.** The two-run aligner is handed a known-good
  reference the single-trajectory baselines weren't, so the claim is an *asymmetry*, not a clean SOTA
  win. Stated openly.
- **Standalone adoption will be a trickle.** Distribution belongs to incumbents; usage is a byproduct,
  not the goal. This doc treats that as a fact, not a problem to paper over.

---

## 8. Positioning statements (reusable one-liners)

- **README hero:** "Point at a failing agent run. See exactly where it diverged from a known-good run,
  and what changed. Local, deterministic, no account."
- **Mental-model teacher:** "Observability shows you what happened. amberfork shows you what *changed*."
- **The intuition pump (use once, not as the headline):** "Like `git bisect`, but for agent runs."
- **The depth claim (for the writeup):** "A move-typed sequence aligner that localizes the decisive step
  in a regression as well as an LLM judge, but locally, explainably, and reproducibly without a key."
- **The honesty line:** "It tells you where two runs forked and why, and it tells you where it can't."
