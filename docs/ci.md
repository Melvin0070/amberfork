# amberfork in CI

amberfork is a gate. It reads two traces, decides whether the second diverged from the first,
and exits with a code your CI already understands. There is no threshold to tune and no
service to sign up for.

## The exit-code contract

`amberfork diff` follows `diff(1)`:

| exit | meaning | what CI should do |
|---|---|---|
| `0` | **converged** — the runs stayed aligned | pass |
| `1` | **forked** — a divergence was found and localized | fail the job, publish the report |
| `2` | **trouble** — unreadable, invalid, or over `--max-steps` | fail the job, but as an *infrastructure* error, not a regression |

Distinguishing `1` from `2` matters. `1` is a finding about your agent. `2` means amberfork could
not do its job — a malformed trace, a missing file, a run longer than `--max-steps` (default 2000).
Treating them alike turns a broken export into a phantom regression.

Verified behaviour, not documentation drift:

```console
$ amberfork diff run_a.json --against run_a.json ; echo $?
0
$ amberfork diff run_b.json --against run_a.json ; echo $?
1
$ amberfork diff missing.json --against run_a.json ; echo $?
2
```

## Why there is no `--gate` or `--threshold` flag

There is no `--gate` flag, and that one is a settled design position: a flag that re-exposes an
exit code the process already returns is surface area for nothing. `amberfork diff` **is** the
gate, and the table above is its whole contract.

`--threshold` is a different case, and this document used to overstate it. The argument here was
that you capture a reference with `record`, re-drive the candidate under `replay`, and therefore
any fork is a real regression with no tolerance to tune. **There is no user-facing `replay`
command.** `amberfork --help` lists `diff`, `demo`, `serve` and `record`; replay is internal
machinery reachable only through `diff --verify`, which needs two already-recorded cassettes. So
the tolerance-free path as written was not something you could actually run.

What is true today: you record two runs of the same agent, or you diff two traces you already
have, and `diff` reports whether they forked. Two runs of a non-deterministic agent can differ for
reasons nobody cares about, and **how often that produces a spurious fork is not yet measured** —
so a threshold is absent because no evidence justifies a particular one, not because divergence
has been ruled out. The measurement is pre-registered in
[`docs/notebook.md`](notebook.md) entry 082, with the decision rule fixed before any number exists.
See [`BENCHMARK.md`](../BENCHMARK.md) for what is and is not measured.

## `record` does NOT follow this contract

The surprising part, called out because it will bite you:

```sh
amberfork record --out run.json -- python agent.py
```

**`record` propagates the *agent's* exit code**, not a diff verdict. If your agent exits 3,
`record` exits 3. The cassette is still written when the agent fails — that is the point, since a
failing run is the one you want to diff. So never gate on `record`'s exit code expecting
diff(1) semantics; gate on the `diff` that follows it.

## GitHub Actions

Commit a known-good reference trace, produce a candidate in CI, gate on the diff, and publish the
fork view as an artifact when it fails.

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
          # Pin the version. The asset filename embeds it, so `releases/latest/download/...`
          # would 404 the moment a new version ships — and pinning is what you want in CI anyway.
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

      # Produce the candidate trace however your agent does it. Under `record`, remember that
      # this step's exit code is your AGENT's, not amberfork's.
      - name: Run the agent
        run: amberfork record --out candidate.json -- python agent.py

      - name: Gate on the fork
        id: gate
        run: |
          set +e
          amberfork diff candidate.json \
            --against traces/reference.json \
            --html fork.html --json > result.json
          code=$?
          set -e
          case $code in
            0) echo "converged" ;;
            1) echo "::error::agent trajectory forked — see the fork.html artifact" ;;
            2) echo "::error::amberfork could not read the traces (exit 2)" ;;
          esac
          exit $code

      - name: Publish the fork view
        if: failure()
        uses: actions/upload-artifact@v4
        with:
          name: amberfork-fork-view
          path: |
            fork.html
            result.json
```

Every release publishes a `.sha256` beside each artifact, so the install step verifies what it
downloaded rather than trusting the transport.

`--html` writes a self-contained static page — no JS, no wasm, no network — so the artifact opens
cold from the Actions UI. `--json` emits the versioned `DiffResult` if you would rather post your
own summary:

```sh
jq -r '.fork.index, .fork.confidence' result.json
```

## Known limitation

On two runs that share **no common prefix** — traces from different agents, or different
instrumentation of the same agent — amberfork currently reports a fork at step 0 with high
confidence rather than reporting that the runs are not comparable. Two runs of the same agent
captured under `record` share a prefix by construction, so this does not arise there. If you diff arbitrary trace files
from unrelated sources, treat a step-0 fork as a signal to check that the traces are comparable.

The measurement behind this, and the registered fix, are in
[`docs/notebook.md`](notebook.md) entries 070 and 071.
