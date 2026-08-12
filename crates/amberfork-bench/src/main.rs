//! `amberfork-bench` — the offline benchmark harness (issue #6, BENCHMARK.md's
//! pre-registered protocol).
//!
//! `run --pairs <dir>` scores every protocol arm ([`arms::ALL`] — the factorial ladder from
//! the random floor to the shipped engine) on a local chimera pair set and emits the markdown
//! results table (stdout) plus an optional results JSON (`--json-out`). Wilson 95% intervals
//! on every rate; abstentions reported, never dropped. Rules 1 and 4 live here too: every
//! pair carries its dev/test assignment (stable hash of the task key — `--split` selects
//! which side is scored), and the coverage line above the table counts every excluded case
//! with its reason. Rule 2 as well: parameters come ONLY from a frozen file (`--params`,
//! default `bench/params.toml`), and the published artifact names that file's sha256 — no
//! code-default fallback exists. Rule 7: the aligner arms publish their reliability curve
//! (fork confidence binned vs exact-hit rate) under the main table.
//!
//! `report` re-renders a committed results document — no pairs, no engine, no fetch — through
//! the same renderer `run` prints with ([`results::render`]), so the published table
//! reproduces offline, byte for byte, from the repo alone (BENCHMARK.md's definition of
//! done). The canonical committed document lives under `bench/results/`.
//!
//! `judge-prompt` renders the frozen LLM-judge baseline prompt for one pair — the exact bytes
//! a provider will be sent (issue #46, pre-registered in notebook 069). Offline and free: it
//! asks nobody anything, it only shows what would be asked, under the template's pinned
//! sha256. The baseline's credibility rests on that question being auditable before it is
//! paid for.
//!
//! `judge-ask` answers one of those questions. Replay-only by default — the answer comes off
//! disk or the command fails — so no test and no absent-minded invocation can spend money or
//! reach a provider; `--live` plus a key in the environment is the only path to a real call,
//! and what comes back is recorded as a cassette keyed on the rendered prompt's hash. The
//! cassette never stores the prompt itself: TRAIL prompts embed gated GAIA questions.
//!
//! Real pair sets are NOT committed: chimera pairs derive from Who&When logs whose questions
//! originate in GAIA (gated upstream — notebook 001/T30). Regenerate locally with
//! `python3 spike/make_pairs.py`. The committed sets under `tests/fixtures/` are
//! hand-authored fiction, kept so CI can lock the harness itself.
//!
//! A harness, not the product CLI: exit 0 = ran, 2 = trouble. stdout carries only the
//! published artifact (coverage line + table, paste-ready); diagnostics and context go to
//! stderr.

mod aggregate;
mod arms;
mod build;
mod build_trail;
mod calibration;
mod fetch;
mod hal_fetch;
mod hash;
mod jitter;
mod judge_answer;
mod judge_cassette;
mod judge_prompt;
mod judge_provider;
mod judge_run;
mod multiref;
mod pairs;
mod params;
mod pyjson;
mod results;
mod sanitize;
mod score;
mod split;

use arms::Prediction;
use clap::{Args, Parser, Subcommand, ValueEnum};
use judge_prompt::{JudgeArm, PromptSet};
use judge_provider::{Decoding, Gemini, Localizer, Ollama, OpenAi, UreqPost};
use pairs::{Pair, load_pairs};
use results::{ArmResult, BenchResults, Coverage, ExclusionRecord, PairRecord, ParamsUsed};
use split::Split;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Wait between retries of a retryable provider failure (notebook 069: three attempts, then
/// the pair is an exclusion for that arm). Linear, and short enough that a stuck run is
/// obvious rather than merely slow.
const RETRY_BACKOFF: std::time::Duration = std::time::Duration::from_millis(500);

const EXIT_OK: u8 = 0;
const EXIT_TROUBLE: u8 = 2;

#[derive(Parser)]
#[command(name = "amberfork-bench", version, about)]
/// Run amberfork's pre-registered offline benchmark protocol (BENCHMARK.md).
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Score the protocol arms on a local directory of chimera pairs.
    Run(RunArgs),
    /// Re-render a committed results document — offline, zero fetch.
    Report(ReportArgs),
    /// Pool results documents into one exact aggregate — offline, zero engine (issue #14).
    Aggregate(AggregateArgs),
    /// Construct a cross-system Mode A′ pair set from raw TapeAgents + Who&When data (issue #7).
    BuildPairs(BuildPairsArgs),
    /// Construct a natural pair set from TRAIL failing traces + HAL passing references,
    /// matched on GAIA task_id (issue #41 S4c).
    BuildTrailPairs(BuildTrailPairsArgs),
    /// Fetch the pinned raw upstream data `build-pairs` consumes (issue #7).
    Fetch(FetchArgs),
    /// Decrypt a HAL reference-trace zip into the plaintext dump `hal` ingest reads (issue #41).
    HalDecrypt(HalDecryptArgs),
    /// Fetch the pinned HAL Open Deep Research reference zips from Hugging Face (issue #41 S4b).
    HalFetch(HalFetchArgs),
    /// GAIA-sanitize Who&When-derived logs/pairs for redistribution (issues #11/#17).
    Sanitize(SanitizeArgs),
    /// Score multi-reference consensus against a single reference draw (issue #45 slice B,
    /// pre-registered in docs/notebook.md 065).
    Consensus(ConsensusArgs),
    /// Render the frozen LLM-judge baseline prompt for one pair — exactly the bytes a
    /// provider will be sent (issue #46, pre-registered in docs/notebook.md 069).
    JudgePrompt(JudgePromptArgs),
    /// Ask a judge one pair's question and read its answer — cassette-replayed by default,
    /// live only with --live and an API key (issue #46).
    JudgeAsk(JudgeAskArgs),
    /// Score the LLM-judge baseline arms against the product on identical pairs (issue #46
    /// slice A3, pre-registered in docs/notebook.md 069).
    JudgeRun(JudgeRunArgs),
}

#[derive(Args)]
struct RunArgs {
    /// Directory of pair_*.json manifests (the spike/make_pairs.py format).
    #[arg(long, value_name = "DIR")]
    pairs: PathBuf,

    /// Which protocol split to score: dev while tuning, test only with frozen params.
    #[arg(long, value_enum, default_value_t = SplitSelection::All)]
    split: SplitSelection,

    /// Frozen engine parameters (protocol rule 2). The file's sha256 publishes with the
    /// table; there is no code-default fallback. The default resolves from the repo root.
    #[arg(long, value_name = "FILE", default_value = "bench/params.toml")]
    params: PathBuf,

    /// Also write the full results document as JSON.
    #[arg(long, value_name = "FILE")]
    json_out: Option<PathBuf>,
}

#[derive(Args)]
struct ConsensusArgs {
    /// One or more directories of pair_*.json manifests. Named repeatedly because the dev
    /// split lives across three committed seed directories and the experiment is registered
    /// over all 25 pairs, not one seed's 8.
    #[arg(long = "pairs", value_name = "DIR", num_args = 1.., required = true)]
    pairs: Vec<PathBuf>,

    /// Which protocol split to score. Defaults to dev: notebook 065 registers this experiment
    /// on dev only, and the test split is sealed (rule 1).
    #[arg(long, value_enum, default_value_t = SplitSelection::Dev)]
    split: SplitSelection,

    /// Frozen engine parameters (protocol rule 2).
    #[arg(long, value_name = "FILE", default_value = "bench/params.toml")]
    params: PathBuf,

    /// Also write the full results document as JSON.
    #[arg(long, value_name = "FILE")]
    json_out: Option<PathBuf>,
}

#[derive(Args)]
struct ReportArgs {
    /// A results document (what `run --json-out` writes). The default resolves from the
    /// repo root to the canonical committed artifact.
    #[arg(
        long,
        value_name = "FILE",
        default_value = "bench/results/chimera_noise_seed42_dev.json"
    )]
    results: PathBuf,
}

#[derive(Args)]
struct AggregateArgs {
    /// Two or more results documents (what `run --json-out` writes) scoring the same
    /// protocol, split, and frozen params — hits and n are summed per metric and the Wilson
    /// intervals recomputed, so the output is the table one run over the union would have
    /// published. Name paths repo-relative when writing a committable artifact: they are
    /// recorded verbatim as the aggregate's sources.
    #[arg(long = "results", value_name = "FILE", num_args = 1.., required = true)]
    results: Vec<PathBuf>,

    /// Also write the aggregate document as JSON.
    #[arg(long, value_name = "FILE")]
    json_out: Option<PathBuf>,
}

#[derive(Args)]
struct BuildPairsArgs {
    /// Directory of raw TapeAgents tape JSON files (the reference/passing side).
    #[arg(long, value_name = "DIR")]
    tapes: PathBuf,

    /// Directory holding `Hand-Crafted/` and/or `Algorithm-Generated/` subdirectories of raw
    /// Who&When logs (the failing side).
    #[arg(long, value_name = "DIR")]
    logs: PathBuf,

    /// Directory to write the `pair_*.json` + `a_*`/`b_*` triples into (created if absent).
    #[arg(long, value_name = "DIR")]
    out: PathBuf,
}

#[derive(Args)]
struct BuildTrailPairsArgs {
    /// Directory of raw TRAIL trace JSON files (the failing side; `fetch`'s `trail-traces/`).
    #[arg(long, value_name = "DIR")]
    traces: PathBuf,

    /// Directory of TRAIL error-annotation JSON files, one per trace, sharing its basename
    /// (`fetch`'s `trail-gold/`).
    #[arg(long, value_name = "DIR")]
    gold: PathBuf,

    /// Directory of decrypted HAL dump JSON files, one per backing model (the reference side;
    /// `hal-fetch` + `hal-decrypt` output).
    #[arg(long, value_name = "DIR")]
    hal: PathBuf,

    /// Directory to write the `pair_*.json` + `a_*`/`b_*` triples into (created if absent).
    #[arg(long, value_name = "DIR")]
    out: PathBuf,
}

#[derive(Args)]
struct FetchArgs {
    /// Directory to cache the fetched sources under (gitignored: the data is licensed for
    /// local benchmarking, never for committing). A re-run skips files already present;
    /// delete a source's subdirectory to refetch it.
    #[arg(long, value_name = "DIR", default_value = "bench/data")]
    out: PathBuf,
}

#[derive(Args)]
struct HalDecryptArgs {
    /// The encrypted HAL config zip (a `gaia_hf_open_deep_research_*_UPLOAD.zip`), holding one
    /// Fernet-encrypted `.json.encrypted` member.
    #[arg(long, value_name = "FILE")]
    zip: PathBuf,

    /// Where to write the decrypted traces JSON. Omit to stream it to stdout (the dump can be
    /// hundreds of MB — redirect or pipe it into `amberfork-ingest` rather than a terminal).
    #[arg(long, value_name = "FILE")]
    out: Option<PathBuf>,
}

#[derive(Args)]
struct HalFetchArgs {
    /// Directory to cache the reference zips under (gitignored: GAIA-lineage data, never
    /// committed). A re-run skips zips already present; delete one to refetch it.
    #[arg(long, value_name = "DIR", default_value = "bench/data/hal")]
    out: PathBuf,

    /// Only fetch zips whose filename or model label contains this substring (e.g. `gpt41`,
    /// `o3mini`, `claudesonnet45`). Repeatable. Omit to fetch the whole set (~10.8 GB).
    #[arg(long, value_name = "SUBSTR")]
    model: Vec<String>,
}

#[derive(Args)]
struct SanitizeArgs {
    #[command(subcommand)]
    mode: SanitizeMode,
}

/// The two redaction stages (see [`sanitize`]): `canonical` before pair generation so
/// placeholders bake into the prefix, `pairs` after it to catch cross-log leaks.
#[derive(Subcommand)]
enum SanitizeMode {
    /// Redact each canonical log against its own question+answer (stage 1, pre-make_pairs).
    Canonical(SanitizeCanonicalArgs),
    /// Sweep generated pairs against both source logs' question+answer (stage 2).
    Pairs(SanitizePairsArgs),
}

#[derive(Args)]
struct SanitizeCanonicalArgs {
    /// Directory of canonical trace logs plus their index.json (real questions — gated,
    /// never committed). The default resolves from the repo root.
    #[arg(long, value_name = "DIR", default_value = "spike/data/canonical")]
    src: PathBuf,

    /// Directory the sanitized logs and hash-redacted index are written to.
    #[arg(
        long,
        value_name = "DIR",
        default_value = "spike/data/canonical_sanitized"
    )]
    out: PathBuf,

    /// Runs of at least this many consecutive question tokens are redacted (notebook 013).
    #[arg(long, default_value_t = sanitize::DEFAULT_NGRAM)]
    ngram: usize,
}

#[derive(Args)]
struct SanitizePairsArgs {
    /// Directory of pair_*.json manifests plus their run files (the spike/make_pairs.py
    /// format).
    #[arg(long, value_name = "DIR")]
    pairs: PathBuf,

    /// The RAW canonical directory the manifests' meta.x/meta.y source questions are read
    /// from. The default resolves from the repo root.
    #[arg(long, value_name = "DIR", default_value = "spike/data/canonical")]
    canonical: PathBuf,

    /// Directory the swept pairs are written to; may equal --pairs for an in-place sweep.
    #[arg(long, value_name = "DIR")]
    out: PathBuf,

    /// Runs of at least this many consecutive question tokens are redacted (notebook 013).
    #[arg(long, default_value_t = sanitize::DEFAULT_NGRAM)]
    ngram: usize,
}

#[derive(Args)]
struct JudgePromptArgs {
    /// Directory of pair_*.json manifests holding the pair to render.
    #[arg(long, value_name = "DIR")]
    pairs: PathBuf,

    /// Which pair, by manifest stem (`pair_00`).
    #[arg(long, value_name = "NAME")]
    pair: String,

    /// Which registered baseline condition to render.
    #[arg(long, value_enum)]
    arm: JudgeArmSelection,

    /// The failing-run step `judge-stepwise` asks about. Ignored by the other arms.
    #[arg(long, default_value_t = 0)]
    candidate: usize,

    /// The frozen prompt templates (rule 10). Their sha256s are pinned in-code against
    /// notebook 069; the default resolves from the repo root.
    #[arg(long, value_name = "DIR", default_value = "bench/judge_prompts")]
    prompts: PathBuf,
}

#[derive(Args)]
struct JudgeAskArgs {
    #[command(flatten)]
    prompt: JudgePromptArgs,

    /// Which provider answers. `ollama` needs no key; the others read one from the
    /// environment (`OPENAI_API_KEY`, `GEMINI_API_KEY`) and only when `--live` is given.
    #[arg(long, value_enum)]
    provider: ProviderSelection,

    /// The model id, recorded in the cassette key and the published table.
    #[arg(long, value_name = "MODEL")]
    model: String,

    /// Cassette directory. Committed, so a published table replays offline forever.
    #[arg(long, value_name = "DIR", default_value = "bench/cassettes/judge")]
    cassettes: PathBuf,

    /// Call the provider on a cassette miss and record the answer. Without it, a miss is an
    /// error: the default posture cannot spend money or reach a network.
    #[arg(long)]
    live: bool,

    /// Send no temperature at all — the registered fallback for a model that rejects the
    /// parameter. What was actually sent is recorded in the cassette.
    #[arg(long)]
    no_temperature: bool,

    /// Output token ceiling, sent and recorded.
    #[arg(long, default_value_t = 2000)]
    max_output_tokens: u32,

    /// Override the provider's base URL (a local gateway, a proxy, a test double).
    #[arg(long, value_name = "URL")]
    base_url: Option<String>,

    /// Ollama only: the context window to load the model with, in tokens. Unset means Ollama's
    /// 4096 default, which silently truncates most `judge-paired` prompts.
    #[arg(long, value_name = "TOKENS")]
    num_ctx: Option<u32>,
}

#[derive(Args)]
struct JudgeRunArgs {
    /// One or more directories of pair_*.json manifests.
    #[arg(long = "pairs", value_name = "DIR", num_args = 1.., required = true)]
    pairs: Vec<PathBuf>,

    /// Which protocol split to score. Defaults to dev: notebook 069 registers the judge arms
    /// on the dev split, and the test split is sealed until the release tag (rule 1).
    #[arg(long, value_enum, default_value_t = SplitSelection::Dev)]
    split: SplitSelection,

    /// Which registered conditions to score. Repeatable; defaults to all three.
    #[arg(long = "arm", value_enum, num_args = 1.., default_values_t = [JudgeArmSelection::Single, JudgeArmSelection::Paired, JudgeArmSelection::Stepwise])]
    arms: Vec<JudgeArmSelection>,

    /// Frozen engine parameters (rule 2). A baseline never edits these (rule 10); they are
    /// loaded so the product's arms re-score here exactly as they do in `run`.
    #[arg(long, value_name = "FILE", default_value = "bench/params.toml")]
    params: PathBuf,

    /// The frozen prompt templates, pinned against notebook 069.
    #[arg(long, value_name = "DIR", default_value = "bench/judge_prompts")]
    prompts: PathBuf,

    /// Cassette directory. Committed, so the published table replays offline forever.
    #[arg(long, value_name = "DIR", default_value = "bench/cassettes/judge")]
    cassettes: PathBuf,

    #[arg(long, value_enum)]
    provider: ProviderSelection,

    #[arg(long, value_name = "MODEL")]
    model: String,

    /// Call the provider for any question with no cassette, and record the answers. This is
    /// the flag that spends money.
    #[arg(long)]
    live: bool,

    /// Send no temperature at all — the registered fallback for a model that rejects it.
    #[arg(long)]
    no_temperature: bool,

    #[arg(long, default_value_t = 2000)]
    max_output_tokens: u32,

    #[arg(long, value_name = "URL")]
    base_url: Option<String>,

    /// Ollama only: the context window to load the model with, in tokens. Unset means Ollama's
    /// 4096 default, which silently truncates most `judge-paired` prompts; a pair whose prompt
    /// fills the window is excluded rather than answered from a truncated question.
    #[arg(long, value_name = "TOKENS")]
    num_ctx: Option<u32>,

    /// Also write the full results document as JSON.
    #[arg(long, value_name = "FILE")]
    json_out: Option<PathBuf>,
}

/// `--provider`'s choices: the three registered places to ask (notebook 069).
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ProviderSelection {
    Openai,
    Gemini,
    Ollama,
}

impl ProviderSelection {
    /// The environment variable holding this provider's key, if it needs one.
    fn key_var(self) -> Option<&'static str> {
        match self {
            Self::Openai => Some("OPENAI_API_KEY"),
            Self::Gemini => Some("GEMINI_API_KEY"),
            Self::Ollama => None,
        }
    }

    fn default_base_url(self) -> &'static str {
        match self {
            Self::Openai => "https://api.openai.com",
            Self::Gemini => "https://generativelanguage.googleapis.com",
            Self::Ollama => "http://127.0.0.1:11434",
        }
    }
}

/// `--arm`'s choices: the three conditions notebook 069 registered.
#[derive(Clone, Copy, ValueEnum)]
enum JudgeArmSelection {
    Single,
    Paired,
    Stepwise,
}

impl From<JudgeArmSelection> for JudgeArm {
    fn from(selection: JudgeArmSelection) -> Self {
        match selection {
            JudgeArmSelection::Single => Self::Single,
            JudgeArmSelection::Paired => Self::Paired,
            JudgeArmSelection::Stepwise => Self::Stepwise,
        }
    }
}

/// The `--split` choices — the two protocol sides plus `all` (the whole evaluated set, the
/// walking-skeleton default; published tables come from `test`).
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SplitSelection {
    All,
    Dev,
    Test,
}

impl SplitSelection {
    fn admits(self, split: Split) -> bool {
        match self {
            Self::All => true,
            Self::Dev => split == Split::Dev,
            Self::Test => split == Split::Test,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Dev => "dev",
            Self::Test => "test",
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let outcome = match cli.command {
        Command::Run(args) => run(&args),
        Command::Report(args) => report(&args),
        Command::Aggregate(args) => aggregate_documents(&args),
        Command::BuildPairs(args) => build_pairs(&args),
        Command::BuildTrailPairs(args) => build_trail_pairs(&args),
        Command::Fetch(args) => fetch_data(&args),
        Command::HalDecrypt(args) => hal_decrypt(&args),
        Command::HalFetch(args) => hal_fetch_data(&args),
        Command::Sanitize(args) => sanitize_data(&args),
        Command::Consensus(args) => consensus_experiment(&args),
        Command::JudgePrompt(args) => judge_prompt(&args),
        Command::JudgeAsk(args) => judge_ask(&args),
        Command::JudgeRun(args) => judge_run(&args),
    };
    outcome.unwrap_or_else(|err| {
        eprintln!("amberfork-bench: {err}");
        ExitCode::from(EXIT_TROUBLE)
    })
}

fn run(args: &RunArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    // Config before data: a run that cannot establish its frozen parameters (rule 2) has
    // nothing meaningful to say about any pair set.
    let frozen = params::load(&args.params)?;
    let set = load_pairs(&args.pairs)?;
    for exclusion in &set.exclusions {
        eprintln!(
            "amberfork-bench: excluded {}: {}",
            exclusion.name, exclusion.reason
        );
    }
    for pair in &set.pairs {
        for (file, warning) in &pair.warnings {
            eprintln!(
                "amberfork-bench: {}: {}: {}",
                pair.name,
                file.display(),
                warning.msg
            );
        }
    }

    let dev = set
        .pairs
        .iter()
        .filter(|pair| pair.split == Split::Dev)
        .count();
    let test = set.pairs.len() - dev;
    let scored: Vec<&Pair> = set
        .pairs
        .iter()
        .filter(|pair| args.split.admits(pair.split))
        .collect();
    if scored.is_empty() {
        return Err(format!(
            "no pairs to score in split {} (evaluated: dev {dev}, test {test})",
            args.split.as_str()
        )
        .into());
    }

    let params = frozen.params;
    let golds: Vec<usize> = scored.iter().map(|pair| pair.gold_step).collect();

    // The set's cross-system character is a fact of its pairs, not an operator flag: a scored
    // pair is Mode A′ iff its manifest declared it. A set carrying any such pair is labeled
    // `mode-a-prime` and gets the table's cross-system disclosure (issue #7).
    let cross_system = scored.iter().filter(|pair| pair.cross_system).count();
    let protocol = if cross_system > 0 {
        "mode-a-prime"
    } else {
        "chimera"
    };

    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for exclusion in &set.exclusions {
        *reasons
            .entry(exclusion.reason.kind().to_string())
            .or_default() += 1;
    }

    let results = BenchResults {
        bench_schema_version: results::SCHEMA_VERSION.to_string(),
        protocol: protocol.to_string(),
        split: args.split.as_str().to_string(),
        coverage: Coverage {
            total: set.total(),
            evaluated: set.pairs.len(),
            dev,
            test,
            reasons,
            exclusions: set
                .exclusions
                .iter()
                .map(|exclusion| ExclusionRecord {
                    name: exclusion.name.clone(),
                    reason: exclusion.reason.kind().to_string(),
                    file: exclusion.reason.file().display().to_string(),
                    source: None,
                })
                .collect(),
        },
        n_pairs: scored.len(),
        cross_system,
        params: ParamsUsed {
            source: frozen.source,
            sha256: frozen.sha256,
            tau: params.fork.tau,
            resync_k: params.fork.resync_k,
            gap_open: params.align.gap_open,
            gap_ext: params.align.gap_ext,
        },
        sources: Vec::new(),
        pairs: set
            .pairs
            .iter()
            .map(|pair| PairRecord {
                name: pair.name.clone(),
                task_key: pair.task_key.clone(),
                split: pair.split.as_str().to_string(),
                source: None,
            })
            .collect(),
        arms: arms::ALL
            .iter()
            .map(|arm| {
                let preds: Vec<Option<Prediction>> = scored
                    .iter()
                    .map(|pair| arm.predict(pair, &params))
                    .collect();
                let steps: Vec<Option<usize>> = preds
                    .iter()
                    .map(|pred| pred.map(|prediction| prediction.step))
                    .collect();
                ArmResult {
                    arm: arm.name().to_string(),
                    score: score::score(&steps, &golds),
                    calibration: arm
                        .emits_confidence()
                        .then(|| calibration::calibrate(&preds, &golds)),
                }
            })
            .collect(),
    };

    if let Some(path) = &args.json_out {
        write_results(path, &results)?;
    }

    eprintln!(
        "{} protocol · split={} · {} scored of {} evaluated",
        results.protocol, results.split, results.n_pairs, results.coverage.evaluated,
    );
    println!("{}", results::render(&results));
    Ok(ExitCode::from(EXIT_OK))
}

/// The one serialization of a results document — `run` and `aggregate` both write through
/// it, so an aggregate artifact is byte-comparable with the run documents it pools.
/// The multi-reference consensus experiment (issue #45 slice B). Pre-registered in notebook
/// 065: the corpus, the arms, N, the resample count, the statistic, and the decision rule were
/// all committed to git before this code existed.
///
/// Pairs are keyed `<dir>/<name>` because `pair_00` exists in every committed seed directory
/// and the jitter stream must not collide across them.
fn consensus_experiment(args: &ConsensusArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let frozen = params::load(&args.params)?;

    let mut scored: Vec<(String, Pair)> = Vec::new();
    for dir in &args.pairs {
        let set = load_pairs(dir)?;
        for exclusion in &set.exclusions {
            eprintln!(
                "amberfork-bench: excluded {}: {}",
                exclusion.name, exclusion.reason
            );
        }
        let label = dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("pairs");
        for pair in set.pairs {
            if args.split.admits(pair.split) {
                scored.push((format!("{label}/{}", pair.name), pair));
            }
        }
    }
    if scored.is_empty() {
        return Err(format!("no pairs to score in split {}", args.split.as_str()).into());
    }
    // Deterministic order regardless of how the operator ordered `--pairs`: the bootstrap
    // resamples by index, so a reordered input set must not move the published interval.
    scored.sort_by(|(a, _), (b, _)| a.cmp(b));

    let results = multiref::run_experiment(&scored, &frozen.params);

    if let Some(path) = &args.json_out {
        let json = serde_json::to_string_pretty(&results)?;
        std::fs::write(path, json)
            .map_err(|err| format!("write results {}: {err}", path.display()))?;
    }

    eprintln!(
        "consensus experiment · split={} · {} pairs × N={} references · params {}",
        args.split.as_str(),
        results.n_pairs,
        results.n_references,
        frozen.sha256,
    );
    print!("{}", multiref::render(&results));
    Ok(ExitCode::SUCCESS)
}

fn write_results(path: &Path, results: &BenchResults) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string_pretty(results)?;
    std::fs::write(path, json).map_err(|err| format!("write results {}: {err}", path.display()))?;
    Ok(())
}

/// Pool committed results documents into one exact aggregate (issue #14): the cross-seed
/// table, `report`-reproducible from the repo alone instead of asserted in prose. Loads and
/// hashes each input, delegates the pooling and its refusals to [`aggregate::aggregate`],
/// and publishes through the same renderer as `run` and `report`.
fn aggregate_documents(args: &AggregateArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut sources = Vec::with_capacity(args.results.len());
    for path in &args.results {
        let text = std::fs::read_to_string(path)
            .map_err(|err| format!("read results {}: {err}", path.display()))?;
        sources.push(aggregate::SourceDoc {
            file: path.display().to_string(),
            sha256: sha256_hex(text.as_bytes()),
            results: results::parse(&text, path)?,
        });
    }
    let n_sources = sources.len();
    let pooled = aggregate::aggregate(sources)?;

    if let Some(path) = &args.json_out {
        write_results(path, &pooled)?;
    }

    eprintln!(
        "pooled {n_sources} documents · {} protocol · split={} · scored {}",
        pooled.protocol, pooled.split, pooled.n_pairs,
    );
    println!("{}", results::render(&pooled));
    Ok(ExitCode::from(EXIT_OK))
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

/// Construct a Mode A′ pair set (issue #7). A data-prep step, not a scoring run: it emits no
/// table, only the pair triples and a coverage summary on stderr. Building zero pairs is a
/// legitimate outcome (raw sources may not overlap), so it is loud, not an error — only an
/// unreadable input or output path is trouble.
fn build_pairs(args: &BuildPairsArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let stats = build::build_pairs(&args.tapes, &args.logs, &args.out)?;
    for dropped in &stats.drops {
        eprintln!(
            "amberfork-bench: unpaired tape {}: {}",
            dropped.stem, dropped.reason
        );
    }
    eprintln!(
        "amberfork-bench: built {} cross-system pair(s) -> {} \
         (tapes: {}, logs: {}; {} log(s) without a usable gold step)",
        stats.pairs,
        args.out.display(),
        stats.tapes_read,
        stats.logs_read,
        stats.logs_without_gold,
    );
    Ok(ExitCode::from(EXIT_OK))
}

/// Construct a natural TRAIL/HAL pair set (issue #41 S4c). Like `build-pairs`, a data-prep step:
/// no table, only the pair triples and a coverage summary on stderr. Building zero pairs is a
/// legitimate outcome, not an error — only an unreadable input or output path is trouble.
fn build_trail_pairs(args: &BuildTrailPairsArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let stats = build_trail::build_pairs(&args.traces, &args.gold, &args.hal, &args.out)?;
    for dropped in &stats.drops {
        eprintln!(
            "amberfork-bench: unpaired trace {}: {}",
            dropped.stem, dropped.reason
        );
    }
    eprintln!(
        "amberfork-bench: built {} natural pair(s) -> {} \
         (TRAIL traces: {}, {} without a usable gold step; HAL dumps: {}, {} runs read)",
        stats.pairs,
        args.out.display(),
        stats.traces_read,
        stats.traces_without_gold,
        stats.hal_dumps_read,
        stats.hal_runs_read,
    );
    Ok(ExitCode::from(EXIT_OK))
}

/// Acquire the raw Mode A′ sources (issue #7). The step before `build-pairs`: after it, the
/// operator has everything the pair generator consumes, cached locally and never committed.
fn fetch_data(args: &FetchArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let stats = fetch::fetch_all(&fetch::GithubClient, &args.out)?;
    for stat in &stats {
        eprintln!(
            "amberfork-bench: {}: {} file(s) ({} downloaded, {} already cached)",
            stat.name, stat.files, stat.downloaded, stat.skipped,
        );
    }
    let out = args.out.display();
    eprintln!(
        "amberfork-bench: next: amberfork-bench build-pairs --tapes {out}/tapes \
         --logs {out}/whowhen --out {out}/pairs_real",
    );
    Ok(ExitCode::from(EXIT_OK))
}

/// Decrypt a hand-downloaded HAL reference-trace zip into the plaintext dump the `hal` ingest
/// adapter reads (issue #41 S4b). The pinned network fetch of the zip is the next slice; today
/// the operator supplies the zip and this unwraps it. The decrypted JSON is the artifact (stdout
/// or `--out`); the receipt goes to stderr.
fn hal_decrypt(args: &HalDecryptArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let zip = std::fs::read(&args.zip)?;
    let json = hal_fetch::decrypt_traces(&zip, hal_fetch::HAL_PASSWORD)?;
    match &args.out {
        Some(path) => {
            std::fs::write(path, &json)?;
            eprintln!(
                "amberfork-bench: decrypted {} -> {} ({} bytes)",
                args.zip.display(),
                path.display(),
                json.len(),
            );
        }
        None => std::io::Write::write_all(&mut std::io::stdout(), &json)?,
    }
    Ok(ExitCode::from(EXIT_OK))
}

/// Fetch the pinned HAL Open Deep Research reference zips from Hugging Face (issue #41 S4b) — the
/// passing-run side of a natural pair. Selection is by `--model` substring; the notice and
/// per-zip receipts go to stderr. The cached zips feed `hal-decrypt` → `hal` ingest.
fn hal_fetch_data(args: &HalFetchArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let selected: Vec<hal_fetch::HalZip> = hal_fetch::HAL_ODR_ZIPS
        .iter()
        .copied()
        .filter(|zip| {
            args.model.is_empty()
                || args
                    .model
                    .iter()
                    .any(|needle| zip.file.contains(needle) || zip.model.contains(needle))
        })
        .collect();
    if selected.is_empty() {
        eprintln!(
            "amberfork-bench: no reference zip matches --model {:?}; available models:",
            args.model,
        );
        for zip in &hal_fetch::HAL_ODR_ZIPS {
            eprintln!("  {}", zip.model);
        }
        return Ok(ExitCode::from(EXIT_TROUBLE));
    }

    let total: u64 = selected.iter().map(|zip| zip.bytes).sum();
    eprintln!("amberfork-bench: {}", hal_fetch::HAL_NOTICE);
    eprintln!(
        "amberfork-bench: fetching {} reference zip(s) (~{:.1} GB) -> {}",
        selected.len(),
        total as f64 / 1e9,
        args.out.display(),
    );
    let stats = hal_fetch::fetch_hal_zips(&hal_fetch::HfClient, &selected, &args.out)?;
    for stat in &stats {
        eprintln!(
            "amberfork-bench: {} {} [{}] (~{:.0} MB)",
            if stat.downloaded {
                "downloaded"
            } else {
                "cached   "
            },
            stat.file,
            stat.model,
            stat.bytes as f64 / 1e6,
        );
    }
    eprintln!(
        "amberfork-bench: next: amberfork-bench hal-decrypt --zip {}/<file> --out <plaintext.json>",
        args.out.display(),
    );
    Ok(ExitCode::from(EXIT_OK))
}

/// GAIA-sanitize logs or pairs for redistribution (issues #11/#17). A provenance step, not a
/// scoring run: output goes where the operator pointed it, the receipt goes to stderr. A
/// verify failure — any post-condition violation on the written output — is trouble (exit 2):
/// the artifact exists but must not be redistributed.
fn sanitize_data(args: &SanitizeArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    match &args.mode {
        SanitizeMode::Canonical(mode) => {
            let files = sanitize::sanitize_canonical(&mode.src, &mode.out, mode.ngram)?;
            eprintln!(
                "amberfork-bench: sanitized {files} canonical log(s) -> {} (ngram={})",
                mode.out.display(),
                mode.ngram,
            );
        }
        SanitizeMode::Pairs(mode) => {
            let files =
                sanitize::sanitize_pairs(&mode.pairs, &mode.canonical, &mode.out, mode.ngram)?;
            eprintln!(
                "amberfork-bench: swept {files} pair(s) -> {} (ngram={})",
                mode.out.display(),
                mode.ngram,
            );
        }
    }
    eprintln!(
        "amberfork-bench: verify OK — space counts preserved; \
         no surviving question n-gram or answer residue",
    );
    Ok(ExitCode::from(EXIT_OK))
}

fn report(args: &ReportArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let results = results::load(&args.results)?;
    eprintln!(
        "rendering {} · {} protocol · split={} · {} scored of {} evaluated",
        args.results.display(),
        results.protocol,
        results.split,
        results.n_pairs,
        results.coverage.evaluated,
    );
    println!("{}", results::render(&results));
    Ok(ExitCode::from(EXIT_OK))
}

/// Render one pair's frozen judge prompt (issue #46). A prep/inspection step, not a scoring
/// run: stdout carries the exact bytes a provider will be sent, stderr the two hashes that
/// identify them — the template revision (pinned against notebook 069) and the rendered
/// prompt (half of the cassette key the next slice writes).
///
/// It exists so the question a baseline is asked is auditable *before* anyone spends money on
/// answering it. A published table that says "we asked a frontier model" is worth exactly as
/// much as a reader's ability to see what was asked.
/// One pair's question, ready to ask: the pinned template set, the arm, the pair's name, how
/// many steps the failing run has (the answer contract's range), and the rendered bytes.
///
/// Shared by `judge-prompt` and `judge-ask` so the text that gets shown is provably the text
/// that gets sent — two render paths would eventually drift, and a cassette keyed on a prompt
/// nobody can reproduce is worthless.
struct JudgeQuestion {
    prompts: PromptSet,
    arm: JudgeArm,
    pair: String,
    failing_steps: usize,
    rendered: judge_prompt::Rendered,
}

fn render_judge_question(
    args: &JudgePromptArgs,
) -> Result<JudgeQuestion, Box<dyn std::error::Error>> {
    // Prompts before pairs, the same ordering `run` uses for params: an arm that cannot
    // establish which prompt revision it is running has nothing to render.
    let prompts = PromptSet::load(&args.prompts)?;
    let set = load_pairs(&args.pairs)?;
    let pair = set
        .pairs
        .iter()
        .find(|pair| pair.name == args.pair)
        .ok_or_else(|| {
            let available: Vec<&str> = set.pairs.iter().map(|pair| pair.name.as_str()).collect();
            format!(
                "no pair named {} in {} (evaluable: {})",
                args.pair,
                args.pairs.display(),
                available.join(", ")
            )
        })?;

    let arm = JudgeArm::from(args.arm);
    let rendered = match arm {
        JudgeArm::Single => prompts.render_single(&pair.failing),
        JudgeArm::Paired => prompts.render_paired(&pair.reference, &pair.failing),
        JudgeArm::Stepwise => prompts.render_stepwise(&pair.failing, args.candidate),
    }?;

    Ok(JudgeQuestion {
        prompts,
        arm,
        pair: pair.name.clone(),
        failing_steps: pair.failing.steps.len(),
        rendered,
    })
}

fn judge_prompt(args: &JudgePromptArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let question = render_judge_question(args)?;
    eprintln!("{}", question.receipt());
    print!("{}", question.rendered.text);
    Ok(ExitCode::from(EXIT_OK))
}

impl JudgeQuestion {
    fn receipt(&self) -> String {
        let prompt = self.prompts.prompt(self.arm);
        format!(
            "{} · {} · template {} sha256 {} · rendered sha256 {} · {} chars",
            self.pair,
            self.arm,
            prompt.source,
            prompt.sha256,
            self.rendered.sha256,
            self.rendered.text.chars().count(),
        )
    }
}

/// Ask one judge one pair's question (issue #46 slice A2b).
///
/// Replay-only by default: the answer comes off disk or the command fails. `--live` is the
/// only way to reach a provider, and it additionally needs the provider's key in the
/// environment — so no test, and no absent-minded invocation, can spend money.
///
/// A parse failure is NOT an error here. The registration scores it as a miss and counts it,
/// because a judge that cannot obey its own output contract is worse at the task rather than
/// un-evaluable; surfacing it as exit 2 would invite the operator to "fix" it by retrying.
/// Build the provider a judge arm asks through.
///
/// The API key is read ONLY for a live call. A replay must work on a machine that has no key
/// at all — that is the whole point of committing cassettes, and a key required merely to
/// re-render a published table would quietly make the table unreproducible.
fn build_localizer(
    provider: ProviderSelection,
    model: &str,
    base_url: Option<&str>,
    live: bool,
    decoding: Decoding,
    num_ctx: Option<u32>,
) -> Result<Box<dyn Localizer>, Box<dyn std::error::Error>> {
    let api_key = match (live, provider.key_var()) {
        (true, Some(var)) => {
            std::env::var(var).map_err(|_| format!("--live needs {var} in the environment"))?
        }
        _ => String::new(),
    };
    let base_url = base_url
        .map(ToString::to_string)
        .unwrap_or_else(|| provider.default_base_url().to_string());

    Ok(match provider {
        ProviderSelection::Openai => Box::new(OpenAi::new(
            UreqPost,
            base_url,
            model.to_string(),
            api_key,
            decoding,
        )),
        ProviderSelection::Gemini => Box::new(Gemini::new(
            UreqPost,
            base_url,
            model.to_string(),
            api_key,
            decoding,
        )),
        ProviderSelection::Ollama => {
            let ollama = Ollama::new(UreqPost, base_url, model.to_string(), decoding);
            Box::new(match num_ctx {
                Some(num_ctx) => ollama.with_num_ctx(num_ctx),
                None => ollama,
            })
        }
    })
}

/// `--live` is the only way to reach a provider.
fn cassette_mode(live: bool) -> judge_cassette::Mode {
    if live {
        judge_cassette::Mode::Record
    } else {
        judge_cassette::Mode::ReplayOnly
    }
}

/// The registered decoding, or the no-temperature fallback for a model that rejects it.
fn decoding_from(no_temperature: bool, max_output_tokens: u32) -> Decoding {
    if no_temperature {
        Decoding {
            temperature: None,
            max_output_tokens,
        }
    } else {
        Decoding::registered(max_output_tokens)
    }
}

fn judge_ask(args: &JudgeAskArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let question = render_judge_question(&args.prompt)?;
    let decoding = decoding_from(args.no_temperature, args.max_output_tokens);

    let localizer = build_localizer(
        args.provider,
        &args.model,
        args.base_url.as_deref(),
        args.live,
        decoding,
        args.num_ctx,
    )?;
    let mode = cassette_mode(args.live);
    let answer = judge_cassette::obtain(
        &judge_cassette::Cassettes::new(&args.cassettes),
        mode,
        localizer.as_ref(),
        judge_cassette::Question {
            arm: question.arm,
            prompt_sha256: &question.prompts.prompt(question.arm).sha256,
            rendered_prompt_sha256: &question.rendered.sha256,
            prompt: &question.rendered.text,
        },
        RETRY_BACKOFF,
    )?;

    eprintln!("{}", question.receipt());
    eprintln!(
        "{} · {} · cassette {} · {}",
        localizer.provider(),
        localizer.model(),
        answer.key,
        if answer.replayed {
            "replayed"
        } else {
            "recorded live"
        },
    );

    let verdict = match question.arm {
        JudgeArm::Single | JudgeArm::Paired => {
            match judge_answer::parse_step(&answer.text, question.failing_steps) {
                Ok(step) => format!("step {step}"),
                Err(failure) => format!("parse failure (scored as a miss): {failure}"),
            }
        }
        JudgeArm::Stepwise => match judge_answer::parse_decisive(&answer.text) {
            Ok(decisive) => format!("decisive {decisive}"),
            Err(failure) => format!("parse failure (scored as a miss): {failure}"),
        },
    };
    println!("{verdict}");
    println!("--- response ---");
    print!("{}", answer.text);
    Ok(ExitCode::from(EXIT_OK))
}

/// Score the LLM-judge baseline arms (issue #46 slice A3).
///
/// A separate experiment from `run`, the same shape `consensus` took for #45: it emits its own
/// document and leaves every published four-arm table alone (rule 10). amberfork's arms are
/// re-scored here on the identical pairs, from the same frozen params, because rule 9's paired
/// interval needs the product's hits on the very pairs the judge answered.
///
/// Replay-only by default. `--live` is the only path to a provider, and the only way this
/// command costs anything.
fn judge_run(args: &JudgeRunArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let frozen = params::load(&args.params)?;
    let prompts = PromptSet::load(&args.prompts)?;
    let decoding = decoding_from(args.no_temperature, args.max_output_tokens);
    let localizer = build_localizer(
        args.provider,
        &args.model,
        args.base_url.as_deref(),
        args.live,
        decoding,
        args.num_ctx,
    )?;

    let mut scored: Vec<(String, Pair)> = Vec::new();
    for dir in &args.pairs {
        let set = load_pairs(dir)?;
        for exclusion in &set.exclusions {
            eprintln!(
                "amberfork-bench: excluded {}: {}",
                exclusion.name, exclusion.reason
            );
        }
        let label = dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("pairs");
        for pair in set.pairs {
            if args.split.admits(pair.split) {
                scored.push((format!("{label}/{}", pair.name), pair));
            }
        }
    }
    if scored.is_empty() {
        return Err(format!("no pairs to score in split {}", args.split.as_str()).into());
    }
    // Deterministic order regardless of how the operator ordered `--pairs`: the bootstrap
    // resamples by index, so a reordered input set must not move the published interval.
    scored.sort_by(|(a, _), (b, _)| a.cmp(b));

    let arms: Vec<JudgeArm> = args.arms.iter().map(|arm| JudgeArm::from(*arm)).collect();
    let cassettes = judge_cassette::Cassettes::new(&args.cassettes);
    let results = judge_run::run_experiment(
        &scored,
        args.split.as_str(),
        &frozen.sha256,
        &judge_run::Config {
            arms: &arms,
            prompts: &prompts,
            cassettes: &cassettes,
            mode: cassette_mode(args.live),
            localizer: localizer.as_ref(),
            params: &frozen.params,
            backoff: RETRY_BACKOFF,
        },
    )?;

    if let Some(path) = &args.json_out {
        let json = serde_json::to_string_pretty(&results)?;
        std::fs::write(path, json)
            .map_err(|err| format!("write results {}: {err}", path.display()))?;
    }

    eprintln!(
        "judge baseline · split={} · {} pairs · {} {} · prompts {}",
        results.split,
        results.n_pairs,
        results.provider,
        results.model,
        args.prompts.display(),
    );
    print!("{}", judge_run::render(&results));
    Ok(ExitCode::from(EXIT_OK))
}
