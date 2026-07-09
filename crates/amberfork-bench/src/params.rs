//! Frozen-parameter loading — protocol rule 2's mechanism.
//!
//! The harness never runs on code defaults: every `run` names a params file (default
//! `bench/params.toml`) and the loaded values travel with their identity — the sha256 of
//! the exact file bytes — so every published table names the config that produced it and a
//! reviewer verifies it with `shasum -a 256 bench/params.toml`. Hashing bytes rather than
//! parsed values is deliberate: a comment or changelog edit is a new config revision too,
//! and the file's own changelog plus git history explain each hash.
//!
//! Parsing is strict on purpose. Unknown keys and missing keys are errors — a typo must
//! not half-apply while the rest falls back — and the parsed values pass the engine's own
//! `DiffParams::validated()`, so the file cannot express a configuration the engine would
//! reject. The mirror structs below duplicate the engine's params tree instead of
//! deserializing into it directly: the deny-unknown-fields policy stays a bench decision,
//! and a new engine knob forces a conscious schema change here rather than silently
//! widening the frozen format.

use amberfork_align::{AlignParams, DiffParams, ForkParams, ParamError};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::{Path, PathBuf};

/// A parameter set with its provenance: the engine values, the file they came from (as
/// named on the command line — the publishing workflow runs from the repo root, so the
/// committed results say `bench/params.toml`), and the sha256 of that file's exact bytes.
#[derive(Debug)]
pub struct FrozenParams {
    pub params: DiffParams,
    pub source: String,
    pub sha256: String,
}

/// Why a params file was rejected. Every variant is fatal (exit 2): rule 2 has no fallback,
/// so a run that cannot establish its config must not produce a table.
#[derive(Debug)]
pub enum ParamsError {
    /// The file could not be read at all.
    Read {
        file: PathBuf,
        source: std::io::Error,
    },
    /// The bytes are not UTF-8 text — hashable, but not a config.
    NotUtf8 {
        file: PathBuf,
        source: std::str::Utf8Error,
    },
    /// The bytes are not the frozen schema: bad TOML, unknown key, or missing key.
    Parse {
        file: PathBuf,
        source: toml::de::Error,
    },
    /// The values parse but violate an engine invariant.
    Invalid { file: PathBuf, source: ParamError },
}

impl fmt::Display for ParamsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { file, source } => {
                write!(f, "read params {}: {source}", file.display())
            }
            Self::NotUtf8 { file, source } => {
                write!(f, "read params {}: {source}", file.display())
            }
            Self::Parse { file, source } => {
                write!(f, "parse params {}: {source}", file.display())
            }
            Self::Invalid { file, source } => {
                write!(f, "invalid params {}: {source}", file.display())
            }
        }
    }
}

impl std::error::Error for ParamsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::NotUtf8 { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::Invalid { source, .. } => Some(source),
        }
    }
}

/// The frozen file's schema, mirroring the engine's params tree (`[align]`, `[fork]`).
/// Every field required, nothing unknown tolerated.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParamsFile {
    align: AlignSection,
    fork: ForkSection,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AlignSection {
    gap_open: f64,
    gap_ext: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ForkSection {
    tau: f64,
    resync_k: usize,
}

/// Load, hash, parse, and validate a frozen params file.
///
/// # Errors
/// A [`ParamsError`] naming the file and the first thing wrong with it.
pub fn load(path: &Path) -> Result<FrozenParams, ParamsError> {
    let bytes = std::fs::read(path).map_err(|source| ParamsError::Read {
        file: path.to_path_buf(),
        source,
    })?;
    let sha256 = format!("{:x}", Sha256::digest(&bytes));

    let text = std::str::from_utf8(&bytes).map_err(|source| ParamsError::NotUtf8 {
        file: path.to_path_buf(),
        source,
    })?;
    let file: ParamsFile = toml::from_str(text).map_err(|source| ParamsError::Parse {
        file: path.to_path_buf(),
        source,
    })?;

    let params = DiffParams {
        align: AlignParams {
            gap_open: file.align.gap_open,
            gap_ext: file.align.gap_ext,
        },
        fork: ForkParams {
            tau: file.fork.tau,
            resync_k: file.fork.resync_k,
        },
    };
    let params = params.validated().map_err(|source| ParamsError::Invalid {
        file: path.to_path_buf(),
        source,
    })?;

    Ok(FrozenParams {
        params,
        source: path.display().to_string(),
        sha256,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed frozen file, reached from the crate root.
    fn committed_file() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/params.toml")
    }

    fn scratch_file(name: &str, content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("amberfork_bench_params_{name}"));
        std::fs::write(&path, content).expect("write scratch params file");
        path
    }

    const VALID: &str =
        "[align]\ngap_open = 0.6\ngap_ext = 0.3\n\n[fork]\ntau = 0.3\nresync_k = 2\n";

    #[test]
    fn the_committed_freeze_is_the_shipped_engine() {
        // The published table must describe the product people actually run: the frozen
        // file and `DiffParams::default()` (what `amberfork diff` uses) stay equal. A
        // deliberate retune changes BOTH — plus the file's changelog and a notebook entry
        // (rules 2 and 3) — and this test is the tripwire that makes skipping one loud.
        let frozen = load(&committed_file()).expect("committed bench/params.toml loads");
        assert_eq!(frozen.params, DiffParams::default());
        assert_eq!(frozen.sha256.len(), 64, "full sha256 hex, not a prefix");
    }

    #[test]
    fn the_hash_is_the_standard_sha256_a_reviewer_computes() {
        // Independent known answer (`printf '<VALID bytes>' | shasum -a 256`, coreutils —
        // NOT the sha2 crate, which would make the check circular): locks that the
        // reported identity is the standard hash a reviewer verifies with standard tools.
        let valid = scratch_file("valid", VALID);
        let frozen = load(&valid).expect("valid scratch params load");
        assert_eq!(
            frozen.sha256,
            "f4e437bec16a2c460e7fc71b951125317771b3c7e2e8ce811bd224ff0c69574b"
        );
        assert_eq!(frozen.source, valid.display().to_string());
    }

    #[test]
    fn an_unknown_key_is_rejected_not_ignored() {
        let path = scratch_file(
            "unknown_key",
            "[align]\ngap_open = 0.6\ngap_ext = 0.3\nresync_k = 2\n\n[fork]\ntau = 0.3\nresync_k = 2\n",
        );
        let err = load(&path).expect_err("a knob in the wrong section must not vanish");
        assert!(matches!(err, ParamsError::Parse { .. }), "got: {err}");
    }

    #[test]
    fn a_missing_key_is_rejected_not_defaulted() {
        let path = scratch_file(
            "missing_key",
            "[align]\ngap_open = 0.6\ngap_ext = 0.3\n\n[fork]\ntau = 0.3\n",
        );
        let err = load(&path).expect_err("an absent knob must not inherit a code default");
        assert!(matches!(err, ParamsError::Parse { .. }), "got: {err}");
        assert!(err.to_string().contains("resync_k"), "got: {err}");
    }

    #[test]
    fn an_engine_invariant_violation_is_rejected_with_the_engine_message() {
        let path = scratch_file(
            "bad_tau",
            "[align]\ngap_open = 0.6\ngap_ext = 0.3\n\n[fork]\ntau = 2.0\nresync_k = 2\n",
        );
        let err = load(&path).expect_err("tau outside [0, 1] must be rejected");
        assert!(matches!(
            err,
            ParamsError::Invalid {
                source: ParamError::TauOutOfRange(_),
                ..
            }
        ));
        assert!(
            err.to_string().contains("tau must be within [0, 1]"),
            "got: {err}"
        );
    }

    #[test]
    fn a_missing_file_is_an_error_naming_the_path() {
        let err = load(Path::new("does/not/exist.toml")).expect_err("missing file");
        assert!(matches!(err, ParamsError::Read { .. }));
        assert!(
            err.to_string().contains("does/not/exist.toml"),
            "got: {err}"
        );
    }
}
