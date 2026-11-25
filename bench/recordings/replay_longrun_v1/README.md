# replay_longrun_v1 — the recordings behind notebook 089 (issue #65)

70 cassettes recorded on 2026-11-25 through the shipped `amberfork record`, from
`spike/longrun_agent.py` over `spike/longrun_world.py`, driven by a local `qwen3:8b`. They are
committed so the result in `bench/results/replay_longrun_all.json` can be reproduced offline, with
no model and no key:

```sh
cargo build -q --release -p amberfork
python3 spike/score_replay_longrun.py --amberfork-bin target/release/amberfork --out /tmp/rescore.json
cmp /tmp/rescore.json bench/results/replay_longrun_all.json
```

## Layout

Seven task directories, each with:

- `ref_01..05` — five sessions of the unchanged agent (classes A, B and C).
- `changed_01..05` — one live session per reference tape, recorded with that tape's *middle*
  registered change active (class D).
- a `.summary.json` beside each cassette, written by the agent.

`manifest.json` is the recorder's log: 70 attempts, all first attempts, no exclusions.
`manifest.void-server-down.json` is the voided log notebook 088a describes — 105 attempts made
while no model server was running, kept unmodified.

## What these are and are not

- **Synthetic world, real model output.** The dataset the tools answer from is invented for this
  experiment (no GAIA, Who&When, TRAIL or HAL content), so nothing here needs redaction. It
  lives under `bench/recordings/`, not `bench/fixtures/`, because two tests treat every directory
  under `bench/fixtures/` as sanitized output of the Python pair pipeline, which this is not. The model's responses are real and
  verbatim.
- **Do not re-record into this directory.** The recorder writes to `bench/data/longrun/`. These
  files are the ones 089 was scored on; replacing them changes the result's identity.
- The agent and world scripts must stay byte-compatible with these tapes: a change to the system
  prompt, tool schemas, model options or a tool's output makes every replay diverge at exchange 0.

Read notebooks 088, 088a and 089 before quoting any number from this set.
