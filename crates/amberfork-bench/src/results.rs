//! The results document and its published rendering — one type, one renderer, two modes.
//!
//! `run` builds a [`BenchResults`] live and prints it through [`render`]; `report` loads a
//! committed copy ([`load`]) and prints it through the same function. That shared path is
//! the offline-reproduction guarantee (BENCHMARK.md's definition of done): the table a
//! reader re-renders from the committed JSON is byte-identical to the one the original run
//! published, because no second renderer exists to drift.

use crate::calibration::{CalibrationBin, N_BINS};
use crate::score::{ArmScore, Rate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// The document version this binary writes. Bumped whenever the shape changes, so a
/// renderer never draws a table from a document shaped by different rules.
pub const SCHEMA_VERSION: &str = "0.6";

/// The versions [`load`] vouches for. A 0.5 document is a 0.6 document with no `sources`
/// (and no per-record `source` tags) — the fields are additive and optional, so reading the
/// committed 0.5 artifacts stays legal without rewriting them. Rewriting is not an option:
/// the sealed test-split documents were produced once, at the v0.2.0 tag (protocol rule 2),
/// and must keep the exact bytes of that reveal.
const READABLE_VERSIONS: [&str; 2] = ["0.5", SCHEMA_VERSION];

/// The results document `run --json-out` writes and `report` renders. Versioned
/// independently of the trace schema so a committed copy stays readable as later slices
/// extend it. 0.2: added `split` (the selection scored), `coverage` (rule 4), and `pairs`
/// (the rule-1 split manifest); `n_pairs` narrowed from "pairs loaded" to "pairs scored".
/// 0.3: `params` gained its identity — `source` (the file as named on the command line) and
/// `sha256` of its exact bytes (rule 2). 0.4: confidence-bearing arms carry `calibration`,
/// the rule-7 reliability curve (fixed-width bins, exact-hit rate per bin). 0.5: `cross_system`
/// — how many scored pairs align against a different-agent-system reference (Mode A′); when it
/// is non-zero the protocol reads `mode-a-prime` and the table carries the cross-system
/// disclosure (issue #7). 0.6: `sources` — present exactly when the document is an exact
/// aggregate of other results documents (`aggregate` pools hits and n per metric and
/// recomputes the Wilson intervals); each pair and exclusion record then carries `source`
/// naming the document it came from (issue #14).
#[derive(Debug, Serialize, Deserialize)]
pub struct BenchResults {
    pub bench_schema_version: String,
    /// The evaluation protocol: `chimera` = controlled injection on real logs, `mode-a-prime`
    /// = natural cross-system run-vs-reference pairs (BENCHMARK.md).
    pub protocol: String,
    /// Which split selection produced the arm scores.
    pub split: String,
    pub coverage: Coverage,
    /// Pairs actually scored: evaluated ∩ selected split.
    pub n_pairs: usize,
    /// Of the scored pairs, how many are cross-system (Mode A′): the reference is a run of a
    /// different agent system, so it diverges from step 0 and step-exact gold is murky. Drives
    /// the table's cross-system disclosure; zero for a same-system chimera set.
    #[serde(default)]
    pub cross_system: usize,
    pub params: ParamsUsed,
    /// Non-empty exactly when this document is an aggregate: the results documents it pools,
    /// each with the sha256 of its exact bytes — so the aggregate names its inputs the same
    /// way the table names its params, and a reader can verify the pooling from the repo
    /// alone. Empty for a single `run`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SourceRecord>,
    /// The split manifest: every evaluated pair with its task key and assignment, whatever
    /// the selection — committed alongside results so the split is auditable (rule 1).
    pub pairs: Vec<PairRecord>,
    pub arms: Vec<ArmResult>,
}

/// One pooled input of an aggregate document: the file as named on the command line, the
/// sha256 of its exact bytes, and how many pairs it scored (its share of the pooled n).
#[derive(Debug, Serialize, Deserialize)]
pub struct SourceRecord {
    pub file: String,
    pub sha256: String,
    pub n_pairs: usize,
}

/// Rule 4's accounting: every manifest found is either evaluated (and split-assigned) or
/// excluded for a tabulated reason. `evaluated / total` is the coverage the table reports.
#[derive(Debug, Serialize, Deserialize)]
pub struct Coverage {
    pub total: usize,
    pub evaluated: usize,
    /// Evaluated pairs on each side of the split.
    pub dev: usize,
    pub test: usize,
    /// Exclusion counts by reason kind (empty when nothing was excluded).
    pub reasons: BTreeMap<String, usize>,
    /// Per-case records, in manifest order.
    pub exclusions: Vec<ExclusionRecord>,
}

/// One excluded case in the results document: dir-relative file, kebab-case reason. The
/// prose diagnostics stay on stderr — they may carry absolute paths and OS error text, which
/// have no business in a committed artifact.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExclusionRecord {
    pub name: String,
    pub reason: String,
    pub file: String,
    /// In an aggregate document, the results document this record came from — pair and
    /// exclusion names repeat across seed sets, so provenance must be explicit. Absent for
    /// a single `run`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// One line of the split manifest.
#[derive(Debug, Serialize, Deserialize)]
pub struct PairRecord {
    pub name: String,
    pub task_key: String,
    pub split: String,
    /// In an aggregate document, the results document this record came from (see
    /// [`ExclusionRecord::source`]). Absent for a single `run`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// The engine parameters every arm ran with, carrying their identity (protocol rule 2):
/// which file they came from and the sha256 of its exact bytes. The values are echoed too,
/// so a results document is readable without chasing the file.
#[derive(Debug, Serialize, Deserialize)]
pub struct ParamsUsed {
    pub source: String,
    pub sha256: String,
    pub tau: f64,
    pub resync_k: usize,
    pub gap_open: f64,
    pub gap_ext: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArmResult {
    pub arm: String,
    #[serde(flatten)]
    pub score: ArmScore,
    /// Rule 7's reliability curve — present exactly for the arms whose predictions carry a
    /// confidence ([`crate::arms::Arm::emits_confidence`]); a baseline has nothing to
    /// calibrate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calibration: Option<Vec<CalibrationBin>>,
}

/// Load a committed results document, vouching for its version before its shape: a foreign
/// `bench_schema_version` is named as such rather than surfacing as a field-level parse
/// error, because "this renderer does not speak that document" is the actual problem.
pub fn load(path: &Path) -> Result<BenchResults, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|err| format!("read results {}: {err}", path.display()))?;
    parse(&text, path)
}

/// [`load`] for text already in hand — the seam `aggregate` uses, because it needs the
/// document's exact bytes for the source sha256 as well as the parsed shape.
pub fn parse(text: &str, path: &Path) -> Result<BenchResults, String> {
    #[derive(Deserialize)]
    struct VersionOnly {
        bench_schema_version: String,
    }

    let version: VersionOnly = serde_json::from_str(text)
        .map_err(|err| format!("results document {}: {err}", path.display()))?;
    if !READABLE_VERSIONS.contains(&version.bench_schema_version.as_str()) {
        return Err(format!(
            "results document {}: bench_schema_version {} is not one this binary renders \
             ({}) — regenerate it with `run --json-out`",
            path.display(),
            version.bench_schema_version,
            READABLE_VERSIONS.join(", ")
        ));
    }
    serde_json::from_str(text).map_err(|err| format!("results document {}: {err}", path.display()))
}

/// The full published artifact: an optional aggregate disclosure, coverage line, params
/// line, an optional cross-system disclosure, the arms table, the calibration table — the
/// exact stdout of `run`, `aggregate`, and `report`. The disclosures appear only for the
/// documents they describe, so a single-run chimera table is byte-identical to what it was
/// before either seam existed.
#[must_use]
pub fn render(results: &BenchResults) -> String {
    let mut header = Vec::new();
    if let Some(line) = aggregate_line(results) {
        header.push(line);
    }
    header.push(coverage_line(results));
    header.push(params_line(results));
    if let Some(line) = cross_system_line(results) {
        header.push(line);
    }
    format!(
        "{}\n\n{}\n\n{}",
        header.join("\n"),
        markdown_table(results),
        calibration_table(results)
    )
}

/// The aggregate disclosure (issue #14), present only for a pooled document — and FIRST,
/// before any number: a reader must know the coverage below sums several runs before they
/// can read it. Pooling method stated inline (hits and n summed, intervals recomputed) so
/// the line cannot be mistaken for a mean of per-seed rates.
fn aggregate_line(results: &BenchResults) -> Option<String> {
    (!results.sources.is_empty()).then(|| {
        let parts: Vec<String> = results
            .sources
            .iter()
            .map(|source| format!("{} (n={})", source.file, source.n_pairs))
            .collect();
        format!(
            "aggregate of {} results documents (hits and n pooled, intervals recomputed): {}",
            results.sources.len(),
            parts.join(" · ")
        )
    })
}

/// The cross-system disclosure (issue #7), present only when the scored set contains Mode A′
/// pairs. Cross-system references come from a different agent system and legitimately diverge
/// from step 0, so the honest reading of the table below is the windowed one — BENCHMARK.md's
/// "report windowed metrics; do not overclaim step-exact", notebook 002's decision C.
fn cross_system_line(results: &BenchResults) -> Option<String> {
    (results.cross_system > 0).then(|| {
        format!(
            "cross-system: {}/{} scored pairs align a failing run against a reference from a \
             different agent system — cross-system references diverge from step 0, so ±1/±3 are \
             the metric of record and step-exact is not claimed.",
            results.cross_system, results.n_pairs
        )
    })
}

/// The coverage line the table is published under (rule 4: a rate without its denominator's
/// history is a lie). Exclusion reasons appear inline, alphabetically, only when present.
fn coverage_line(results: &BenchResults) -> String {
    let coverage = &results.coverage;
    let excluded = if coverage.reasons.is_empty() {
        String::new()
    } else {
        let reasons: Vec<String> = coverage
            .reasons
            .iter()
            .map(|(kind, count)| format!("{kind} {count}"))
            .collect();
        format!(" (excluded: {})", reasons.join(", "))
    };
    format!(
        "coverage: {}/{} pairs evaluated{excluded} · split={} (dev {}, test {}) · scored {}",
        coverage.evaluated,
        coverage.total,
        results.split,
        coverage.dev,
        coverage.test,
        results.n_pairs
    )
}

/// The config-identity line (rule 2: every published table names the config hash that
/// produced it). The 12-hex prefix reads like a git short hash; the results JSON carries
/// the full digest, and `shasum -a 256 <source>` verifies it.
fn params_line(results: &BenchResults) -> String {
    let params = &results.params;
    format!(
        "params: {} sha256:{} · tau {} · resync_k {} · gap {}+{}",
        params.source,
        &params.sha256[..12],
        params.tau,
        params.resync_k,
        params.gap_open,
        params.gap_ext
    )
}

/// The results as a markdown table (the shape BENCHMARK.md's published table takes):
/// `rate [ci_lo, ci_hi]` per windowed metric, two decimals, one row per arm.
fn markdown_table(results: &BenchResults) -> String {
    let mut lines = vec![
        "| arm | exact | ±1 | ±3 | no-pred | n |".to_string(),
        "|---|---|---|---|---|---|".to_string(),
    ];
    for arm in &results.arms {
        lines.push(format!(
            "| {} | {} | {} | {} | {:.2} | {} |",
            arm.arm,
            cell(arm.score.exact),
            cell(arm.score.w1),
            cell(arm.score.w3),
            arm.score.no_pred.rate,
            arm.score.exact.n,
        ));
    }
    lines.join("\n")
}

fn cell(rate: Rate) -> String {
    format!(
        "{:.2} [{:.2}, {:.2}]",
        rate.rate, rate.ci95_lo, rate.ci95_hi
    )
}

/// The reliability curve as a markdown table (rule 7): one row per confidence bin, one
/// column per confidence-bearing arm, `hits/n · rate [ci]` per occupied cell and `—` for an
/// empty bin — published, not dropped. The caption states the correctness metric and why
/// abstentions are absent, so the table stands alone when pasted.
fn calibration_table(results: &BenchResults) -> String {
    let curves: Vec<(&str, &Vec<CalibrationBin>)> = results
        .arms
        .iter()
        .filter_map(|arm| {
            arm.calibration
                .as_ref()
                .map(|bins| (arm.arm.as_str(), bins))
        })
        .collect();
    let mut lines = vec![
        "calibration: exact-hit rate by fork confidence (abstentions carry no confidence)"
            .to_string(),
        format!(
            "| confidence | {} |",
            curves
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
                .join(" | ")
        ),
        format!("|---{}|", "|---".repeat(curves.len())),
    ];
    for bin in 0..N_BINS {
        let (lo, hi) = curves
            .first()
            .map_or((0.0, 0.0), |(_, bins)| (bins[bin].lo, bins[bin].hi));
        // The last bin is closed so confidence 1.0 has a home; the label says so.
        let close = if bin == N_BINS - 1 { ']' } else { ')' };
        let cells: Vec<String> = curves
            .iter()
            .map(|(_, bins)| match bins[bin].rate {
                Some(rate) => format!("{}/{} · {}", rate.hits, rate.n, cell(rate)),
                None => "—".to_string(),
            })
            .collect();
        lines.push(format!(
            "| [{lo:.1}, {hi:.1}{close} | {} |",
            cells.join(" | ")
        ));
    }
    lines.join("\n")
}
