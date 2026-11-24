//! End-to-end contract of `amberfork diff <bad> --against <good>` (issue #4, slice 1).
//!
//! These tests run the real binary against the committed smoke fixtures, which makes them the
//! first place `amberfork-ingest` and `amberfork-align` are exercised together. The contract
//! under test:
//! - exit codes follow the `diff(1)` precedent — 0 converged, 1 forked, 2 trouble;
//! - `--json` emits a deserializable [`amberfork_model::DiffResult`] on stdout (the machine
//!   contract), locating the fork at the fixture manifest's gold step;
//! - errors go to stderr, never stdout, and name the offending path.

use amberfork_model::DiffResult;
use assert_cmd::Command;
use std::path::{Path, PathBuf};

/// Exit codes under test, named once (see `diff(1)`: 0 same, 1 differ, 2 trouble).
const EXIT_CONVERGED: i32 = 0;
const EXIT_FORKED: i32 = 1;
const EXIT_TROUBLE: i32 = 2;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spike/fixtures/smoke")
}

fn amberfork() -> Command {
    Command::cargo_bin("amberfork").expect("amberfork binary builds")
}

/// The committed manifest: which fixture is the failing side and where the fork truly is.
fn manifest() -> (PathBuf, PathBuf, usize) {
    let dir = fixture_dir();
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("pair_smoke.json")).unwrap())
            .unwrap();
    let bad = dir.join(manifest["failing"].as_str().unwrap());
    let good = dir.join(manifest["reference"].as_str().unwrap());
    let gold = manifest["gold_step"].as_u64().expect("gold_step") as usize;
    (bad, good, gold)
}

#[test]
fn forked_pair_exits_1_and_json_locates_the_gold_step() {
    let (bad, good, gold) = manifest();

    let assert = amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--json")
        .assert()
        .code(EXIT_FORKED);

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let result: DiffResult =
        serde_json::from_str(&stdout).expect("--json stdout must be a valid DiffResult");

    let fork = result.fork.expect("smoke pair encodes a real divergence");
    assert_eq!(
        fork.b_step,
        Some(gold),
        "fork must land on failing-run step {gold} (manifest gold)"
    );
    // Side convention of the contract: <good> is reference = a, <bad> is observed = b.
    assert_eq!(
        result.runs.b.n_steps,
        result
            .alignment
            .iter()
            .filter(|m| m.b_idx.is_some())
            .count()
    );
    // The content-diff pane's data (issue #13): the fork pair carries field-level evidence.
    assert!(
        result.field_diffs.iter().any(|fd| fd.step == fork.index),
        "--json must carry field diffs at the fork's alignment index"
    );
}

#[test]
fn self_diff_exits_0_and_json_reports_converged() {
    let (bad, _, _) = manifest();

    let assert = amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&bad)
        .arg("--json")
        .assert()
        .code(EXIT_CONVERGED);

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let result: DiffResult = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        result.fork, None,
        "a run against itself is the converged state"
    );
}

#[test]
fn jsonl_input_exits_2_and_stderr_names_the_shape_the_file_and_the_guide() {
    // The likeliest first mistake (issue #20): pointing `diff` at a raw exporter transcript
    // (JSONL) instead of a converted canonical trace. The first error message is the product
    // surface for exactly that user — it must say which file, what shape it detected, and
    // where the conversion guide lives, all without dirtying stdout.
    let (_, good, _) = manifest();
    let jsonl = Path::new(env!("CARGO_TARGET_TMPDIR")).join("raw_transcript.jsonl");
    std::fs::write(
        &jsonl,
        "{\"role\": \"assistant\", \"content\": \"hi\"}\n{\"role\": \"tool\", \"name\": \"web.search\"}\n",
    )
    .unwrap();

    amberfork()
        .arg("diff")
        .arg(&jsonl)
        .arg("--against")
        .arg(&good)
        .assert()
        .code(EXIT_TROUBLE)
        .stdout(predicates::str::is_empty())
        .stderr(predicates::str::contains("raw_transcript.jsonl"))
        .stderr(predicates::str::contains("JSON-Lines"))
        .stderr(predicates::str::contains("docs/run-on-your-own-agent.md"));
}

/// Step count of the longer smoke-fixture side — the number the size-guard tests pivot on.
fn longest_side() -> usize {
    let (bad, good, _) = manifest();
    [bad, good]
        .iter()
        .map(|path| {
            let run: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            run["steps"].as_array().expect("canonical trace").len()
        })
        .max()
        .expect("two sides")
}

#[test]
fn lowered_max_steps_exits_2_and_stderr_names_the_escape_hatch() {
    let (bad, good, _) = manifest();

    amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--max-steps")
        .arg((longest_side() - 1).to_string())
        .assert()
        .code(EXIT_TROUBLE)
        .stdout(predicates::str::is_empty())
        .stderr(predicates::str::contains("alignment guard"))
        .stderr(predicates::str::contains("--max-steps"));
}

#[test]
fn max_steps_exactly_at_the_trace_length_is_honored() {
    // The override controls the ceiling: at the longer side's exact length the same pair
    // aligns normally (the smoke pair forks, so exit 1 — not the guard's exit 2).
    let (bad, good, _) = manifest();

    amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--max-steps")
        .arg(longest_side().to_string())
        .assert()
        .code(EXIT_FORKED);
}

#[test]
fn missing_file_exits_2_with_the_path_on_stderr_and_clean_stdout() {
    let (_, good, _) = manifest();

    amberfork()
        .arg("diff")
        .arg("no/such/trace.json")
        .arg("--against")
        .arg(&good)
        .assert()
        .code(EXIT_TROUBLE)
        .stdout(predicates::str::is_empty())
        .stderr(predicates::str::contains("no/such/trace.json"));
}

// --- `diff --verify` wiring (issue #37, slice 4b) -----------------------------------------------
//
// The re-execution mechanism itself is verified offline with stubs at the `amberfork-attrib`
// layer; these lock the CLI's own contract — flag validation and cassette-only input — and stay
// hermetic by failing before any network is touched. The counterfactual `--json` block is locked
// by `amberfork-model`'s round-trip test, which `diff --json` serializes through unchanged.

#[test]
fn verify_companion_flags_without_verify_exit_2_and_name_the_gate() {
    // A companion flag on its own is a mistake worth catching loudly: it only means something with
    // --verify, and silently ignoring it would hide that the re-execution never ran.
    let (bad, good, _) = manifest();

    amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--upstream")
        .arg("http://127.0.0.1:0")
        .assert()
        .code(EXIT_TROUBLE)
        .stdout(predicates::str::is_empty())
        .stderr(predicates::str::contains("--verify"));
}

#[test]
fn verify_without_upstream_exits_2_and_names_the_missing_flag() {
    let (bad, good, _) = manifest();

    amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--verify")
        .arg("--base-url-env")
        .arg("OPENAI_BASE_URL")
        .arg("--")
        .arg("true")
        .assert()
        .code(EXIT_TROUBLE)
        .stdout(predicates::str::is_empty())
        .stderr(predicates::str::contains("--upstream"));
}

#[test]
fn verify_on_canonical_traces_exits_2_and_asks_for_a_cassette() {
    // --verify replays recorded exchanges, so both runs must be cassettes; the smoke fixtures are
    // canonical traces. This fails at load, before the runtime or any network — so it is hermetic.
    let (bad, good, _) = manifest();

    amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--verify")
        .arg("--upstream")
        .arg("http://127.0.0.1:0")
        .arg("--base-url-env")
        .arg("OPENAI_BASE_URL")
        .arg("--")
        .arg("true")
        .assert()
        .code(EXIT_TROUBLE)
        .stdout(predicates::str::is_empty())
        .stderr(predicates::str::contains("cassette"));
}

#[test]
fn verify_runs_below_one_is_rejected_by_the_parser() {
    // The consensus is a majority over N re-runs; N = 0 has no majority to take, so the parser
    // refuses it up front rather than letting it degrade to a silent `Unverified`.
    let (bad, good, _) = manifest();

    amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--verify")
        .arg("--runs")
        .arg("0")
        .arg("--upstream")
        .arg("http://127.0.0.1:0")
        .arg("--base-url-env")
        .arg("OPENAI_BASE_URL")
        .arg("--")
        .arg("true")
        .assert()
        .code(EXIT_TROUBLE)
        .stderr(predicates::str::contains("invalid value"));
}

/// One Claude Code transcript record per element, joined into the `.jsonl` a session writes.
fn claude_code_session(session_id: &str, policy_doc: &str, verdict: &str, action: &str) -> String {
    let records = [
        serde_json::json!({"type": "mode", "sessionId": session_id, "mode": "default"}),
        serde_json::json!({"type": "user", "sessionId": session_id, "isSidechain": false,
            "message": {"role": "user", "content": "Handle the refund for order 8841"}}),
        serde_json::json!({"type": "assistant", "sessionId": session_id, "isSidechain": false,
            "message": {"role": "assistant", "model": "claude-opus-5-5", "content": [
                {"type": "text", "text": "I'll look up the refund policy first."}]}}),
        serde_json::json!({"type": "assistant", "sessionId": session_id, "isSidechain": false,
            "message": {"role": "assistant", "model": "claude-opus-5-5", "content": [
                {"type": "tool_use", "id": "toolu_01", "name": "Read",
                 "input": {"file_path": "policy/refunds.md"}}]}}),
        serde_json::json!({"type": "user", "sessionId": session_id, "isSidechain": false,
            "message": {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "toolu_01", "content": policy_doc}]}}),
        serde_json::json!({"type": "assistant", "sessionId": session_id, "isSidechain": false,
            "message": {"role": "assistant", "model": "claude-opus-5-5", "content": [
                {"type": "text", "text": verdict}]}}),
        serde_json::json!({"type": "assistant", "sessionId": session_id, "isSidechain": false,
            "message": {"role": "assistant", "model": "claude-opus-5-5", "content": [
                {"type": "tool_use", "id": "toolu_02", "name": "Bash",
                 "input": {"command": action}}]}}),
        serde_json::json!({"type": "user", "sessionId": session_id, "isSidechain": false,
            "message": {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "toolu_02", "content": "ok"}]}}),
    ];
    records.iter().map(|record| format!("{record}\n")).collect()
}

#[test]
fn two_claude_code_sessions_diff_with_no_conversion_step() {
    // The zero-setup path: the transcripts already on a developer's disk are valid inputs as
    // they are. Steps: 0 user · 1 assistant · 2 Read · 3 assistant · 4 Bash — the stale policy
    // doc lands in step 2's output, and nothing after it re-syncs.
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("good-session.jsonl");
    let bad = dir.path().join("bad-session.jsonl");
    std::fs::write(
        &good,
        claude_code_session(
            "good",
            "Refunds v3 (CURRENT). Electronics: 30-day return window, refund to original payment.",
            "Order is 23 days old, inside the 30-day window. Issuing the refund.",
            "payments refund --order 8841 --amount 129.00",
        ),
    )
    .unwrap();
    std::fs::write(
        &bad,
        claude_code_session(
            "bad",
            "Refunds v1 (ARCHIVED). Electronics: 14 days from delivery, store credit only.",
            "Past the 14-day limit, so policy says no. Drafting a polite denial.",
            "crm set-status --order 8841 --status denied",
        ),
    )
    .unwrap();

    let assert = amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--json")
        .assert()
        .code(EXIT_FORKED);
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let result: DiffResult = serde_json::from_str(&stdout).expect("a valid DiffResult");
    assert_eq!(
        result.fork_step_observed(),
        Some(2),
        "the fork is the Read that returned the stale policy"
    );

    amberfork()
        .arg("diff")
        .arg(&good)
        .arg("--against")
        .arg(&good)
        .assert()
        .code(EXIT_CONVERGED);
}

/// An ATIF `trajectory.json` (Harbor / Terminal-Bench) for the same refund task.
fn atif_trajectory(session_id: &str, policy_doc: &str, verdict: &str, action: &str) -> String {
    serde_json::json!({
        "schema_version": "ATIF-v1.8",
        "session_id": session_id,
        "agent": {"name": "terminus", "version": "2.0.0", "model_name": "openai/gpt-5.6-sol"},
        "steps": [
            {"step_id": 1, "source": "user", "message": "Handle the refund for order 8841"},
            {"step_id": 2, "source": "agent", "message": "I'll look up the refund policy first.",
             "tool_calls": [{"tool_call_id": "c1", "function_name": "read_file",
                             "arguments": {"path": "policy/refunds.md"}}],
             "observation": {"results": [{"source_call_id": "c1", "content": policy_doc}]}},
            {"step_id": 3, "source": "agent", "message": verdict,
             "tool_calls": [{"tool_call_id": "c2", "function_name": "bash",
                             "arguments": {"cmd": action}}],
             "observation": {"results": [{"source_call_id": "c2", "content": "ok"}]}},
        ]
    })
    .to_string()
}

#[test]
fn two_atif_trajectories_diff_with_no_conversion_step() {
    // Canonical steps: 0 user · 1 assistant · 2 read_file · 3 assistant · 4 bash. The stale
    // policy lands in step 2's output — inside ATIF step 2, which the fan-out makes addressable.
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("good").join("trajectory.json");
    let bad = dir.path().join("bad").join("trajectory.json");
    std::fs::create_dir_all(good.parent().unwrap()).unwrap();
    std::fs::create_dir_all(bad.parent().unwrap()).unwrap();
    std::fs::write(
        &good,
        atif_trajectory(
            "good",
            "Refunds v3 (CURRENT). Electronics: 30-day return window, refund to original payment.",
            "Order is 23 days old, inside the 30-day window. Issuing the refund.",
            "payments refund --order 8841 --amount 129.00",
        ),
    )
    .unwrap();
    std::fs::write(
        &bad,
        atif_trajectory(
            "bad",
            "Refunds v1 (ARCHIVED). Electronics: 14 days from delivery, store credit only.",
            "Past the 14-day limit, so policy says no. Drafting a polite denial.",
            "crm set-status --order 8841 --status denied",
        ),
    )
    .unwrap();

    let assert = amberfork()
        .arg("diff")
        .arg(&bad)
        .arg("--against")
        .arg(&good)
        .arg("--json")
        .assert()
        .code(EXIT_FORKED);
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let result: DiffResult = serde_json::from_str(&stdout).expect("a valid DiffResult");
    assert_eq!(result.fork_step_observed(), Some(2));

    amberfork()
        .arg("diff")
        .arg(&good)
        .arg("--against")
        .arg(&good)
        .assert()
        .code(EXIT_CONVERGED);
}
