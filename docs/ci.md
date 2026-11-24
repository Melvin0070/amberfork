# amberfork in CI

amberfork gives CI two gates, and they answer different questions:

| gate | question | needs |
|---|---|---|
| `amberfork replay` | Is my agent still doing what it did when I recorded it? | one committed recording; no network, no API key |
| `amberfork diff` | Did these two traces I already have diverge, and where? | two trace files |

Use `replay` to gate changes to your agent. Use `diff` when the two runs already exist.

## The exit-code contract

Both follow `diff(1)`:

| exit | `replay` | `diff` | what CI should do |
|---|---|---|---|
| `0` | the agent followed the tape | the runs converged | pass |
| `1` | the agent diverged from the tape | a fork was found and localized | fail the job |
| `2` | trouble: not a cassette, agent not runnable | trouble: unreadable, invalid, or over `--max-steps` | fail the job as an *infrastructure* error |

Distinguishing `1` from `2` matters. `1` is a finding about your agent. `2` means amberfork could
not do its job. Treating them alike turns a broken export into a phantom regression.

## Why the gate is record-then-replay, not two live runs

The obvious gate — run the agent, diff it against a reference run — does not work, and that is a
measurement, not a caution. On 273 pairs of real agent runs where **both runs passed the task**
(same agent, same model, same task; 9 scaffolds), `amberfork diff` reported a fork on **96.7%**
[93.9%, 98.3%] ([notebook 084](notebook.md)). Two independent executions of a real agent diverge
almost immediately whether or not anything is wrong, so a diff between them cannot tell you that
something is.

`replay` removes the independence. It answers each of the agent's provider calls from the
recording, so the new run *is* the recorded run until the agent itself sends a request the
recording never saw. That first off-tape request is the change.

```console
$ amberfork replay reference.cassette.json --base-url-env AGENT_BASE_URL -- python agent.py
amberfork: replay followed the tape — 2 of 2 recorded exchange(s) served, agent exited 0
$ echo $?
0
$ AGENT_SYSTEM_PROMPT="Be strict." \
  amberfork replay reference.cassette.json --base-url-env AGENT_BASE_URL -- python agent.py
amberfork: replay diverged at exchange 0 — POST /v1/chat is not on the tape (0 of 2 recorded exchange(s) served, agent exited 1)
$ echo $?
1
```

Both transcripts are real: the agent and recording are
[`docs/examples/ci-gate/`](examples/ci-gate/), and this repository's own
[`ci-gate example` workflow](../.github/workflows/ci-gate-example.yml) runs them on every push.

### What a replay gate does and does not check

- **It checks everything on your side of the provider boundary.** A changed prompt, a tool that
  returns something different, a code path that makes one call more or one fewer — each changes a
  request, and the first changed request ends the replay with exit 1. An agent that stops short of
  the recording also exits 1.
- **It does not check the model.** Responses come from the tape, so the model is never called. A
  model upgrade with unchanged prompts replays to exit 0. To see what a different model does, add
  `--upstream` (below) and point the agent at it — that is a live run and is no longer a gate.
- **Requests must be reproducible.** A prompt that embeds the current time or a random id differs
  on every run and will diverge at once. Tool-call ids, which SDKs mint fresh per run, are
  normalized for you; anything else non-deterministic in a request is yours to pin. How often a
  real, unchanged agent replays cleanly has **not yet been measured** — treat a divergence at
  exchange 0 on an agent you did not change as a reproducibility problem first.
- **The agent's exit code is reported, not returned.** A recording of a failing run replays to
  the same failure, and that is "followed the tape" (exit 0).

### Seeing what the changed agent goes on to do

Strict replay stops at the first off-tape request. To let the run continue against the real
provider and inspect the result, give it an upstream and an output file, then diff:

```sh
amberfork replay reference.cassette.json --base-url-env OPENAI_BASE_URL \
    --upstream https://api.openai.com --out candidate.cassette.json -- python agent.py
amberfork diff candidate.cassette.json --against reference.cassette.json --html fork.html
```

The two cassettes share their prefix exactly — the tape answered it — so the fork `diff` reports
is where the agent's behaviour changed, not where sampling happened to differ. This needs the
network and the agent's API key.

## Recording the reference

```sh
amberfork record --upstream https://api.openai.com --base-url-env OPENAI_BASE_URL \
    --out reference.cassette.json -- python agent.py
```

**`record` does not follow the contract above.** It is a transparent wrapper: it propagates the
*agent's* exit code, and writes the cassette even when the agent fails. Never gate on `record`'s
exit code expecting diff(1) semantics.

A cassette holds full request and response bodies. Read
[the privacy contract](cassette-format.md#privacy-contract) before committing one.

## GitHub Actions

Commit the reference recording, replay the agent against it on every pull request.

```yaml
name: agent regression gate
on: [pull_request]

jobs:
  amberfork:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install amberfork
        env:
          # Pin the version: the asset filename embeds it, and pinning is what you want in CI.
          # `replay` ships in the release after v1.0.0 — build from source until it is tagged.
          AMBERFORK_VERSION: v1.0.0
          TARGET: x86_64-unknown-linux-gnu
        run: |
          base="https://github.com/Melvin0070/amberfork/releases/download/${AMBERFORK_VERSION}"
          asset="amberfork-${AMBERFORK_VERSION}-${TARGET}.tar.gz"
          curl -sSL -O "${base}/${asset}"
          curl -sSL -O "${base}/${asset}.sha256"
          sha256sum -c "${asset}.sha256"
          tar xzf "${asset}"
          sudo install -m755 amberfork /usr/local/bin/amberfork
          amberfork --version

      # Offline: no API key, no provider. Exit 1 fails the job where the agent diverged.
      - name: Replay the agent against the reference recording
        run: |
          amberfork replay traces/reference.cassette.json \
            --base-url-env OPENAI_BASE_URL -- python agent.py
```

Every release publishes a `.sha256` beside each artifact, so the install step verifies what it
downloaded rather than trusting the transport.

### Gating on two traces you already have

When both runs exist as files — exported traces, two cassettes, two Claude Code sessions —
`diff` is the gate, and `--html` gives CI a self-contained report (no JS, no wasm, no network):

```sh
amberfork diff candidate.json --against reference.json --html fork.html --json > result.json
jq -r '.fork.index, .fork.confidence' result.json
```

Read the 96.7% above before gating on this with two *independently executed* runs of a real agent.

## Why there is no `--gate` or `--threshold` flag

There is no `--gate` flag: a flag that re-exposes an exit code the process already returns is
surface area for nothing. There is no `--threshold` either. Under `replay` there is nothing to
tolerate — a request is on the tape or it is not. Under `diff` on two live runs, the measurement
above says no threshold would rescue the comparison: both classes fork at nearly the same rate.

## Known limitation

On two runs that share **no common prefix** — traces from different agents, or different
instrumentation of the same agent — amberfork currently reports a fork at step 0 with high
confidence rather than reporting that the runs are not comparable. A recording and its `replay`
share a prefix by construction, so this does not arise there. If you diff arbitrary trace files
from unrelated sources, treat a step-0 fork as a signal to check that the traces are comparable.

The measurement behind this, and the registered fix, are in
[`docs/notebook.md`](notebook.md) entries 070 and 071.
