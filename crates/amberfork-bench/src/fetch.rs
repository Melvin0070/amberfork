//! Acquiring the raw upstream data Mode A′ pair construction consumes (issue #7).
//!
//! The last piece of the Mode A′ pipeline: [`crate::build`] joins a directory of raw
//! TapeAgents tapes to a directory of raw Who&When logs, but nothing in-tree could *obtain*
//! them — the spike downloaded by hand. `fetch` pulls both sources from GitHub at **pinned
//! commits** ([`SOURCES`]) into the exact layout `build-pairs --tapes/--logs` expects, so the
//! whole path from upstream data to the honest table is one binary, no Python in the loop.
//!
//! Reproducibility comes from the pin, not from checksums: a raw file addressed by
//! `(repo, commit, path)` is immutable content on GitHub, and the file *list* itself is read
//! from the git tree at that same commit — bumping a pin is a reviewed manifest edit, never a
//! silent drift. The downstream consumer strict-parses every file (a malformed source is a hard
//! [`crate::build::BuildError`]), which is the integrity check that actually matters.
//!
//! Licensing is part of the contract, not a footnote: every source carries its license and a
//! redistribution notice that prints *before* any bytes move, and the cache directory is
//! gitignored — Who&When and the tapes embed GAIA questions (gated upstream, "no crawlable
//! resharing"), so fetched data is for local benchmarking, never for committing (BENCHMARK.md
//! "Data & licensing", notebook 001 addendum).
//!
//! The network is quarantined behind the [`Http`] seam (one blocking `GET`); everything that
//! can be wrong — the manifest, the tree filter, the path mapping, skip-vs-download, the
//! provenance record — is pure or fake-drivable and tested offline. CI never touches the
//! network; the one live end-to-end test is `#[ignore]`d for the operator.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

/// One pinned upstream source: a GitHub repo, the commit the file list and contents are read
/// at, the upstream directory to take `.json` files from, and where they land under `--out`.
pub struct Source {
    /// Short identifier used in output and the provenance record.
    pub name: &'static str,
    /// GitHub `owner/repo`.
    pub repo: &'static str,
    /// Full 40-hex commit the fetch is pinned to. Content addressed by `(repo, commit, path)`
    /// is immutable, so this pin *is* the reproducibility story.
    pub commit: &'static str,
    /// Upstream directory prefix (`/`-terminated) whose `.json` blobs are fetched.
    pub prefix: &'static str,
    /// Subdirectory under the output root the files land in (a single path component).
    pub dest: &'static str,
    /// SPDX license of the upstream source.
    pub license: &'static str,
    /// The honest-use line printed before any bytes of this source move.
    pub notice: &'static str,
}

/// The fetch manifest. Tapes first: 8 small files fail fast on a broken network before the
/// 184-file Who&When pull starts.
pub const SOURCES: [Source; 2] = [
    Source {
        name: "tapeagents-gaia",
        repo: "ServiceNow/TapeAgents",
        commit: "e22d5e39ee043fcfe759902df4748d7e937d8aa0",
        prefix: "tests/examples/res/gaia_agent/tapes/",
        dest: "tapes",
        license: "Apache-2.0",
        notice: "TapeAgents GAIA tapes (Apache-2.0). GAIA lineage: local benchmarking only \
                 — never commit or redistribute the fetched files.",
    },
    Source {
        name: "whowhen",
        repo: "ag2ai/Agents_Failure_Attribution",
        commit: "f4d2b6da464a826580e59b3a0eae15ea2d642d7c",
        prefix: "Who&When/",
        dest: "whowhen",
        license: "MIT",
        notice: "Who&When failure logs (MIT, sourced from GitHub — never the unlicensed HF \
                 mirror). GAIA lineage: local benchmarking only — never commit or \
                 redistribute the fetched files.",
    },
];

/// Cap on any single response body. The largest real payload is a recursive tree listing
/// (~1 MB); the cap only exists so a misbehaving endpoint cannot balloon memory.
const MAX_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;

/// The one seam that touches the network: a blocking `GET` returning the body on success.
pub trait Http {
    /// Fetch `url`, returning the response body on a 2xx status.
    ///
    /// # Errors
    /// [`HttpError`] on any transport failure or non-2xx status.
    fn get(&self, url: &str) -> Result<Vec<u8>, HttpError>;
}

/// A failed `GET`: the URL it was for and the transport's own description.
#[derive(Debug)]
pub struct HttpError {
    pub url: String,
    pub msg: String,
}

/// The real [`Http`] implementation, backed by `ureq` (blocking on purpose — the tokio
/// quarantine keeps async out of harness code).
pub struct GithubClient;

impl Http for GithubClient {
    fn get(&self, url: &str) -> Result<Vec<u8>, HttpError> {
        let wrap = |msg: String| HttpError {
            url: url.to_string(),
            msg,
        };
        let mut response = ureq::get(url)
            .header("User-Agent", "amberfork-bench")
            .header("Accept", "application/vnd.github+json")
            .call()
            .map_err(|err| wrap(err.to_string()))?;
        response
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_to_vec()
            .map_err(|err| wrap(err.to_string()))
    }
}

/// What fetching one source did: how many files the pinned tree lists, and how the local
/// cache got there (downloaded now vs already present from an earlier run).
#[derive(Debug)]
pub struct SourceStats {
    pub name: &'static str,
    pub files: usize,
    pub downloaded: usize,
    pub skipped: usize,
}

/// Fetch every source in [`SOURCES`] into `out` and write the provenance record beside them.
///
/// # Errors
/// [`FetchError`] on the first source that cannot be listed, downloaded, or written. Partial
/// progress is kept: a re-run skips every file already present.
pub fn fetch_all(client: &dyn Http, out: &Path) -> Result<Vec<SourceStats>, FetchError> {
    let mut all = Vec::with_capacity(SOURCES.len());
    for source in &SOURCES {
        eprintln!("amberfork-bench: {}: {}", source.name, source.notice);
        eprintln!(
            "amberfork-bench: fetching {}@{} {} -> {}",
            source.repo,
            &source.commit[..8],
            source.prefix,
            out.join(source.dest).display(),
        );
        all.push(fetch_source(client, source, out)?);
    }
    write_provenance(out, &all)?;
    Ok(all)
}

/// Record what the cache was built from, beside the cache itself.
fn write_provenance(out: &Path, stats: &[SourceStats]) -> Result<(), FetchError> {
    let doc = ProvenanceDoc {
        fetched_with: concat!("amberfork-bench fetch v", env!("CARGO_PKG_VERSION")),
        sources: SOURCES
            .iter()
            .zip(stats)
            .map(|(source, stat)| ProvenanceSource {
                name: source.name,
                repo: source.repo,
                commit: source.commit,
                upstream_prefix: source.prefix,
                dest: source.dest,
                license: source.license,
                files: stat.files,
            })
            .collect(),
    };
    let mut json = serde_json::to_string_pretty(&doc).map_err(FetchError::Encode)?;
    json.push('\n');
    let path = out.join("provenance.json");
    std::fs::write(&path, json).map_err(|source| FetchError::Write { path, source })
}

/// Fetch one source: list the pinned tree, then download every wanted file not already
/// cached under `out/<dest>/`, atomically (write-then-rename, so an interrupted run never
/// leaves a truncated file that a later run would skip).
///
/// # Errors
/// [`FetchError`] if the listing cannot be fetched or parsed, or any download or write fails.
pub fn fetch_source(
    client: &dyn Http,
    source: &Source,
    out: &Path,
) -> Result<SourceStats, FetchError> {
    let listing_url = tree_url(source);
    let bytes = client.get(&listing_url).map_err(FetchError::Http)?;
    let listing: TreeResponse =
        serde_json::from_slice(&bytes).map_err(|source| FetchError::Listing {
            url: listing_url,
            source,
        })?;
    let paths = wanted(&listing, source)?;

    let dest_root = out.join(source.dest);
    let mut downloaded = 0;
    let mut skipped = 0;
    for path in &paths {
        let dest = dest_root.join(relative_dest(source, path)?);
        if dest.is_file() {
            skipped += 1;
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|source| FetchError::Dir {
                dir: parent.to_path_buf(),
                source,
            })?;
        }
        let bytes = client
            .get(&raw_url(source, path))
            .map_err(FetchError::Http)?;
        write_atomic(&dest, &bytes)?;
        eprintln!("amberfork-bench: fetched {}", dest.display());
        downloaded += 1;
    }
    Ok(SourceStats {
        name: source.name,
        files: paths.len(),
        downloaded,
        skipped,
    })
}

/// Write via a temp-then-rename so an interrupted run never leaves a truncated file under the
/// final name — the skip-if-present resume check trusts that a present file is a whole file.
fn write_atomic(dest: &Path, bytes: &[u8]) -> Result<(), FetchError> {
    let temp = dest.with_extension("part");
    let result = std::fs::write(&temp, bytes).and_then(|()| std::fs::rename(&temp, dest));
    if result.is_err() {
        // Best effort: a stale .part never confuses a re-run (skips check the final name),
        // but there is no reason to leave one behind either.
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(|source| FetchError::Write {
        path: dest.to_path_buf(),
        source,
    })
}

/// The GitHub git-tree listing URL for a source's pinned commit (recursive: one request
/// lists the whole repo, then [`wanted`] filters to the source's prefix).
#[must_use]
pub fn tree_url(source: &Source) -> String {
    format!(
        "https://api.github.com/repos/{}/git/trees/{}?recursive=1",
        source.repo, source.commit
    )
}

/// The immutable raw-content URL for one upstream file at the source's pinned commit.
#[must_use]
pub fn raw_url(source: &Source, path: &str) -> String {
    // `&` is a legal sub-delimiter inside a URL path segment (RFC 3986) and GitHub's raw host
    // serves `Who&When/...` verbatim — no percent-encoding, which would 404.
    format!(
        "https://raw.githubusercontent.com/{}/{}/{path}",
        source.repo, source.commit
    )
}

/// A git tree listing, as the GitHub API returns it. Only the fields the filter needs.
#[derive(Deserialize)]
struct TreeResponse {
    /// GitHub sets this when the listing is incomplete — a fetch must refuse to proceed on
    /// a partial file list rather than silently deliver a subset.
    truncated: bool,
    tree: Vec<TreeEntry>,
}

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
}

/// The upstream paths a source wants: `.json` blobs under its prefix, sorted so a fetch is
/// deterministic regardless of listing order.
///
/// # Errors
/// [`FetchError::Truncated`] on a partial listing; [`FetchError::NothingToFetch`] when the
/// prefix matches no files — at a pinned commit that can only mean the manifest is wrong.
fn wanted(listing: &TreeResponse, source: &Source) -> Result<Vec<String>, FetchError> {
    if listing.truncated {
        return Err(FetchError::Truncated { repo: source.repo });
    }
    let mut paths: Vec<String> = listing
        .tree
        .iter()
        .filter(|entry| entry.kind == "blob")
        .filter(|entry| entry.path.starts_with(source.prefix))
        .filter(|entry| entry.path.ends_with(".json"))
        .map(|entry| entry.path.clone())
        .collect();
    if paths.is_empty() {
        return Err(FetchError::NothingToFetch {
            repo: source.repo,
            prefix: source.prefix,
        });
    }
    paths.sort();
    Ok(paths)
}

/// Where an upstream path lands relative to the source's dest dir: the path with the prefix
/// stripped, each component checked so listing content can never write outside the cache.
///
/// # Errors
/// [`FetchError::UnsafePath`] on a path outside the prefix or containing `""`/`.`/`..`
/// components.
fn relative_dest(source: &Source, upstream_path: &str) -> Result<PathBuf, FetchError> {
    let unsafe_path = || FetchError::UnsafePath {
        path: upstream_path.to_string(),
    };
    let rel = upstream_path
        .strip_prefix(source.prefix)
        .ok_or_else(unsafe_path)?;
    let mut dest = PathBuf::new();
    for component in rel.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err(unsafe_path());
        }
        dest.push(component);
    }
    Ok(dest)
}

/// The self-describing record written to `<out>/provenance.json`: which repos, at which
/// commits, under which licenses, produced the cache — so a local dataset a number was
/// measured on can always be traced back (BENCHMARK.md's honesty-in-artifacts rule).
#[derive(Serialize)]
struct ProvenanceDoc<'a> {
    fetched_with: &'static str,
    sources: Vec<ProvenanceSource<'a>>,
}

#[derive(Serialize)]
struct ProvenanceSource<'a> {
    name: &'a str,
    repo: &'a str,
    commit: &'a str,
    upstream_prefix: &'a str,
    dest: &'a str,
    license: &'a str,
    files: usize,
}

/// Everything that can go wrong fetching. Each stops the run: the operator's network, disk,
/// or this crate's own manifest needs fixing — loudly, never a partial cache that looks whole.
#[derive(Debug)]
pub enum FetchError {
    /// A `GET` failed (transport error or non-2xx status).
    Http(HttpError),
    /// A tree listing came back as unparseable JSON.
    Listing {
        url: String,
        source: serde_json::Error,
    },
    /// GitHub returned a partial tree listing for this repo.
    Truncated { repo: &'static str },
    /// The pinned tree has no `.json` files under the manifest prefix — a manifest bug.
    NothingToFetch {
        repo: &'static str,
        prefix: &'static str,
    },
    /// An upstream path would escape the cache directory.
    UnsafePath { path: String },
    /// A cache directory could not be created or read.
    Dir {
        dir: PathBuf,
        source: std::io::Error,
    },
    /// A fetched file or the provenance record could not be written.
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The provenance record could not be encoded.
    Encode(serde_json::Error),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(HttpError { url, msg }) => write!(f, "GET {url}: {msg}"),
            Self::Listing { url, source } => write!(f, "listing {url}: {source}"),
            Self::Truncated { repo } => write!(
                f,
                "{repo}: GitHub returned a truncated tree listing; refusing a partial fetch"
            ),
            Self::NothingToFetch { repo, prefix } => write!(
                f,
                "{repo}: no .json files under {prefix} at the pinned commit — manifest wrong?"
            ),
            Self::UnsafePath { path } => {
                write!(f, "refusing upstream path outside the cache: {path}")
            }
            Self::Dir { dir, source } => write!(f, "directory {}: {source}", dir.display()),
            Self::Write { path, source } => write!(f, "write {}: {source}", path.display()),
            Self::Encode(source) => write!(f, "encode provenance: {source}"),
        }
    }
}

impl std::error::Error for FetchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Dir { source, .. } | Self::Write { source, .. } => Some(source),
            Self::Listing { source, .. } | Self::Encode(source) => Some(source),
            Self::Http(_)
            | Self::Truncated { .. }
            | Self::NothingToFetch { .. }
            | Self::UnsafePath { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// Canned-response [`Http`] with a request log, so orchestration tests assert exactly
    /// which URLs were hit without any network.
    struct FakeHttp {
        responses: HashMap<String, Vec<u8>>,
        requests: RefCell<Vec<String>>,
    }

    impl FakeHttp {
        fn new(responses: &[(&str, &str)]) -> Self {
            Self {
                responses: responses
                    .iter()
                    .map(|(url, body)| ((*url).to_string(), body.as_bytes().to_vec()))
                    .collect(),
                requests: RefCell::new(Vec::new()),
            }
        }

        fn requested(&self) -> Vec<String> {
            self.requests.borrow().clone()
        }
    }

    impl Http for FakeHttp {
        fn get(&self, url: &str) -> Result<Vec<u8>, HttpError> {
            self.requests.borrow_mut().push(url.to_string());
            self.responses.get(url).cloned().ok_or_else(|| HttpError {
                url: url.to_string(),
                msg: "no canned response".to_string(),
            })
        }
    }

    fn tapes_source() -> &'static Source {
        SOURCES
            .iter()
            .find(|source| source.name == "tapeagents-gaia")
            .expect("manifest names the tape source")
    }

    fn whowhen_source() -> &'static Source {
        SOURCES
            .iter()
            .find(|source| source.name == "whowhen")
            .expect("manifest names the whowhen source")
    }

    fn listing(truncated: bool, entries: &[(&str, &str)]) -> TreeResponse {
        TreeResponse {
            truncated,
            tree: entries
                .iter()
                .map(|(kind, path)| TreeEntry {
                    path: (*path).to_string(),
                    kind: (*kind).to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn manifest_pins_are_well_formed() {
        let mut names = std::collections::BTreeSet::new();
        for source in &SOURCES {
            assert!(names.insert(source.name), "duplicate name {}", source.name);
            assert_eq!(
                source.commit.len(),
                40,
                "{}: pin must be a full sha",
                source.name
            );
            assert!(
                source.commit.chars().all(|c| c.is_ascii_hexdigit()),
                "{}: pin must be hex",
                source.name
            );
            assert!(
                source.prefix.ends_with('/'),
                "{}: prefix must be a directory",
                source.name
            );
            assert!(
                !source.dest.is_empty() && !source.dest.contains('/'),
                "{}: dest must be a single component",
                source.name
            );
            assert_eq!(
                source.repo.split('/').count(),
                2,
                "{}: repo is owner/name",
                source.name
            );
        }
    }

    #[test]
    fn urls_have_the_pinned_shapes() {
        let tapes = tapes_source();
        assert_eq!(
            tree_url(tapes),
            "https://api.github.com/repos/ServiceNow/TapeAgents/git/trees/\
             e22d5e39ee043fcfe759902df4748d7e937d8aa0?recursive=1"
        );
        assert_eq!(
            raw_url(tapes, "tests/examples/res/gaia_agent/tapes/l1_task000.json"),
            "https://raw.githubusercontent.com/ServiceNow/TapeAgents/\
             e22d5e39ee043fcfe759902df4748d7e937d8aa0/\
             tests/examples/res/gaia_agent/tapes/l1_task000.json"
        );
        // The Who&When prefix contains a literal `&` — legal in a URL path segment, and it
        // must survive verbatim (percent-mangling it would 404 the raw fetch).
        assert!(
            raw_url(whowhen_source(), "Who&When/Hand-Crafted/1.json").ends_with(
                "/f4d2b6da464a826580e59b3a0eae15ea2d642d7c/Who&When/Hand-Crafted/1.json"
            )
        );
    }

    #[test]
    fn wanted_filters_to_json_blobs_under_the_prefix_sorted() {
        let source = whowhen_source();
        let listing = listing(
            false,
            &[
                ("blob", "Who&When/Hand-Crafted/2.json"),
                ("blob", "README.md"),
                ("tree", "Who&When/Hand-Crafted"),
                ("blob", "Who&When/Hand-Crafted/notes.txt"),
                ("blob", "Who&When/Algorithm-Generated/1.json"),
                ("blob", "Automated_FA/1.json"),
            ],
        );
        let got = wanted(&listing, source).expect("filter succeeds");
        assert_eq!(
            got,
            vec![
                "Who&When/Algorithm-Generated/1.json".to_string(),
                "Who&When/Hand-Crafted/2.json".to_string(),
            ]
        );
    }

    #[test]
    fn wanted_refuses_a_truncated_listing() {
        let source = whowhen_source();
        let listing = listing(true, &[("blob", "Who&When/Hand-Crafted/1.json")]);
        assert!(matches!(
            wanted(&listing, source),
            Err(FetchError::Truncated { .. })
        ));
    }

    #[test]
    fn wanted_refuses_an_empty_match() {
        // A pinned commit whose tree has nothing under the prefix means the manifest is
        // wrong (upstream moved the dir and someone bumped the pin without looking).
        let source = whowhen_source();
        let listing = listing(false, &[("blob", "README.md")]);
        assert!(matches!(
            wanted(&listing, source),
            Err(FetchError::NothingToFetch { .. })
        ));
    }

    #[test]
    fn relative_dest_strips_the_prefix_and_keeps_subdirs() {
        let rel = relative_dest(whowhen_source(), "Who&When/Hand-Crafted/7.json")
            .expect("clean path maps");
        assert_eq!(rel, PathBuf::from("Hand-Crafted/7.json"));
    }

    #[test]
    fn relative_dest_rejects_paths_that_escape_the_cache() {
        for bad in [
            "Who&When/../../etc/passwd",
            "Who&When/Hand-Crafted/../../x.json",
            "Who&When//x.json",
            "Who&When/./x.json",
            "elsewhere/x.json",
        ] {
            assert!(
                matches!(
                    relative_dest(whowhen_source(), bad),
                    Err(FetchError::UnsafePath { .. })
                ),
                "{bad} must be rejected"
            );
        }
    }

    /// A canned one-file tree listing + raw body for the tape source, for orchestration tests.
    fn canned_tapes() -> (String, String, String) {
        let tree = tree_url(tapes_source());
        let raw = raw_url(
            tapes_source(),
            "tests/examples/res/gaia_agent/tapes/l1_task000.json",
        );
        let listing = r#"{"truncated": false, "tree": [
            {"path": "tests/examples/res/gaia_agent/tapes/l1_task000.json", "type": "blob"},
            {"path": "tests/examples/res/gaia_agent/tapes/l1_task001.json", "type": "blob"}
        ]}"#;
        (tree, raw, listing.to_string())
    }

    #[test]
    fn fetch_source_downloads_missing_files_and_skips_cached_ones() {
        let (tree, raw0, listing) = canned_tapes();
        let raw1 = raw_url(
            tapes_source(),
            "tests/examples/res/gaia_agent/tapes/l1_task001.json",
        );
        let fake = FakeHttp::new(&[
            (tree.as_str(), listing.as_str()),
            (raw0.as_str(), r#"{"steps": []}"#),
            (raw1.as_str(), r#"{"steps": [1]}"#),
        ]);
        let out = tempfile::tempdir().expect("tempdir");
        // Pre-cache task001: only task000 may be downloaded.
        let dest = out.path().join("tapes");
        std::fs::create_dir_all(&dest).expect("mkdir");
        std::fs::write(dest.join("l1_task001.json"), "cached").expect("precache");

        let stats = fetch_source(&fake, tapes_source(), out.path()).expect("fetch succeeds");

        assert_eq!(stats.files, 2);
        assert_eq!(stats.downloaded, 1);
        assert_eq!(stats.skipped, 1);
        assert_eq!(
            std::fs::read_to_string(dest.join("l1_task000.json")).expect("written"),
            r#"{"steps": []}"#
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("l1_task001.json")).expect("kept"),
            "cached",
            "a cached file is never re-fetched or overwritten"
        );
        assert_eq!(
            fake.requested(),
            vec![tree, raw0],
            "no GET for the cached file"
        );
    }

    #[test]
    fn fetch_source_fails_loudly_when_a_download_fails() {
        // The fake has the listing but no raw bodies: the first download must surface as an
        // Http error naming the URL, and no file (or leftover temp) may be left behind.
        let (tree, raw0, listing) = canned_tapes();
        let fake = FakeHttp::new(&[(tree.as_str(), listing.as_str())]);
        let out = tempfile::tempdir().expect("tempdir");

        let err = fetch_source(&fake, tapes_source(), out.path()).expect_err("must fail");
        match err {
            FetchError::Http(HttpError { url, .. }) => assert_eq!(url, raw0),
            other => panic!("expected Http error, got {other}"),
        }
        let leftovers: Vec<_> = std::fs::read_dir(out.path().join("tapes"))
            .expect("dest dir exists")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .collect();
        assert!(leftovers.is_empty(), "no partial files: {leftovers:?}");
    }

    #[test]
    fn fetch_all_writes_the_provenance_record() {
        let (tape_tree, tape_raw, tape_listing) = canned_tapes();
        // Trim the canned tape listing to one file so a single raw body covers it.
        let tape_listing = tape_listing.replace(
            r#",
            {"path": "tests/examples/res/gaia_agent/tapes/l1_task001.json", "type": "blob"}"#,
            "",
        );
        let who_tree = tree_url(whowhen_source());
        let who_raw = raw_url(whowhen_source(), "Who&When/Hand-Crafted/1.json");
        let who_listing = r#"{"truncated": false, "tree": [
                {"path": "Who&When/Hand-Crafted/1.json", "type": "blob"}
            ]}"#;
        let fake = FakeHttp::new(&[
            (tape_tree.as_str(), tape_listing.as_str()),
            (tape_raw.as_str(), "{}"),
            (who_tree.as_str(), who_listing),
            (who_raw.as_str(), "{}"),
        ]);
        let out = tempfile::tempdir().expect("tempdir");

        let stats = fetch_all(&fake, out.path()).expect("fetch succeeds");
        assert_eq!(stats.len(), SOURCES.len());

        let provenance =
            std::fs::read_to_string(out.path().join("provenance.json")).expect("record written");
        for source in &SOURCES {
            assert!(
                provenance.contains(source.commit),
                "records {}'s pin",
                source.name
            );
            assert!(
                provenance.contains(source.license),
                "records {}'s license",
                source.name
            );
        }
        assert!(
            out.path().join("whowhen/Hand-Crafted/1.json").is_file(),
            "whowhen layout keeps the split subdir build-pairs reads"
        );
    }

    /// The operator's end-to-end check: pull the real pinned tapes (8 small files) and
    /// strict-parse each through the real adapter — the integrity check that matters.
    #[test]
    #[ignore = "network: pulls pinned files from GitHub"]
    fn network_fetch_tapes_end_to_end() {
        let out = tempfile::tempdir().expect("tempdir");
        let stats =
            fetch_source(&GithubClient, tapes_source(), out.path()).expect("live fetch works");
        assert_eq!(
            stats.files, 8,
            "the pinned commit publishes exactly 8 tapes"
        );
        for entry in std::fs::read_dir(out.path().join("tapes")).expect("dest dir") {
            let path = entry.expect("entry").path();
            amberfork_ingest::tape::convert_file(&path)
                .unwrap_or_else(|err| panic!("{} must parse: {err}", path.display()));
        }
    }
}
