# spike/ — throwaway feasibility spike (2026-07-07)

**This is not product code.** It exists to answer notebook entry 001's three questions before
any Rust is written: (1) are Mode-A failing↔passing pairs constructible from published data,
(2) does move-typed DP alignment beat positional first-mismatch at localizing the annotated
decisive error step, (3) what do real logs look like. See `docs/notebook.md` for results and
`BENCHMARK.md` for the pre-registered protocol that governs the real benchmark.

Datasets are downloaded locally and are NOT committed (licensing: see BENCHMARK.md data section).

```
python3 spike/convert_whowhen.py --src <whowhen_dir> --out spike/data/canonical/
python3 spike/align_spike.py --pairs spike/data/pairs/ --out spike/out/
```
