//! Exact pooling of results documents (issue #14): the cross-seed table the README claims,
//! computed from the committed per-seed documents rather than asserted in prose.
//!
//! The pooling is EXACT, not approximate: every published rate carries its `hits` and `n`
//! ([`crate::score::Rate`]), so the aggregate of k documents is `sum(hits) / sum(n)` per
//! metric per arm — the number a single run over the union of the pair sets would have
//! produced — with the Wilson interval recomputed at the pooled n (rule 6). Calibration
//! bins pool the same way; bin edges are committed code constants, identical across
//! documents by construction. Nothing is averaged, weighted, or dropped: coverage sums,
//! exclusions concatenate (rule 4), and the split manifest concatenates with each record
//! tagged by its source document (rule 1 — pair names repeat across seed sets, so
//! provenance must be explicit).
//!
//! What refuses to pool: fewer than two documents (a one-document "aggregate" is a
//! mislabeled copy), the same document twice (double-counting), a document that is itself
//! an aggregate (sources-of-sources hides the real inputs), and any mismatch in protocol,
//! split, params identity, or arm set (a pooled number over mixed configurations would be
//! exactly the dishonesty this module exists to close).

use crate::results::{
    ArmResult, BenchResults, Coverage, ExclusionRecord, PairRecord, SourceRecord,
};
use crate::score::{ArmScore, Rate, wilson95};
use std::collections::BTreeMap;

/// One input to [`aggregate`]: a parsed results document plus the identity `aggregate`
/// records for it — the file as named on the command line and the sha256 of its exact bytes.
pub struct SourceDoc {
    pub file: String,
    pub sha256: String,
    pub results: BenchResults,
}

/// Pool `sources` into one results document. See the module docs for what pools and what
/// refuses; errors name the offending file(s) and the mismatched values.
pub fn aggregate(sources: Vec<SourceDoc>) -> Result<BenchResults, String> {
    check_poolable(&sources)?;

    let first = &sources[0].results;
    let arms = (0..first.arms.len())
        .map(|i| pool_arm(&sources, i))
        .collect();

    Ok(BenchResults {
        bench_schema_version: crate::results::SCHEMA_VERSION.to_string(),
        protocol: first.protocol.clone(),
        split: first.split.clone(),
        coverage: pool_coverage(&sources),
        n_pairs: sources.iter().map(|s| s.results.n_pairs).sum(),
        cross_system: sources.iter().map(|s| s.results.cross_system).sum(),
        params: clone_params(first),
        sources: sources
            .iter()
            .map(|s| SourceRecord {
                file: s.file.clone(),
                sha256: s.sha256.clone(),
                n_pairs: s.results.n_pairs,
            })
            .collect(),
        pairs: sources
            .iter()
            .flat_map(|s| {
                s.results.pairs.iter().map(|pair| PairRecord {
                    name: pair.name.clone(),
                    task_key: pair.task_key.clone(),
                    split: pair.split.clone(),
                    source: Some(s.file.clone()),
                })
            })
            .collect(),
        arms,
    })
}

/// Every refusal in one place, in the order a reader would want them reported: shape
/// problems (too few, duplicates, nested aggregates) before comparability problems
/// (protocol, split, params, arms).
fn check_poolable(sources: &[SourceDoc]) -> Result<(), String> {
    if sources.len() < 2 {
        return Err(format!(
            "aggregation pools at least two results documents; got {}",
            sources.len()
        ));
    }
    for (i, source) in sources.iter().enumerate() {
        if let Some(dup) = sources[..i].iter().find(|s| s.sha256 == source.sha256) {
            return Err(format!(
                "{} and {} are the same document (sha256 {}) — pooling it twice would \
                 double-count every pair",
                dup.file, source.file, source.sha256
            ));
        }
        if !source.results.sources.is_empty() {
            return Err(format!(
                "{} is itself an aggregate of {} documents — pool the original run \
                 documents instead, so the sources list names the real inputs",
                source.file,
                source.results.sources.len()
            ));
        }
    }

    let first = &sources[0];
    for source in &sources[1..] {
        check_comparable(first, source)?;
    }
    Ok(())
}

fn check_comparable(first: &SourceDoc, other: &SourceDoc) -> Result<(), String> {
    let (a, b) = (&first.results, &other.results);
    if a.protocol != b.protocol {
        return Err(mismatch(first, other, "protocol", &a.protocol, &b.protocol));
    }
    if a.split != b.split {
        return Err(mismatch(first, other, "split", &a.split, &b.split));
    }
    // Params identity is the sha256 of the frozen file's exact bytes (rule 2); the echoed
    // values are derived from those bytes, so the hash comparison covers them.
    if a.params.sha256 != b.params.sha256 {
        return Err(mismatch(
            first,
            other,
            "params sha256",
            &a.params.sha256,
            &b.params.sha256,
        ));
    }
    if a.arms.len() != b.arms.len() {
        return Err(mismatch(
            first,
            other,
            "arm count",
            &a.arms.len().to_string(),
            &b.arms.len().to_string(),
        ));
    }
    for (arm_a, arm_b) in a.arms.iter().zip(&b.arms) {
        if arm_a.arm != arm_b.arm {
            return Err(mismatch(first, other, "arm", &arm_a.arm, &arm_b.arm));
        }
        if arm_a.calibration.is_some() != arm_b.calibration.is_some() {
            return Err(format!(
                "cannot pool {} with {}: arm {} carries a calibration curve in one document \
                 and not the other",
                first.file, other.file, arm_a.arm
            ));
        }
    }
    Ok(())
}

fn mismatch(first: &SourceDoc, other: &SourceDoc, what: &str, a: &str, b: &str) -> String {
    format!(
        "cannot pool {} with {}: {what} {a} vs {b} — a pooled table over mixed \
         configurations would misrepresent both",
        first.file, other.file
    )
}

fn pool_coverage(sources: &[SourceDoc]) -> Coverage {
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for source in sources {
        for (kind, count) in &source.results.coverage.reasons {
            *reasons.entry(kind.clone()).or_default() += count;
        }
    }
    Coverage {
        total: sources.iter().map(|s| s.results.coverage.total).sum(),
        evaluated: sources.iter().map(|s| s.results.coverage.evaluated).sum(),
        dev: sources.iter().map(|s| s.results.coverage.dev).sum(),
        test: sources.iter().map(|s| s.results.coverage.test).sum(),
        reasons,
        exclusions: sources
            .iter()
            .flat_map(|s| {
                s.results
                    .coverage
                    .exclusions
                    .iter()
                    .map(|excl| ExclusionRecord {
                        name: excl.name.clone(),
                        reason: excl.reason.clone(),
                        file: excl.file.clone(),
                        source: Some(s.file.clone()),
                    })
            })
            .collect(),
    }
}

/// Pool arm `i` across all documents: `sum(hits) / sum(n)` per metric, Wilson recomputed.
fn pool_arm(sources: &[SourceDoc], i: usize) -> ArmResult {
    let arms: Vec<&ArmResult> = sources.iter().map(|s| &s.results.arms[i]).collect();
    let pool = |rate_of: fn(&ArmScore) -> Rate| {
        wilson95(
            arms.iter().map(|arm| rate_of(&arm.score).hits).sum(),
            arms.iter().map(|arm| rate_of(&arm.score).n).sum(),
        )
    };
    ArmResult {
        arm: arms[0].arm.clone(),
        score: ArmScore {
            exact: pool(|s| s.exact),
            w1: pool(|s| s.w1),
            w3: pool(|s| s.w3),
            no_pred: pool(|s| s.no_pred),
        },
        calibration: arms[0].calibration.as_ref().map(|first_bins| {
            (0..first_bins.len())
                .map(|bin| {
                    let (mut hits, mut n) = (0, 0);
                    for arm in &arms {
                        let bins = arm
                            .calibration
                            .as_ref()
                            .expect("calibration presence checked per arm");
                        if let Some(rate) = bins[bin].rate {
                            hits += rate.hits;
                            n += rate.n;
                        }
                    }
                    crate::calibration::CalibrationBin {
                        lo: first_bins[bin].lo,
                        hi: first_bins[bin].hi,
                        rate: (n > 0).then(|| wilson95(hits, n)),
                    }
                })
                .collect()
        }),
    }
}

fn clone_params(results: &BenchResults) -> crate::results::ParamsUsed {
    crate::results::ParamsUsed {
        source: results.params.source.clone(),
        sha256: results.params.sha256.clone(),
        tau: results.params.tau,
        resync_k: results.params.resync_k,
        gap_open: results.params.gap_open,
        gap_ext: results.params.gap_ext,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calibration::{CalibrationBin, N_BINS};
    use crate::results::ParamsUsed;

    /// A minimal single-run document: one arm ("engine", with a calibration curve occupying
    /// the top bin) over `n` pairs with `hits` exact/±1/±3 hits, plus one baseline arm
    /// without calibration. Everything else takes the values the checks compare.
    fn doc(hits: usize, n: usize, split: &str, params_sha: &str) -> BenchResults {
        let top_bin_curve: Vec<CalibrationBin> = (0..N_BINS)
            .map(|i| CalibrationBin {
                lo: i as f64 / N_BINS as f64,
                hi: (i + 1) as f64 / N_BINS as f64,
                rate: (i == N_BINS - 1).then(|| wilson95(hits, n)),
            })
            .collect();
        BenchResults {
            bench_schema_version: crate::results::SCHEMA_VERSION.to_string(),
            protocol: "chimera".to_string(),
            split: split.to_string(),
            coverage: Coverage {
                total: n + 1,
                evaluated: n,
                dev: 0,
                test: n,
                reasons: BTreeMap::from([("empty-run".to_string(), 1)]),
                exclusions: vec![ExclusionRecord {
                    name: "pair_99".to_string(),
                    reason: "empty-run".to_string(),
                    file: "empty.json".to_string(),
                    source: None,
                }],
            },
            n_pairs: n,
            cross_system: 0,
            params: ParamsUsed {
                source: "bench/params.toml".to_string(),
                sha256: params_sha.to_string(),
                tau: 0.3,
                resync_k: 2,
                gap_open: 0.6,
                gap_ext: 0.3,
            },
            sources: Vec::new(),
            pairs: vec![PairRecord {
                name: "pair_00".to_string(),
                task_key: "task-a".to_string(),
                split: split.to_string(),
                source: None,
            }],
            arms: vec![
                ArmResult {
                    arm: "baseline".to_string(),
                    score: ArmScore {
                        exact: wilson95(0, n),
                        w1: wilson95(0, n),
                        w3: wilson95(0, n),
                        no_pred: wilson95(0, n),
                    },
                    calibration: None,
                },
                ArmResult {
                    arm: "engine".to_string(),
                    score: ArmScore {
                        exact: wilson95(hits, n),
                        w1: wilson95(hits, n),
                        w3: wilson95(hits, n),
                        no_pred: wilson95(0, n),
                    },
                    calibration: Some(top_bin_curve),
                },
            ],
        }
    }

    fn source(file: &str, sha: &str, results: BenchResults) -> SourceDoc {
        SourceDoc {
            file: file.to_string(),
            sha256: sha.to_string(),
            results,
        }
    }

    const PARAMS_SHA: &str = "8ebd95ce8f3d50549017b4d381c8d0dc1f76264184ce92e5795895b383f36014";

    #[test]
    fn pooling_sums_hits_and_n_and_recomputes_wilson() {
        // 6/8 + 3/13 pooled is 9/21 — the number one run over both sets would have scored —
        // and the interval is wilson95 at the POOLED n, not any combination of the inputs'.
        let pooled = aggregate(vec![
            source("a.json", "sha-a", doc(6, 8, "test", PARAMS_SHA)),
            source("b.json", "sha-b", doc(3, 13, "test", PARAMS_SHA)),
        ])
        .expect("comparable documents pool");
        let engine = &pooled.arms[1];
        assert_eq!(engine.arm, "engine");
        assert_eq!(engine.score.exact, wilson95(9, 21));
        assert_eq!(engine.score.no_pred, wilson95(0, 21));
        assert_eq!(pooled.n_pairs, 21);
        assert_eq!(pooled.bench_schema_version, crate::results::SCHEMA_VERSION);
    }

    #[test]
    fn calibration_bins_pool_positionally_and_empty_stays_empty() {
        let pooled = aggregate(vec![
            source("a.json", "sha-a", doc(2, 4, "test", PARAMS_SHA)),
            source("b.json", "sha-b", doc(1, 3, "test", PARAMS_SHA)),
        ])
        .expect("comparable documents pool");
        let bins = pooled.arms[1]
            .calibration
            .as_ref()
            .expect("the engine arm keeps its curve");
        assert_eq!(bins.len(), N_BINS);
        assert_eq!(
            bins[N_BINS - 1].rate,
            Some(wilson95(3, 7)),
            "occupied bins sum hits and n"
        );
        assert!(
            bins[..N_BINS - 1].iter().all(|bin| bin.rate.is_none()),
            "a bin empty in every input is empty in the pool — data, not an omission"
        );
        assert!(
            pooled.arms[0].calibration.is_none(),
            "a baseline without a curve stays without one"
        );
    }

    #[test]
    fn coverage_pools_and_provenance_is_explicit() {
        let pooled = aggregate(vec![
            source("a.json", "sha-a", doc(6, 8, "test", PARAMS_SHA)),
            source("b.json", "sha-b", doc(3, 13, "test", PARAMS_SHA)),
        ])
        .expect("comparable documents pool");
        // Coverage sums: totals, evaluated, split sides, and the reason tally (rule 4).
        assert_eq!(pooled.coverage.total, 23);
        assert_eq!(pooled.coverage.evaluated, 21);
        assert_eq!(pooled.coverage.test, 21);
        assert_eq!(pooled.coverage.reasons["empty-run"], 2);
        // Every concatenated record names its document: pair and exclusion names repeat
        // across seed sets, so without the tag the manifest would be ambiguous.
        assert_eq!(
            pooled
                .pairs
                .iter()
                .map(|p| p.source.as_deref())
                .collect::<Vec<_>>(),
            [Some("a.json"), Some("b.json")]
        );
        assert_eq!(
            pooled
                .coverage
                .exclusions
                .iter()
                .map(|e| e.source.as_deref())
                .collect::<Vec<_>>(),
            [Some("a.json"), Some("b.json")]
        );
        // The sources list is the aggregate's own identity line.
        assert_eq!(pooled.sources.len(), 2);
        assert_eq!(pooled.sources[0].file, "a.json");
        assert_eq!(pooled.sources[0].sha256, "sha-a");
        assert_eq!(pooled.sources[0].n_pairs, 8);
    }

    #[test]
    fn fewer_than_two_documents_refuse() {
        let err = aggregate(vec![source(
            "a.json",
            "sha-a",
            doc(1, 2, "test", PARAMS_SHA),
        )])
        .expect_err("one document is not an aggregate");
        assert!(err.contains("at least two"), "{err}");
    }

    #[test]
    fn the_same_document_twice_refuses() {
        let err = aggregate(vec![
            source("a.json", "same-sha", doc(1, 2, "test", PARAMS_SHA)),
            source("copy_of_a.json", "same-sha", doc(1, 2, "test", PARAMS_SHA)),
        ])
        .expect_err("identical bytes are the same run");
        assert!(err.contains("double-count"), "{err}");
        assert!(
            err.contains("a.json") && err.contains("copy_of_a.json"),
            "names both files: {err}"
        );
    }

    #[test]
    fn an_aggregate_input_refuses() {
        let mut nested = doc(1, 2, "test", PARAMS_SHA);
        nested.sources = vec![SourceRecord {
            file: "inner.json".to_string(),
            sha256: "sha-inner".to_string(),
            n_pairs: 2,
        }];
        let err = aggregate(vec![
            source("agg.json", "sha-agg", nested),
            source("b.json", "sha-b", doc(1, 2, "test", PARAMS_SHA)),
        ])
        .expect_err("aggregates of aggregates hide the real inputs");
        assert!(
            err.contains("agg.json") && err.contains("original run"),
            "{err}"
        );
    }

    #[test]
    fn mismatched_split_params_or_arms_refuse() {
        let split = aggregate(vec![
            source("a.json", "sha-a", doc(1, 2, "test", PARAMS_SHA)),
            source("b.json", "sha-b", doc(1, 2, "dev", PARAMS_SHA)),
        ])
        .expect_err("test and dev must not pool");
        assert!(split.contains("split test vs dev"), "{split}");

        let params = aggregate(vec![
            source("a.json", "sha-a", doc(1, 2, "test", PARAMS_SHA)),
            source(
                "b.json",
                "sha-b",
                doc(1, 2, "test", &PARAMS_SHA.replace('8', "9")),
            ),
        ])
        .expect_err("different frozen params must not pool");
        assert!(params.contains("params sha256"), "{params}");

        let mut renamed = doc(1, 2, "test", PARAMS_SHA);
        renamed.arms[1].arm = "engine-v2".to_string();
        let arms = aggregate(vec![
            source("a.json", "sha-a", doc(1, 2, "test", PARAMS_SHA)),
            source("b.json", "sha-b", renamed),
        ])
        .expect_err("different arm sets must not pool");
        assert!(arms.contains("arm engine vs engine-v2"), "{arms}");
    }
}
