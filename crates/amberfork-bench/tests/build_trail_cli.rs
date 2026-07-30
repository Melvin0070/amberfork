//! End-to-end test for `amberfork-bench build-trail-pairs` (issue #41 S4c).
//!
//! The vertical slice, proven whole: a TRAIL failing trace (with its gold annotation file) and a
//! decrypted HAL reference dump, on the same GAIA task, go in; the natural pair set comes out and
//! flows through the same `run` seam `build-pairs`' Mode A′ set already does. A second TRAIL trace
//! on a task no HAL dump names anything for is a counted, named drop — never silently skipped.
//!
//! Inputs are hand-authored fiction (a 6×7/3×3 arithmetic task), written to a scratch tree under
//! `CARGO_TARGET_TMPDIR` — nothing benchmark-derived is committed (notebook 001/T30).

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

fn bench() -> Command {
    Command::cargo_bin("amberfork-bench").expect("amberfork-bench binary builds")
}

/// The committed frozen-params file, reached from the crate root (integration tests run with the
/// package as working directory, so the in-repo default `bench/params.toml` does not resolve here).
fn frozen_params() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/params.toml")
}

/// A clean scratch tree for this test under Cargo's per-crate temp dir — removed first so a rerun
/// never inherits a stale output set.
fn work_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("build_trail_pairs");
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent dir");
    }
    fs::write(path, contents).expect("write fixture file");
}

/// A TRAIL trace on GAIA task `gaia-abc`: a harness span carrying the task id in its structured
/// logs (the S4a join key), and one `LLM` child span (`step0001`) that the gold annotation below
/// locates its error at.
const TRACE_ABC: &str = r#"{
  "trace_id": "trace-gaia-0001",
  "spans": [
    {
      "span_id": "root0000",
      "span_name": "get_examples_to_answer",
      "timestamp": "2025-03-19T16:40:46.000000Z",
      "span_attributes": {},
      "logs": [ { "body": { "function.output": [ { "task_id": "gaia-abc" } ] } } ],
      "child_spans": [
        {
          "span_id": "step0001",
          "span_name": "LiteLLMModel.__call__",
          "timestamp": "2025-03-19T16:40:47.000000Z",
          "span_attributes": {
            "openinference.span.kind": "LLM",
            "input.mime_type": "text/plain",
            "input.value": "What is 6 times 7?",
            "output.mime_type": "text/plain",
            "output.value": "41"
          },
          "child_spans": []
        }
      ]
    }
  ]
}"#;

/// The gold annotation for `TRACE_ABC`: one decisive error located at `step0001`, resolving to
/// step index 1 (the root harness span is index 0).
const GOLD_ABC: &str = r#"{
  "trace_id": "trace-gaia-0001",
  "errors": [
    { "category": "Wrong Tool Use", "location": "step0001", "impact": "HIGH" }
  ]
}"#;

/// A second TRAIL trace, on GAIA task `gaia-xyz` — no HAL dump below names this task, so it must
/// be counted as an unpaired drop rather than silently dropped.
const TRACE_XYZ: &str = r#"{
  "trace_id": "trace-gaia-0002",
  "spans": [
    {
      "span_id": "root0000",
      "span_name": "get_examples_to_answer",
      "timestamp": "2025-03-19T16:41:00.000000Z",
      "span_attributes": {},
      "logs": [ { "body": { "function.output": [ { "task_id": "gaia-xyz" } ] } } ],
      "child_spans": [
        {
          "span_id": "step0001",
          "span_name": "LiteLLMModel.__call__",
          "timestamp": "2025-03-19T16:41:01.000000Z",
          "span_attributes": {
            "openinference.span.kind": "LLM",
            "input.value": "What is 3 times 3?",
            "output.value": "8"
          },
          "child_spans": []
        }
      ]
    }
  ]
}"#;

const GOLD_XYZ: &str = r#"{
  "trace_id": "trace-gaia-0002",
  "errors": [
    { "category": "Wrong Tool Use", "location": "step0001", "impact": "MEDIUM" }
  ]
}"#;

/// A decrypted HAL config dump (one backing model, `gpt-4o`) that passed GAIA task `gaia-abc` —
/// the reference side of the natural pair.
const HAL_DUMP_GPT4O: &str = r#"{
  "config": {
    "run_id": "gaia_hf_open_deep_research_gpt4o",
    "agent_args": { "model_name": "gpt-4o" }
  },
  "results": {
    "successful_tasks": ["gaia-abc"],
    "failed_tasks": []
  },
  "raw_logging_results": [
    {
      "id": "openai-1",
      "trace_id": "hal-trace-1",
      "started_at": "2025-04-16T23:46:16.000000+00:00",
      "ended_at": "2025-04-16T23:46:18.000000+00:00",
      "attributes": { "weave_task_id": "gaia-abc" },
      "summary": { "weave": { "trace_name": "openai.chat.completions.create" } },
      "inputs": { "model": "gpt-4o", "messages": [ { "role": "user", "content": "What is 6 times 7?" } ] },
      "output": { "choices": [ { "finish_reason": "stop", "index": 0, "message": { "role": "assistant", "content": "42" } } ] }
    }
  ]
}"#;

#[test]
fn build_trail_pairs_constructs_a_natural_set_that_flows_through_the_seam() {
    let work = work_dir();
    let traces = work.join("traces");
    let gold = work.join("gold");
    let hal = work.join("hal");
    let out = work.join("out");
    write(&traces.join("trace_0001.json"), TRACE_ABC);
    write(&gold.join("trace_0001.json"), GOLD_ABC);
    write(&traces.join("trace_0002.json"), TRACE_XYZ);
    write(&gold.join("trace_0002.json"), GOLD_XYZ);
    write(&hal.join("gpt4o.json"), HAL_DUMP_GPT4O);

    // Build: the gaia-abc trace pairs with the gpt-4o reference; the gaia-xyz trace has no
    // passing HAL reference and is a counted, named drop.
    bench()
        .arg("build-trail-pairs")
        .arg("--traces")
        .arg(&traces)
        .arg("--gold")
        .arg(&gold)
        .arg("--hal")
        .arg(&hal)
        .arg("--out")
        .arg(&out)
        .assert()
        .success()
        .stderr(predicate::str::contains("built 1 natural pair(s) -> "))
        .stderr(predicate::str::contains(
            "TRAIL traces: 2, 0 without a usable gold step; HAL dumps: 1, 1 runs read",
        ))
        .stderr(predicate::str::contains(
            "unpaired trace trace-gaia-0002: no passing HAL reference shares this trace's task_id",
        ));

    // The manifest carries the natural-pair contract: earliest resolved gold step, same-agent
    // (never cross-system), and the HAL model in provenance.
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(out.join("pair_00.json")).expect("pair_00.json written"))
            .expect("pair_00.json is valid JSON");
    assert_eq!(manifest["cross_system"], false);
    assert_eq!(manifest["gold_step"], 1);
    assert_eq!(manifest["failing"], "a_00.json");
    assert_eq!(manifest["reference"], "b_00.json");
    assert_eq!(manifest["meta"]["task_id"], "gaia-abc");
    assert_eq!(manifest["meta"]["hal_model"], "gpt-4o");
    assert!(
        out.join("a_00.json").is_file() && out.join("b_00.json").is_file(),
        "both run files are written beside the manifest"
    );

    // Score the generated set: a natural pair carries no cross-system flag, so the seam reads it
    // as the ordinary chimera protocol — no Mode A′ banner.
    let json_path = out.join("results.json");
    bench()
        .arg("run")
        .arg("--pairs")
        .arg(&out)
        .arg("--params")
        .arg(frozen_params())
        .arg("--json-out")
        .arg(&json_path)
        .assert()
        .success();

    let results: serde_json::Value =
        serde_json::from_slice(&fs::read(&json_path).expect("results.json written"))
            .expect("results.json is valid JSON");
    assert_eq!(results["protocol"], "chimera");
    assert_eq!(results["cross_system"], 0);
    assert_eq!(results["n_pairs"], 1);
}
