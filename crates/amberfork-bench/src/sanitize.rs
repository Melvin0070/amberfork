//! GAIA-sanitize Who&When-derived fixtures for redistribution (issues #11/#17, BENCHMARK.md
//! rule) — the Rust port of `spike/sanitize_gaia.py`, moved in-tree so the code that certifies
//! a licensing-sensitive artifact lives inside the `cargo test` gate instead of the throwaway
//! spike directory.
//!
//! The logs are MIT (ag2ai/Agents_Failure_Attribution, sourced from GitHub), but their
//! questions and ground-truth answers originate in gated GAIA ("no crawlable resharing" —
//! notebook 001/T30). This redacts BOTH, covering STEP CONTENT and not just the `task` field:
//! agents restate the question — whole and in fragments (search queries) — and occasionally
//! the answer, throughout their steps.
//!
//! Two modes, because a committed *pair* is where the constraint actually bites and where
//! naive sanitization silently fails (notebook 013):
//!
//! - **canonical** — redact each run against its OWN question+answer, BEFORE pair generation.
//!   Must run pre-`make_pairs` so placeholders bake into the prefix before `reword()` adds
//!   noise; sanitizing after generation redacts A's noised prefix and B's clean prefix
//!   differently, breaking alignment symmetry and tanking the number (measured 0.75 -> 0.55).
//! - **pairs** — sweep generated pairs against BOTH source questions+answers (from each
//!   manifest's `meta.x`/`meta.y`). A chimera splices log X's prefix onto log Y's tail, so X's
//!   question phrasing can reappear via Y's real content — a cross-log leak canonical mode
//!   cannot see. Run on canonical-sanitized pairs: the prefix is already clean (re-redaction
//!   is a no-op there), so the sweep only touches post-fork tail residue and the number holds.
//!
//! Redaction scheme (deterministic, one-way, alignment/reproducibility-preserving):
//! - question -> any run of >= `ngram` consecutive question tokens, wherever it appears,
//!   becomes per-question placeholders `q<sha8><i>`
//! - answer -> token-boundary, case-insensitive match (and each line) -> `a<sha8><i>`
//! - `task` -> `[GAIA question redacted; sha256:<hash8>]`
//! - index `ground_truth` (canonical mode) -> `sha256:<hash8>`
//!
//! Invariants enforced by [`verify`]:
//! - per-step `' '` count is unchanged (only `[A-Za-z0-9]` spans are edited), so a `--seed 42`
//!   regeneration off canonical-sanitized logs reproduces bit-identical pair STRUCTURE — the
//!   controlled before/after the #11 decision rests on;
//! - no surviving question n-gram and no boundary-matched answer against every relevant Q&A.
//!
//! `ngram` defaults to 4: runs of >=4 consecutive question tokens are GAIA-specific
//! composition; 1-3 token phrases are generic English / proper nouns that don't reconstruct a
//! gated question (notebook 013 records the residual: no >=4-gram survives, but ~86% of
//! question content *words* still appear individually scattered — a licensing judgment
//! recorded in the #11 decision, not a bug here). Answers shorter than [`MIN_ANSWER_LEN`]
//! (bare digits) aren't searched in step text (boundary-matching "3" shreds CSV cells); they
//! are still hashed in task/index regardless of length.
//!
//! **Byte parity is the port's contract.** Output files are written through
//! [`crate::pyjson`] (Python's `json.dumps(run, indent=1)` byte-for-byte), and every redaction
//! primitive mirrors its Python original exactly — including `str.count(" ")`-style space
//! accounting and even `or ""` truthiness on `ground_truth`. The committed fixtures were
//! certified by the Python pipeline; a port that produced different bytes would silently
//! invalidate the "regenerates byte-identically" provenance claim (notebook 013/014).

use crate::pyjson;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

/// Default n-gram threshold: the notebook-013 licensing judgment (see module docs).
pub const DEFAULT_NGRAM: usize = 4;
/// Answers shorter than this (in chars) are never searched in step text.
const MIN_ANSWER_LEN: usize = 3;

/// Stage 1: redact every canonical log under `src` against its own question+answer, writing
/// the sanitized logs plus a hash-redacted `index.json` to `out`. Returns the number of logs
/// sanitized; any post-condition violation is a [`SanitizeError::Verify`].
pub fn sanitize_canonical(src: &Path, out: &Path, ngram: usize) -> Result<usize, SanitizeError> {
    check_ngram(ngram)?;
    ensure_dir(out)?;
    let index_path = src.join("index.json");
    let index = read_json(&index_path)?;
    let entries = index
        .as_array()
        .ok_or_else(|| shape(&index_path, "index.json is not an array"))?;

    let mut new_index = Vec::with_capacity(entries.len());
    let mut failures = Vec::new();
    for meta in entries {
        let meta = meta
            .as_object()
            .ok_or_else(|| shape(&index_path, "index entry is not an object"))?;
        let file = meta
            .get("file")
            .and_then(Value::as_str)
            .ok_or_else(|| shape(&index_path, "index entry without a string `file`"))?;
        let path = src.join(file);
        let original = read_json(&path)?;

        let question = task_text(&original, &path)?.to_string();
        let answer = ground_truth_text(meta, &index_path)?;
        let spec = Spec::new(&question, &answer, ngram);

        let (run, problems) = redact_run(&original, &[&spec], ngram, &spec.qh, &path)?;
        failures.extend(problems.into_iter().map(|p| format!("{file}: {p}")));
        write_pyjson(&out.join(file), &run)?;

        let mut new_meta = meta.clone();
        new_meta.insert(
            "ground_truth".to_string(),
            Value::String(format!("sha256:{}", spec.ah)),
        );
        new_index.push(Value::Object(new_meta));
    }

    write_pyjson(&out.join("index.json"), &Value::Array(new_index))?;
    if failures.is_empty() {
        Ok(entries.len())
    } else {
        Err(SanitizeError::Verify { failures })
    }
}

/// Stage 2: sweep every `pair_*.json` triple under `pairs_dir` against BOTH source logs'
/// question+answer (resolved through the RAW `canonical` dir named by each manifest's
/// `meta.x`/`meta.y`), writing swept runs and verbatim manifests to `out` (which may equal
/// `pairs_dir` for an in-place sweep). Returns the number of pairs swept.
pub fn sanitize_pairs(
    pairs_dir: &Path,
    canonical: &Path,
    out: &Path,
    ngram: usize,
) -> Result<usize, SanitizeError> {
    check_ngram(ngram)?;
    ensure_dir(out)?;
    let index_path = canonical.join("index.json");
    let index = read_json(&index_path)?;
    let entries = index
        .as_array()
        .ok_or_else(|| shape(&index_path, "index.json is not an array"))?;
    let mut index_by_file: Vec<(&str, &Map<String, Value>)> = Vec::with_capacity(entries.len());
    for meta in entries {
        let meta = meta
            .as_object()
            .ok_or_else(|| shape(&index_path, "index entry is not an object"))?;
        let file = meta
            .get("file")
            .and_then(Value::as_str)
            .ok_or_else(|| shape(&index_path, "index entry without a string `file`"))?;
        index_by_file.push((file, meta));
    }

    let spec_for = |canonical_file: &str| -> Result<Spec, SanitizeError> {
        let path = canonical.join(canonical_file);
        let run = read_json(&path)?;
        let question = task_text(&run, &path)?.to_string();
        let (_, meta) = index_by_file
            .iter()
            .find(|(file, _)| *file == canonical_file)
            .ok_or_else(|| shape(&index_path, &format!("no index entry for {canonical_file}")))?;
        let answer = ground_truth_text(meta, &index_path)?;
        Ok(Spec::new(&question, &answer, ngram))
    };

    let manifests = manifest_paths(pairs_dir)?;
    let mut failures = Vec::new();
    for manifest_path in &manifests {
        let manifest_text =
            std::fs::read_to_string(manifest_path).map_err(|source| SanitizeError::Read {
                path: manifest_path.clone(),
                source,
            })?;
        let manifest: Value =
            serde_json::from_str(&manifest_text).map_err(|source| SanitizeError::Parse {
                path: manifest_path.clone(),
                source,
            })?;
        let source_log = |key: &str| -> Result<&str, SanitizeError> {
            manifest
                .get("meta")
                .and_then(|meta| meta.get(key))
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    shape(
                        manifest_path,
                        &format!("manifest without a string meta.{key}"),
                    )
                })
        };
        let specs = [spec_for(source_log("x")?)?, spec_for(source_log("y")?)?];
        let spec_refs: [&Spec; 2] = [&specs[0], &specs[1]];

        for side in ["failing", "reference"] {
            let file = manifest.get(side).and_then(Value::as_str).ok_or_else(|| {
                shape(
                    manifest_path,
                    &format!("manifest without a string `{side}`"),
                )
            })?;
            let path = pairs_dir.join(file);
            let original = read_json(&path)?;
            // Both sides carry X's task (the reference IS log X), so the marker hashes X's
            // question — the same `specs[0]` choice the Python made.
            let (run, problems) = redact_run(&original, &spec_refs, ngram, &specs[0].qh, &path)?;
            failures.extend(problems.into_iter().map(|p| format!("{file}: {p}")));
            write_pyjson(&out.join(file), &run)?;
        }

        let manifest_name = manifest_path
            .file_name()
            .ok_or_else(|| shape(manifest_path, "manifest path without a file name"))?;
        let copy_path = out.join(manifest_name);
        std::fs::write(&copy_path, &manifest_text).map_err(|source| SanitizeError::Write {
            path: copy_path,
            source,
        })?;
    }

    if failures.is_empty() {
        Ok(manifests.len())
    } else {
        Err(SanitizeError::Verify { failures })
    }
}

/// One source Q&A to redact against: the question's n-gram set and both placeholder hashes.
struct Spec {
    ngrams: HashSet<Vec<String>>,
    /// `sha256(question)[..8]` — the `q…` placeholder / task-marker hash.
    qh: String,
    answer: String,
    /// `sha256(answer.lower())[..8]` — the `a…` placeholder / index hash.
    ah: String,
}

impl Spec {
    fn new(question: &str, answer: &str, ngram: usize) -> Self {
        Self {
            ngrams: question_ngrams(question, ngram),
            qh: hash8(question),
            answer: answer.to_string(),
            ah: hash8(&answer.to_lowercase()),
        }
    }
}

/// Redact a run's steps against every spec (question spans first, then the answer — the
/// Python's composition order), stamp the task marker, and return the sanitized run plus
/// whatever [`verify`] found. The original is never mutated: verification compares against it.
fn redact_run(
    original: &Value,
    specs: &[&Spec],
    ngram: usize,
    task_hash: &str,
    path: &Path,
) -> Result<(Value, Vec<String>), SanitizeError> {
    let mut run = original.clone();
    let steps = run
        .get_mut("steps")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| shape(path, "run without a `steps` array"))?;
    for step in steps.iter_mut() {
        let step = step
            .as_object_mut()
            .ok_or_else(|| shape(path, "step is not an object"))?;
        let mut text = step
            .get("outputs")
            .and_then(Value::as_str)
            .ok_or_else(|| shape(path, "step without a string `outputs`"))?
            .to_string();
        for spec in specs {
            text = redact_answer(
                &redact_question_spans(&text, &spec.ngrams, ngram, &spec.qh),
                &spec.answer,
                &spec.ah,
            );
        }
        step.insert("outputs".to_string(), Value::String(text));
    }
    let problems = verify(original, &run, specs, ngram);
    let run_obj = run
        .as_object_mut()
        .ok_or_else(|| shape(path, "run is not an object"))?;
    run_obj.insert(
        "task".to_string(),
        Value::String(format!("[GAIA question redacted; sha256:{task_hash}]")),
    );
    Ok((run, problems))
}

/// Post-conditions across every spec: per-step space counts unchanged; no surviving question
/// n-gram; no boundary-matched answer (when long enough to be searched).
fn verify(original: &Value, sanitized: &Value, specs: &[&Spec], ngram: usize) -> Vec<String> {
    let steps = |run: &'_ Value| -> Vec<Value> {
        run.get("steps")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let mut problems = Vec::new();
    for (orig, san) in steps(original).iter().zip(&steps(sanitized)) {
        let o = orig.get("outputs").and_then(Value::as_str).unwrap_or("");
        let s = san.get("outputs").and_then(Value::as_str).unwrap_or("");
        let idx = orig
            .get("idx")
            .map_or_else(|| "?".to_string(), ToString::to_string);
        let (o_spaces, s_spaces) = (o.matches(' ').count(), s.matches(' ').count());
        if o_spaces != s_spaces {
            problems.push(format!("step {idx}: space count {o_spaces} -> {s_spaces}"));
        }
        let present = ngram_set(&lower_tokens(s), ngram);
        for spec in specs {
            if let Some(survivor) = present.intersection(&spec.ngrams).next() {
                problems.push(format!(
                    "step {idx}: question {ngram}-gram survives {}",
                    survivor.join(" ")
                ));
            }
            let answer = spec.answer.trim();
            if answer.chars().count() >= MIN_ANSWER_LEN && find_boundary_ci(s, answer, 0).is_some()
            {
                problems.push(format!("step {idx}: answer survives"));
            }
        }
    }
    problems
}

/// One word token: a maximal `[A-Za-z0-9]+` run, as byte offsets into the original text.
/// Whitespace (incl. newlines) and punctuation are pure separators, so tokenization is
/// identical however a step wraps the text, and redacting only these spans leaves every space
/// character (and its count) untouched.
struct WordToken {
    start: usize,
    end: usize,
    lower: String,
}

fn word_tokens(text: &str) -> Vec<WordToken> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_alphanumeric() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
                i += 1;
            }
            tokens.push(WordToken {
                start,
                end: i,
                lower: text[start..i].to_ascii_lowercase(),
            });
        } else {
            i += 1;
        }
    }
    tokens
}

fn lower_tokens(text: &str) -> Vec<String> {
    word_tokens(text).into_iter().map(|t| t.lower).collect()
}

fn ngram_set(tokens: &[String], n: usize) -> HashSet<Vec<String>> {
    if tokens.len() < n {
        return HashSet::new();
    }
    tokens.windows(n).map(<[String]>::to_vec).collect()
}

/// Set of n-length lowercased question word-token windows.
fn question_ngrams(question: &str, n: usize) -> HashSet<Vec<String>> {
    ngram_set(&lower_tokens(question), n)
}

/// Replace every word token lying on a matched question n-gram with `q<qh><token index in
/// hex>`, editing only word-character spans so all whitespace (and its count) is preserved
/// exactly.
fn redact_question_spans(text: &str, ngrams: &HashSet<Vec<String>>, n: usize, qh: &str) -> String {
    if ngrams.is_empty() {
        return text.to_string();
    }
    let tokens = word_tokens(text);
    let norm: Vec<String> = tokens.iter().map(|t| t.lower.clone()).collect();
    let mut covered = vec![false; norm.len()];
    if norm.len() >= n {
        for j in 0..=norm.len() - n {
            if ngrams.contains(&norm[j..j + n]) {
                covered[j..j + n].fill(true);
            }
        }
    }
    if !covered.iter().any(|&c| c) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (j, token) in tokens.iter().enumerate() {
        if covered[j] {
            out.push_str(&text[cursor..token.start]);
            out.push_str(&format!("q{qh}{j:x}"));
            cursor = token.end;
        }
    }
    out.push_str(&text[cursor..]);
    out
}

/// Case-insensitive, token-boundary replacement of an answer and each of its lines. Each
/// match becomes one `a<ah><i>` placeholder per space-separated segment, so the replacement
/// preserves the match's space count exactly.
fn redact_answer(text: &str, answer: &str, ah: &str) -> String {
    let mut result = text.to_string();
    let mut units: Vec<&str> = vec![answer];
    if answer.contains('\n') {
        units.extend(answer.split('\n'));
    }
    for unit in units {
        let unit = unit.trim();
        if unit.chars().count() < MIN_ANSWER_LEN {
            continue;
        }
        result = replace_boundary_ci(&result, unit, ah);
    }
    result
}

/// Replace every boundary-valid, case-insensitive occurrence of `unit` in `text`,
/// left-to-right and non-overlapping — the Python `re.sub` scan, hand-rolled because the
/// `regex` crate has no lookaround and this tool warrants no extra dependency.
fn replace_boundary_ci(text: &str, unit: &str, ah: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    let mut from = 0;
    while let Some((start, end)) = find_boundary_ci(text, unit, from) {
        out.push_str(&text[cursor..start]);
        let segments = text[start..end].matches(' ').count() + 1;
        let placeholders: Vec<String> = (0..segments).map(|i| format!("a{ah}{i:x}")).collect();
        out.push_str(&placeholders.join(" "));
        cursor = end;
        from = end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// Find the next case-insensitive occurrence of `needle` at or after byte offset `from` whose
/// neighbors are not `[A-Za-z0-9]` — the port of Python's
/// `(?<![A-Za-z0-9])needle(?![A-Za-z0-9])` with `re.IGNORECASE`. A candidate failing the
/// boundary test resumes the search one char later, exactly as a failed lookaround does.
fn find_boundary_ci(text: &str, needle: &str, mut from: usize) -> Option<(usize, usize)> {
    while from <= text.len() {
        let (start, end) = find_ci(text, needle, from)?;
        let boundary_before = text[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_ascii_alphanumeric());
        let boundary_after = text[end..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric());
        if boundary_before && boundary_after {
            return Some((start, end));
        }
        from = start + text[start..].chars().next().map_or(1, char::len_utf8);
    }
    None
}

/// First case-insensitive occurrence of `needle` at or after byte offset `from`.
fn find_ci(text: &str, needle: &str, from: usize) -> Option<(usize, usize)> {
    if from > text.len() {
        return None;
    }
    for (offset, _) in text[from..].char_indices() {
        let start = from + offset;
        if let Some(end) = match_ci_at(text, start, needle) {
            return Some((start, end));
        }
    }
    None
}

/// Try to match `needle` case-insensitively at byte offset `start`; return the end offset.
fn match_ci_at(text: &str, start: usize, needle: &str) -> Option<usize> {
    let mut haystack = text[start..].chars();
    let mut end = start;
    for needle_char in needle.chars() {
        let hay_char = haystack.next()?;
        let equal =
            hay_char == needle_char || hay_char.to_lowercase().eq(needle_char.to_lowercase());
        if !equal {
            return None;
        }
        end += hay_char.len_utf8();
    }
    Some(end)
}

fn hash8(text: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(text.as_bytes()));
    digest[..8].to_string()
}

/// The run's `task` field: absent means `""` (the Python `.get("task", "")`), but a present
/// non-string is a shape error rather than a silently different coercion.
fn task_text<'a>(run: &'a Value, path: &Path) -> Result<&'a str, SanitizeError> {
    match run.get("task") {
        None => Ok(""),
        Some(Value::String(s)) => Ok(s),
        Some(_) => Err(shape(path, "`task` is not a string")),
    }
}

/// The index entry's `ground_truth` as the Python `str(meta.get("ground_truth", "") or "")`
/// computed it: absent, null, and *falsy* values (empty string, a literal 0) collapse to
/// `""`; integers render as their digits. Any other type is a shape error — Python would have
/// produced a repr this port does not reproduce.
fn ground_truth_text(meta: &Map<String, Value>, path: &Path) -> Result<String, SanitizeError> {
    match meta.get("ground_truth") {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(s)) => Ok(s.clone()),
        Some(Value::Number(n)) => {
            if let Some(i) = n.as_i64() {
                Ok(if i == 0 { String::new() } else { i.to_string() })
            } else if let Some(u) = n.as_u64() {
                Ok(u.to_string())
            } else {
                Err(shape(path, "non-integer `ground_truth`"))
            }
        }
        Some(_) => Err(shape(path, "`ground_truth` has an unsupported type")),
    }
}

/// The `pair_*.json` manifests under `dir`, sorted by file name (the Python `sorted(glob)`).
fn manifest_paths(dir: &Path) -> Result<Vec<PathBuf>, SanitizeError> {
    let entries = std::fs::read_dir(dir).map_err(|source| SanitizeError::Dir {
        dir: dir.to_path_buf(),
        source,
    })?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| SanitizeError::Dir {
            dir: dir.to_path_buf(),
            source,
        })?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("pair_") && name.ends_with(".json") {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

fn check_ngram(ngram: usize) -> Result<(), SanitizeError> {
    if ngram == 0 {
        return Err(SanitizeError::ZeroNgram);
    }
    Ok(())
}

fn ensure_dir(dir: &Path) -> Result<(), SanitizeError> {
    std::fs::create_dir_all(dir).map_err(|source| SanitizeError::Dir {
        dir: dir.to_path_buf(),
        source,
    })
}

fn read_json(path: &Path) -> Result<Value, SanitizeError> {
    let text = std::fs::read_to_string(path).map_err(|source| SanitizeError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| SanitizeError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

fn write_pyjson(path: &Path, value: &Value) -> Result<(), SanitizeError> {
    let text = pyjson::to_string_indent1(value).map_err(|source| SanitizeError::Encode {
        path: path.to_path_buf(),
        source,
    })?;
    std::fs::write(path, text).map_err(|source| SanitizeError::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn shape(path: &Path, what: &str) -> SanitizeError {
    SanitizeError::Shape {
        path: path.to_path_buf(),
        what: what.to_string(),
    }
}

/// Everything that can stop a sanitization run. Every variant is fatal: this tool certifies
/// an artifact for redistribution, so a file it cannot fully account for is a hard stop,
/// never a skip.
#[derive(Debug)]
pub enum SanitizeError {
    /// `--ngram 0` would redact nothing while claiming to have run.
    ZeroNgram,
    /// A directory could not be read or created.
    Dir {
        dir: PathBuf,
        source: std::io::Error,
    },
    /// An input file could not be read.
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    /// An input file is not valid JSON.
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    /// An input file parses but lacks the shape the sanitizer relies on.
    Shape { path: PathBuf, what: String },
    /// A sanitized value could not be encoded Python-compatibly (a non-integer number).
    Encode {
        path: PathBuf,
        source: pyjson::PyJsonError,
    },
    /// An output file could not be written.
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    /// Post-condition violations: the output was written but MUST NOT be redistributed.
    Verify { failures: Vec<String> },
}

impl fmt::Display for SanitizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroNgram => write!(f, "--ngram must be at least 1"),
            Self::Dir { dir, source } => write!(f, "directory {}: {source}", dir.display()),
            Self::Read { path, source } => write!(f, "read {}: {source}", path.display()),
            Self::Parse { path, source } => write!(f, "parse {}: {source}", path.display()),
            Self::Shape { path, what } => write!(f, "{}: {what}", path.display()),
            Self::Encode { path, source } => write!(f, "encode {}: {source}", path.display()),
            Self::Write { path, source } => write!(f, "write {}: {source}", path.display()),
            Self::Verify { failures } => {
                write!(f, "VERIFY FAILED:")?;
                for failure in failures.iter().take(20) {
                    write!(f, "\n{failure}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for SanitizeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Dir { source, .. } | Self::Read { source, .. } | Self::Write { source, .. } => {
                Some(source)
            }
            Self::Parse { source, .. } => Some(source),
            Self::Encode { source, .. } => Some(source),
            Self::ZeroNgram | Self::Shape { .. } | Self::Verify { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // The synthetic Q&A from spike/test_sanitize.py — fabricated, never benchmark-derived
    // (notebook 001/T30). The invariant suite below is that file's port, verbatim.
    const QUESTION: &str =
        "what is the total penguin population on dream island at the end of 2012";
    const ANSWER: &str = "42000";
    const STEP: &str = "the agent searched for the total penguin population on dream island\n\
                        and reported 42000 as the final count for the year";

    fn sanitize_step(text: &str) -> String {
        let ngrams = question_ngrams(QUESTION, DEFAULT_NGRAM);
        let out = redact_question_spans(text, &ngrams, DEFAULT_NGRAM, "deadbeef");
        redact_answer(&out, ANSWER, "cafebabe")
    }

    fn surviving_ngrams(text: &str) -> HashSet<Vec<String>> {
        ngram_set(&lower_tokens(text), DEFAULT_NGRAM)
            .intersection(&question_ngrams(QUESTION, DEFAULT_NGRAM))
            .cloned()
            .collect()
    }

    #[test]
    fn space_count_preserved_and_no_residue() {
        let redacted = sanitize_step(STEP);
        assert_eq!(STEP.matches(' ').count(), redacted.matches(' ').count());
        assert!(
            surviving_ngrams(&redacted).is_empty(),
            "question 4-gram survived"
        );
        assert!(
            find_boundary_ci(&redacted, ANSWER, 0).is_none(),
            "answer survived redaction"
        );
    }

    #[test]
    fn deterministic() {
        assert_eq!(sanitize_step(STEP), sanitize_step(STEP));
    }

    #[test]
    fn idempotent() {
        let once = sanitize_step(STEP);
        let twice = sanitize_step(&once);
        assert_eq!(once, twice, "re-sanitizing is not a no-op");
    }

    #[test]
    fn answer_match_respects_token_boundaries_and_case() {
        // Embedded in a longer alphanumeric run: not a token, must survive.
        assert_eq!(redact_answer("x42000y", ANSWER, "cafebabe"), "x42000y");
        // Punctuation-delimited and case-shifted: both are matches.
        assert_eq!(
            redact_answer("total: 42000.", ANSWER, "cafebabe"),
            "total: acafebabe0."
        );
        assert_eq!(
            redact_answer("The Answer Is Tenet", "tenet", "cafebabe"),
            "The Answer Is acafebabe0"
        );
        // A multi-word answer becomes one placeholder per space-separated segment, so the
        // match's space count (and the step's) is preserved exactly.
        assert_eq!(
            redact_answer("it was time to say time flies", "time flies", "cafebabe"),
            "it was time to say acafebabe0 acafebabe1"
        );
    }

    #[test]
    fn question_placeholders_use_token_index_hex() {
        // Tokens 0..=3 all lie on matched 4-grams of this 5-token question; token indices
        // print in hex, matching the committed fixtures' q<hash8><i> scheme.
        let ngrams = question_ngrams("alpha beta gamma delta", 4);
        assert_eq!(
            redact_question_spans("alpha beta gamma delta!", &ngrams, 4, "deadbeef"),
            "qdeadbeef0 qdeadbeef1 qdeadbeef2 qdeadbeef3!"
        );
    }

    /// The committed dev fixtures under `bench/fixtures/` must carry the sanitizer's
    /// signature — this is the issue-#17 CI check on the licensing-sensitive artifact itself,
    /// using only what is checkable without the gated source data: every pair manifest names
    /// files that exist with a `gold_step` inside the failing run, every run's `task` is a
    /// redaction marker, and the marker's hash reappears as `q<hash8>…` placeholders in step
    /// content (proof the redaction ran over steps, not just the task field).
    #[test]
    fn committed_fixtures_carry_the_sanitizer_signature() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bench/fixtures");
        let mut seed_dirs: Vec<PathBuf> = std::fs::read_dir(&root)
            .expect("bench/fixtures must exist")
            .map(|entry| entry.expect("readable dir entry").path())
            .filter(|path| path.is_dir())
            .collect();
        seed_dirs.sort();
        assert!(
            seed_dirs.len() >= 3,
            "expected the three committed seed dirs, found {seed_dirs:?}"
        );

        for dir in seed_dirs {
            let mut run_files = HashSet::new();
            let mut referenced = HashSet::new();
            for entry in std::fs::read_dir(&dir).expect("readable seed dir") {
                let name = entry.expect("readable entry").file_name();
                let name = name.to_string_lossy().to_string();
                if name.ends_with(".json") && !name.starts_with("pair_") {
                    run_files.insert(name);
                }
            }

            for manifest_path in manifest_paths(&dir).expect("listable manifests") {
                let manifest = read_json(&manifest_path).expect("parseable manifest");
                let failing = manifest["failing"].as_str().expect("string `failing`");
                let reference = manifest["reference"].as_str().expect("string `reference`");
                let gold = manifest["gold_step"].as_u64().expect("integer `gold_step`");
                for key in ["x", "y"] {
                    assert!(
                        manifest["meta"][key].is_string(),
                        "{}: manifest without meta.{key}",
                        manifest_path.display()
                    );
                }
                referenced.insert(failing.to_string());
                referenced.insert(reference.to_string());

                let failing_run = read_json(&dir.join(failing)).expect("parseable failing run");
                let reference_run =
                    read_json(&dir.join(reference)).expect("parseable reference run");
                let steps = failing_run["steps"].as_array().expect("steps array").len();
                assert!(
                    (gold as usize) < steps,
                    "{}: gold_step {gold} outside failing run of {steps} steps",
                    manifest_path.display()
                );
                assert_eq!(
                    failing_run["task"], reference_run["task"],
                    "pair sides disagree on the (redacted) task"
                );

                for run in [&failing_run, &reference_run] {
                    let task = run["task"].as_str().expect("string task");
                    let hash = task
                        .strip_prefix("[GAIA question redacted; sha256:")
                        .and_then(|rest| rest.strip_suffix(']'))
                        .unwrap_or_else(|| panic!("task is not a redaction marker: {task}"));
                    assert!(
                        hash.len() == 8 && hash.chars().all(|c| c.is_ascii_hexdigit()),
                        "marker hash is not 8 hex chars: {task}"
                    );
                    let body: String = run["steps"]
                        .as_array()
                        .expect("steps array")
                        .iter()
                        .map(|step| step["outputs"].as_str().unwrap_or_default())
                        .collect();
                    assert!(
                        body.contains(&format!("q{hash}")),
                        "no q{hash} placeholder in step content — redaction did not reach steps"
                    );
                }
            }
            assert_eq!(
                referenced,
                run_files,
                "{}: run files and manifest references disagree",
                dir.display()
            );
        }
    }
}
