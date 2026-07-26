//! Unwrapping the encrypted HAL reference-trace dumps into the plaintext JSON the
//! [`amberfork_ingest::hal`] adapter reads (issue #41 S4b).
//!
//! HAL (`hal.cs.princeton.edu`) publishes each agent config's traces as a **Fernet-encrypted
//! JSON envelope inside a zip** (one `gaia_hf_open_deep_research_<model>_…_UPLOAD.zip` per
//! backing model, on Hugging Face — notebook 047). This module both **fetches** those zips at a
//! pinned Hugging Face revision ([`fetch_hal_zips`], content-verified against each file's LFS
//! SHA-256) and **decrypts** one into the plaintext traces JSON ([`decrypt_traces`]). Either the
//! operator hand-downloads a zip (as the S4b feasibility spike did) or `hal-fetch` pulls the
//! pinned set; both feed the same [`decrypt_traces`] → `amberfork_ingest::hal::convert_str` path.
//!
//! The recipe is HAL's own published `hal-decrypt.sh`, not a guess. Inside the zip sits exactly
//! one `.json.encrypted` member, itself a JSON envelope `{salt, encrypted_data}`, and:
//!
//! ```text
//! key       = urlsafe_b64( PBKDF2-HMAC-SHA256(password="hal1234", salt=b64d(salt), iters=480000) )
//! plaintext = Fernet(key).decrypt( b64d(encrypted_data) )
//! ```
//!
//! Two layerings are easy to get wrong, so both are pinned by the offline known-answer test:
//! - **The key is derived, not stored.** PBKDF2-HMAC-SHA256 stretches the fixed password over
//!   the per-dump `salt` (480 000 iterations), and the 32 output bytes are *urlsafe*-base64
//!   encoded (with padding) to form the Fernet key string — matching Python's
//!   `base64.urlsafe_b64encode`, which is what `cryptography.fernet.Fernet(key)` consumes.
//! - **`encrypted_data` is double-base64.** A Fernet token is already urlsafe-base64 text; HAL
//!   base64-encodes that token string a second time into the envelope. So one standard-base64
//!   decode of `encrypted_data` yields the Fernet token string, which [`fernet`] then decrypts.
//!
//! This lives in `amberfork-bench`, not `amberfork-ingest`: ingest is the lean, forgiving
//! JSON-only loader, while decryption and the network fetch ([`fetch_hal_zips`]) are data
//! acquisition — they belong beside [`crate::fetch`] with the zip/crypto/HTTP deps, and hand
//! their plaintext output straight to `amberfork_ingest::hal::convert_str`. Crypto is never
//! hand-rolled: [`fernet`]
//! provides the AES-CBC + HMAC-SHA256 authenticated decryption, so a wrong password or a
//! corrupted dump fails loudly at the MAC ([`HalDecryptError::Decrypt`]) rather than returning
//! garbage.

use crate::fetch::HttpError;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

/// The fixed password HAL's `hal-decrypt.sh` derives every dump's Fernet key from. Passed
/// explicitly to [`decrypt_traces`] so the offline test can round-trip its own password, but
/// this is the only value that decrypts a real HAL dump.
pub const HAL_PASSWORD: &str = "hal1234";

/// PBKDF2 iteration count HAL fixes (`hal-decrypt.sh`). Part of the format, not a tunable — a
/// different count derives a different key and the Fernet MAC rejects it.
const HAL_PBKDF2_ROUNDS: u32 = 480_000;

/// The suffix of the single encrypted member inside a HAL config zip.
const ENCRYPTED_MEMBER_SUFFIX: &str = ".json.encrypted";

/// The encrypted envelope carried by the zip's `.json.encrypted` member: a per-dump PBKDF2 salt
/// and the double-base64 Fernet token, both standard-base64 strings.
#[derive(Deserialize)]
struct Envelope {
    salt: String,
    encrypted_data: String,
}

/// Decrypt one HAL config zip into the plaintext traces JSON its `.json.encrypted` member holds.
///
/// The returned bytes are the decrypted dump — the exact input
/// `amberfork_ingest::hal::convert_str` expects. `password` is [`HAL_PASSWORD`] for any real
/// dump.
///
/// # Errors
/// [`HalDecryptError`] if `zip` is not a readable zip, does not hold exactly one
/// `.json.encrypted` member, that member is not a `{salt, encrypted_data}` envelope, either field
/// is not valid base64, or authenticated decryption fails (a wrong password or a corrupted dump —
/// the Fernet MAC never yields plausible-but-wrong plaintext).
pub fn decrypt_traces(zip: &[u8], password: &str) -> Result<Vec<u8>, HalDecryptError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip)).map_err(HalDecryptError::Zip)?;

    // Exactly one `.json.encrypted` member is HAL's format; zero or many is a hard error, never
    // a pick-the-first — the wrong dump silently decrypting is worse than a stop.
    let mut member_index = None;
    let mut count = 0;
    for index in 0..archive.len() {
        let name = archive
            .by_index(index)
            .map_err(HalDecryptError::Zip)?
            .name()
            .to_string();
        if name.ends_with(ENCRYPTED_MEMBER_SUFFIX) {
            count += 1;
            member_index = Some(index);
        }
    }
    let member_index = match member_index {
        Some(index) if count == 1 => index,
        _ => return Err(HalDecryptError::Member { count }),
    };

    let mut raw = Vec::new();
    archive
        .by_index(member_index)
        .map_err(HalDecryptError::Zip)?
        .read_to_end(&mut raw)
        .map_err(HalDecryptError::Read)?;
    let envelope: Envelope = serde_json::from_slice(&raw).map_err(HalDecryptError::Envelope)?;

    // Derive the Fernet key: PBKDF2-HMAC-SHA256 over the decoded salt, then urlsafe-b64 (the
    // encoding `Fernet(key)` expects). The salt is standard-base64 in the envelope.
    let salt = STANDARD
        .decode(&envelope.salt)
        .map_err(|source| HalDecryptError::Base64 {
            field: "salt",
            source,
        })?;
    let derived =
        pbkdf2::pbkdf2_hmac_array::<Sha256, 32>(password.as_bytes(), &salt, HAL_PBKDF2_ROUNDS);
    let key = URL_SAFE.encode(derived);

    // `encrypted_data` is double-base64: one standard decode yields the Fernet token *string*,
    // which the token's own urlsafe-base64 (handled inside `fernet`) then unwraps.
    let token_bytes = STANDARD
        .decode(&envelope.encrypted_data)
        .map_err(|source| HalDecryptError::Base64 {
            field: "encrypted_data",
            source,
        })?;
    let token = std::str::from_utf8(&token_bytes).map_err(HalDecryptError::Utf8)?;
    let fernet = fernet::Fernet::new(&key).ok_or(HalDecryptError::Key)?;
    fernet.decrypt(token).map_err(|_| HalDecryptError::Decrypt)
}

/// Everything that can go wrong turning a HAL zip into plaintext JSON. Each is a hard stop: the
/// dump, the password, or the file itself is wrong, never a partial or guessed result.
#[derive(Debug)]
pub enum HalDecryptError {
    /// The bytes are not a readable zip archive.
    Zip(zip::result::ZipError),
    /// The zip does not hold exactly one `.json.encrypted` member (found `count`).
    Member { count: usize },
    /// The encrypted member could not be read/decompressed out of the zip.
    Read(std::io::Error),
    /// The member is not a `{salt, encrypted_data}` JSON envelope.
    Envelope(serde_json::Error),
    /// A base64 envelope field (`field`) did not decode.
    Base64 {
        field: &'static str,
        source: base64::DecodeError,
    },
    /// The decoded `encrypted_data` is not the UTF-8 text of a Fernet token.
    Utf8(std::str::Utf8Error),
    /// The derived key was rejected as a Fernet key (a wrong length — unreachable for a 32-byte
    /// PBKDF2 output, kept so the `fernet` boundary never panics).
    Key,
    /// Authenticated decryption failed: a wrong password or a corrupted dump.
    Decrypt,
}

impl fmt::Display for HalDecryptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zip(source) => write!(f, "not a readable zip: {source}"),
            Self::Member { count } => write!(
                f,
                "expected exactly one {ENCRYPTED_MEMBER_SUFFIX} member, found {count}"
            ),
            Self::Read(source) => write!(f, "reading the encrypted member: {source}"),
            Self::Envelope(source) => {
                write!(
                    f,
                    "member is not a {{salt, encrypted_data}} envelope: {source}"
                )
            }
            Self::Base64 { field, source } => write!(f, "envelope {field} is not base64: {source}"),
            Self::Utf8(source) => write!(f, "encrypted_data is not a UTF-8 Fernet token: {source}"),
            Self::Key => write!(f, "derived key rejected by Fernet (wrong length)"),
            Self::Decrypt => write!(
                f,
                "authenticated decryption failed — wrong password or corrupted dump"
            ),
        }
    }
}

impl std::error::Error for HalDecryptError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Zip(source) => Some(source),
            Self::Read(source) => Some(source),
            Self::Envelope(source) => Some(source),
            Self::Base64 { source, .. } => Some(source),
            Self::Utf8(source) => Some(source),
            Self::Member { .. } | Self::Key | Self::Decrypt => None,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Fetch: pulling the pinned HAL reference zips from Hugging Face (#41 S4b, slice 2).
// ---------------------------------------------------------------------------------------------

/// The Hugging Face dataset the HAL Open Deep Research reference traces are published under.
pub const HAL_DATASET: &str = "agent-evals/hal_traces";

/// The immutable Hugging Face commit the reference set is pinned to. Content addressed by
/// `(dataset, revision, file)` is immutable, so this pin *is* the reproducibility story — the
/// same role a GitHub commit plays in [`crate::fetch`]. Bumping it is a reviewed manifest edit.
pub const HAL_REVISION: &str = "e7dcedc82b4f4bc819a170fd6616bdb44841c71e";

/// Printed before any bytes of a reference zip move: these embed GAIA questions/answers (gated
/// upstream), so a fetched zip is for local benchmarking only, never committed or redistributed.
/// The Hugging Face dataset declares no license; use is governed by that GAIA lineage.
pub const HAL_NOTICE: &str = "HAL Open Deep Research GAIA reference traces (Hugging Face \
    agent-evals/hal_traces; no license declared). GAIA lineage: local benchmarking only — never \
    commit or redistribute the fetched files.";

/// One pinned HAL Open Deep Research GAIA reference zip: the backing-model label, the exact
/// filename at [`HAL_REVISION`], its published size, and the LFS content hash the download is
/// verified against.
///
/// `sha256` is not decoration. A half-gigabyte-to-2-GB transfer can truncate or corrupt in ways
/// the commit pin cannot catch, and a binary zip cannot be cheaply strict-parsed to notice
/// (unlike the JSON [`crate::fetch`] pulls). So every download is bounded to `bytes` and checked
/// against `sha256` *before* it is allowed to land under its final name — which is also what
/// makes the skip-if-present resume sound: a present file was verified before it was renamed in.
#[derive(Clone, Copy)]
pub struct HalZip {
    /// Backing-model label (derived from the filename); the `--model` filter, receipts, and
    /// provenance read it. Unique across the manifest (the two `claudesonnet45` reruns keep
    /// their timestamps).
    pub model: &'static str,
    /// Exact filename at [`HAL_REVISION`] — the unique key of a reference zip.
    pub file: &'static str,
    /// Published byte length; the streamed download is bounded to it and must match exactly.
    pub bytes: u64,
    /// Lowercase-hex SHA-256 of the file content (Hugging Face's LFS oid), verified post-download.
    pub sha256: &'static str,
}

/// The 16 GAIA Open Deep Research reference zips HAL publishes, **smallest first** so a broken
/// network or a manifest typo fails on a cheap pull, and so the `#[ignore]`d live test exercises
/// the whole path on the smallest file. One entry per backing-model run. Total ~10.8 GB, so
/// `hal-fetch` prints the selected size before any bytes move and a re-run skips what is cached.
pub const HAL_ODR_ZIPS: [HalZip; 16] = [
    HalZip {
        model: "o3mini20250131_high",
        file: "gaia_hf_open_deep_research_o3mini20250131_high_1744843485_UPLOAD.zip",
        bytes: 106_877_482,
        sha256: "1f08fa7742937a1b5eff524db906ef8f5f2e3aa25709f0869656befc88099e7c",
    },
    HalZip {
        model: "deepseekaideepseekr1",
        file: "gaia_hf_open_deep_research_deepseekaideepseekr1_1744851690_UPLOAD.zip",
        bytes: 261_786_511,
        sha256: "5ac331833dcb0ce7133fb63348a4a0b731488af085e01be0cced68a6cd375401",
    },
    HalZip {
        model: "gpt4120250414",
        file: "gaia_hf_open_deep_research_gpt4120250414_1744843595_UPLOAD.zip",
        bytes: 290_057_118,
        sha256: "0bc051fe7819b31c47aa6c52b88c028edd94258c34d9db217591a31cb9a26095",
    },
    HalZip {
        model: "o320250416",
        file: "gaia_hf_open_deep_research_o320250416_1745876880_UPLOAD.zip",
        bytes: 324_182_150,
        sha256: "1f3b979d0dcb904aa5f95f3fe36a7b565c3cc4d6637e5b2b1e99879b762698ca",
    },
    HalZip {
        model: "deepseekaideepseekv3",
        file: "gaia_hf_open_deep_research_deepseekaideepseekv3_1744851680_UPLOAD.zip",
        bytes: 337_264_192,
        sha256: "9b9ef0220860825bfdd37603a4b3c7bce06a03d89a20fcc21bfa051dbe830c0a",
    },
    HalZip {
        model: "claudeopus41",
        file: "gaia_hf_open_deep_research_claudeopus41_1755030930_UPLOAD.zip",
        bytes: 360_002_437,
        sha256: "51ae75f832dce59d4b37f74259bb69fe29cc498af1b599c2da9b8f26b8fa0590",
    },
    HalZip {
        model: "o4mini20250416_low",
        file: "gaia_hf_open_deep_research_o4mini20250416_low_1744921254_UPLOAD.zip",
        bytes: 409_451_295,
        sha256: "f80e01ef583db1de078bbea62431489ac86558e263315323592258386fe33793",
    },
    HalZip {
        model: "claudeopus41_high",
        file: "gaia_hf_open_deep_research_claudeopus41_high_1755092997_UPLOAD.zip",
        bytes: 410_996_720,
        sha256: "dae8191f7ff2c34dae88aad46e3059dd0eb962c65afebb358b2250576101d591",
    },
    HalZip {
        model: "claudeopus4",
        file: "gaia_hf_open_deep_research_claudeopus4_1754425534_UPLOAD.zip",
        bytes: 465_460_146,
        sha256: "7c3e2fc8a55fc9a43ccd2a3c094b7d0f55d35d1f3c14b6e28520af60917090e4",
    },
    HalZip {
        model: "claude37sonnet20250219_high",
        file: "gaia_hf_open_deep_research_claude37sonnet20250219_high_1745539901_UPLOAD.zip",
        bytes: 535_261_989,
        sha256: "b0b9204b79a631fbbeec9e7d7bc3d94ad6b3890badb283550b22f38982075eff",
    },
    HalZip {
        model: "o4mini20250416_high",
        file: "gaia_hf_open_deep_research_o4mini20250416_high_1744923206_UPLOAD.zip",
        bytes: 575_451_872,
        sha256: "62ceae058b634e340f217afc1886b37ca1c2c2a8648fb4ecc98fcc937a7d3fdf",
    },
    HalZip {
        model: "claude37sonnet20250219",
        file: "gaia_hf_open_deep_research_claude37sonnet20250219_1745000974_UPLOAD.zip",
        bytes: 598_844_870,
        sha256: "e989ee956435ec512f2a4d60476cccd277e65fbd0fb987eb4f974e5f2b81eb0f",
    },
    HalZip {
        model: "gemini20flash",
        file: "gaia_hf_open_deep_research_gemini20flash_1744843220_UPLOAD.zip",
        bytes: 843_077_816,
        sha256: "4971dc0e5a7de87f5ae42a930103271a92b2c2d0cc40737dd620537114191185",
    },
    HalZip {
        model: "gpt520250807",
        file: "gaia_hf_open_deep_research_gpt520250807_1754605128_UPLOAD.zip",
        bytes: 917_921_228,
        sha256: "573999ede4da50f7481aeafd6f19273bdb2595feb297cbe6ddecd12f51edd91d",
    },
    HalZip {
        model: "claudesonnet45_1759311826",
        file: "gaia_hf_open_deep_research_claudesonnet45_1759311826_UPLOAD.zip",
        bytes: 2_017_237_370,
        sha256: "39085eed347058cd49fd7ed15b3340d014c1653a659061163292b00c0389de8b",
    },
    HalZip {
        model: "claudesonnet45_1759261812",
        file: "gaia_hf_open_deep_research_claudesonnet45_1759261812_UPLOAD.zip",
        bytes: 2_357_675_129,
        sha256: "03ac24b2405def43202d01497b83d699d22b6a6203f4a2c6b458c3f5994f3a40",
    },
];

/// The immutable Hugging Face `resolve` URL for one pinned reference zip. The filename is all
/// `[A-Za-z0-9_.]`, so it needs no percent-encoding and is served verbatim.
#[must_use]
pub fn resolve_url(zip: &HalZip) -> String {
    format!(
        "https://huggingface.co/datasets/{HAL_DATASET}/resolve/{HAL_REVISION}/{}",
        zip.file
    )
}

/// The one seam that touches the network for HAL fetches: a blocking `GET` that streams the
/// response body into `sink`, reading at most `max_bytes`, and returns the bytes written.
///
/// Separate from [`crate::fetch::Http`] on purpose. That seam buffers a whole body into a `Vec`
/// under a 64 MB cap and speaks GitHub's API; a HAL reference zip is hundreds of MB to 2 GB and
/// must stream straight to disk. `max_bytes` is the streaming analogue of `fetch`'s response cap
/// — a misbehaving endpoint can never outrun the pinned size, and a short stream is caught by the
/// byte count the caller checks against [`HalZip::bytes`].
pub trait HalHttp {
    /// Stream the body of `GET url` into `sink`, reading at most `max_bytes` bytes.
    ///
    /// # Errors
    /// [`HttpError`] on any transport failure, non-2xx status, or write error into `sink`.
    fn get_to(&self, url: &str, max_bytes: u64, sink: &mut dyn Write) -> Result<u64, HttpError>;
}

/// The real [`HalHttp`], backed by `ureq` (blocking — the tokio quarantine keeps async out of
/// harness code). Follows Hugging Face's 302 from the `resolve` URL to the CDN automatically.
pub struct HfClient;

impl HalHttp for HfClient {
    fn get_to(&self, url: &str, max_bytes: u64, sink: &mut dyn Write) -> Result<u64, HttpError> {
        let wrap = |msg: String| HttpError {
            url: url.to_string(),
            msg,
        };
        let mut response = ureq::get(url)
            .header("User-Agent", "amberfork-bench")
            .call()
            .map_err(|err| wrap(err.to_string()))?;
        // `as_reader()` is unlimited (the 10 MB default cap only applies to the buffered `read_*`
        // helpers), so the `.take` is what actually bounds the stream to the pinned size.
        let mut reader = response.body_mut().as_reader().take(max_bytes);
        std::io::copy(&mut reader, sink).map_err(|err| wrap(err.to_string()))
    }
}

/// A [`Write`] that streams bytes to an inner writer while folding them into a running SHA-256,
/// so a half-gigabyte download is verified as it lands — no second pass over the file.
struct HashingWriter<W: Write> {
    inner: W,
    hasher: Sha256,
}

impl<W: Write> HashingWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
        }
    }

    /// Consume the writer and return the accumulated digest as lowercase hex.
    fn digest_hex(self) -> String {
        let bytes = self.hasher.finalize();
        let mut hex = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            use std::fmt::Write as _;
            let _ = write!(hex, "{byte:02x}");
        }
        hex
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.hasher.update(&buf[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// What fetching one reference zip did: which model/file, its size, and whether this run
/// downloaded it or found it already cached.
#[derive(Debug)]
pub struct HalZipStat {
    pub model: &'static str,
    pub file: &'static str,
    pub bytes: u64,
    pub downloaded: bool,
}

/// Fetch each of `zips` into `out` (verifying every download against its pinned SHA-256) and
/// write the provenance record beside them.
///
/// # Errors
/// [`HalFetchError`] on the first zip that cannot be fetched, verified, or written. Partial
/// progress is kept: a re-run skips every zip already present.
pub fn fetch_hal_zips(
    client: &dyn HalHttp,
    zips: &[HalZip],
    out: &Path,
) -> Result<Vec<HalZipStat>, HalFetchError> {
    let mut stats = Vec::with_capacity(zips.len());
    for zip in zips {
        stats.push(fetch_hal_zip(client, zip, out)?);
    }
    write_hal_provenance(out, zips)?;
    Ok(stats)
}

/// Fetch one reference zip: skip if already cached, else stream it to a `.part` file (hashing as
/// it lands), verify size and SHA-256, and only then atomically rename it into place — so a
/// present file under its final name is always a whole, verified file.
///
/// # Errors
/// [`HalFetchError`] if the download fails, the byte count or SHA-256 does not match the pinned
/// manifest, or a directory/file cannot be created, written, or renamed.
pub fn fetch_hal_zip(
    client: &dyn HalHttp,
    zip: &HalZip,
    out: &Path,
) -> Result<HalZipStat, HalFetchError> {
    // Defence in depth: the manifest names single-component files (asserted in tests), but a
    // filename must never be able to write outside the cache directory.
    if zip.file.is_empty() || zip.file.contains('/') || zip.file.contains("..") {
        return Err(HalFetchError::UnsafeFile {
            file: zip.file.to_string(),
        });
    }
    let dest = out.join(zip.file);
    if dest.is_file() {
        return Ok(HalZipStat {
            model: zip.model,
            file: zip.file,
            bytes: zip.bytes,
            downloaded: false,
        });
    }
    std::fs::create_dir_all(out).map_err(|source| HalFetchError::Dir {
        dir: out.to_path_buf(),
        source,
    })?;
    let temp = dest.with_extension("part");
    match download_verified(client, zip, &temp) {
        Ok(()) => {
            std::fs::rename(&temp, &dest).map_err(|source| HalFetchError::Write {
                path: dest.clone(),
                source,
            })?;
            Ok(HalZipStat {
                model: zip.model,
                file: zip.file,
                bytes: zip.bytes,
                downloaded: true,
            })
        }
        Err(err) => {
            // Never leave a partial `.part`: the skip-if-present check trusts the *final* name,
            // and a stale temp only confuses a human.
            let _ = std::fs::remove_file(&temp);
            Err(err)
        }
    }
}

/// Stream one zip into `temp`, hashing as it goes, and fail unless both the byte count and the
/// SHA-256 match the pinned manifest. Leaves `temp` in place on error for the caller to remove.
fn download_verified(client: &dyn HalHttp, zip: &HalZip, temp: &Path) -> Result<(), HalFetchError> {
    let file = File::create(temp).map_err(|source| HalFetchError::Write {
        path: temp.to_path_buf(),
        source,
    })?;
    let mut writer = HashingWriter::new(std::io::BufWriter::new(file));
    let url = resolve_url(zip);
    let written = client
        .get_to(&url, zip.bytes, &mut writer)
        .map_err(HalFetchError::Http)?;
    writer.flush().map_err(|source| HalFetchError::Write {
        path: temp.to_path_buf(),
        source,
    })?;
    let got = writer.digest_hex();
    // Size first: a short read (truncated transfer) is the common failure and gives the clearest
    // signal; the hash then guards against a same-length but corrupted or wrong body.
    if written != zip.bytes {
        return Err(HalFetchError::ShortRead {
            file: zip.file,
            expected: zip.bytes,
            got: written,
        });
    }
    if got != zip.sha256 {
        return Err(HalFetchError::Sha256 {
            file: zip.file,
            expected: zip.sha256,
            got,
        });
    }
    Ok(())
}

/// Record what the cache was built from, beside the cache itself (BENCHMARK.md honesty rule):
/// the pinned dataset + revision, the GAIA-lineage notice, and every zip's file + SHA-256.
fn write_hal_provenance(out: &Path, zips: &[HalZip]) -> Result<(), HalFetchError> {
    let doc = HalProvenanceDoc {
        fetched_with: concat!("amberfork-bench hal-fetch v", env!("CARGO_PKG_VERSION")),
        source: "Hugging Face",
        dataset: HAL_DATASET,
        revision: HAL_REVISION,
        notice: HAL_NOTICE,
        zips: zips
            .iter()
            .map(|zip| HalProvenanceZip {
                model: zip.model,
                file: zip.file,
                bytes: zip.bytes,
                sha256: zip.sha256,
            })
            .collect(),
    };
    let mut json = serde_json::to_string_pretty(&doc).map_err(HalFetchError::Encode)?;
    json.push('\n');
    let path = out.join("provenance.json");
    std::fs::write(&path, json).map_err(|source| HalFetchError::Write { path, source })
}

#[derive(Serialize)]
struct HalProvenanceDoc<'a> {
    fetched_with: &'static str,
    source: &'static str,
    dataset: &'static str,
    revision: &'static str,
    notice: &'static str,
    zips: Vec<HalProvenanceZip<'a>>,
}

#[derive(Serialize)]
struct HalProvenanceZip<'a> {
    model: &'a str,
    file: &'a str,
    bytes: u64,
    sha256: &'a str,
}

/// Everything that can go wrong fetching a reference zip. Each stops the run: the operator's
/// network, disk, or this crate's own manifest needs fixing — loudly, never a partial or
/// unverified cache that looks whole.
#[derive(Debug)]
pub enum HalFetchError {
    /// A `GET` failed (transport error, non-2xx status, or a write into the sink failed).
    Http(HttpError),
    /// A manifest filename would escape the cache directory.
    UnsafeFile { file: String },
    /// The cache directory could not be created.
    Dir {
        dir: PathBuf,
        source: std::io::Error,
    },
    /// A `.part`, final file, or the provenance record could not be written or renamed.
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The download ended short of the pinned size (a truncated transfer).
    ShortRead {
        file: &'static str,
        expected: u64,
        got: u64,
    },
    /// The downloaded bytes did not match the pinned SHA-256 (a corrupted or wrong body).
    Sha256 {
        file: &'static str,
        expected: &'static str,
        got: String,
    },
    /// The provenance record could not be encoded.
    Encode(serde_json::Error),
}

impl fmt::Display for HalFetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(HttpError { url, msg }) => write!(f, "GET {url}: {msg}"),
            Self::UnsafeFile { file } => {
                write!(f, "refusing manifest filename outside the cache: {file}")
            }
            Self::Dir { dir, source } => write!(f, "directory {}: {source}", dir.display()),
            Self::Write { path, source } => write!(f, "write {}: {source}", path.display()),
            Self::ShortRead {
                file,
                expected,
                got,
            } => write!(
                f,
                "{file}: truncated download — expected {expected} bytes, got {got}"
            ),
            Self::Sha256 {
                file,
                expected,
                got,
            } => write!(
                f,
                "{file}: sha256 mismatch — expected {expected}, got {got}"
            ),
            Self::Encode(source) => write!(f, "encode provenance: {source}"),
        }
    }
}

impl std::error::Error for HalFetchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Dir { source, .. } | Self::Write { source, .. } => Some(source),
            Self::Encode(source) => Some(source),
            Self::Http(_)
            | Self::UnsafeFile { .. }
            | Self::ShortRead { .. }
            | Self::Sha256 { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // A known-answer vector generated once by Python's `cryptography` (the same library HAL's
    // `hal-decrypt.sh` uses), so this offline test pins the Rust path to HAL's real byte format
    // on *both* layers — the PBKDF2 key derivation and the Fernet decryption — not merely to
    // itself. Reproduce with:
    //   salt = base64.b64decode(SALT_B64)
    //   key  = base64.urlsafe_b64encode(hashlib.pbkdf2_hmac("sha256", b"hal1234", salt, 480000))
    //   tok  = cryptography.fernet.Fernet(key).encrypt(PLAINTEXT)
    //   ENCRYPTED_DATA = base64.b64encode(tok)
    const SALT_B64: &str = "YW1iZXJmb3JrLXNhbHQtMDE=";
    const ENCRYPTED_DATA_B64: &str = "Z0FBQUFBQnFaTHVsTVk1OGRyRlRzWk1DWXVpM0l3VFBoZC1CMHBzeXZhb3RCS090RWRMUEQ5LUxMUXExTHJLZGtUaXN4S3pwaTljdUdSNmhsNVViSEdxYU5TYkJhQXVPX1l4T1Vob3h1QW5PWEFKc3BfcjJsR3c9";
    const EXPECTED_PLAINTEXT: &str = r#"{"hello":"world"}"#;
    /// The Fernet key the recipe must derive from `(SALT_B64, "hal1234", 480000)` — the key
    /// derivation pinned on its own, so a PBKDF2 regression localizes here rather than hiding as
    /// a generic decrypt failure.
    const EXPECTED_KEY: &str = "_ENlHHDB4sNSh-cb10udmsQaptKoJ7T60zm0MImqCA0=";

    /// The `{salt, encrypted_data}` envelope JSON for the KAT vector above.
    fn kat_envelope() -> String {
        format!(r#"{{"salt":"{SALT_B64}","encrypted_data":"{ENCRYPTED_DATA_B64}"}}"#)
    }

    /// Build an in-memory zip carrying one member `name` with `content`. Uses the crate's
    /// default (Deflated) compression, so this also exercises `decrypt_traces`' decompression.
    fn zip_with(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buf));
            let options = zip::write::SimpleFileOptions::default();
            for (name, content) in members {
                writer.start_file(*name, options).expect("start member");
                writer.write_all(content).expect("write member");
            }
            writer.finish().expect("finish zip");
        }
        buf
    }

    #[test]
    fn decrypts_the_python_cryptography_known_answer() {
        // The whole recipe end-to-end against a real Python-`cryptography` envelope: zip ->
        // member -> PBKDF2 -> Fernet -> plaintext. If this passes, the Rust path matches HAL's.
        let zip = zip_with(&[("traces.json.encrypted", kat_envelope().as_bytes())]);
        let plaintext = decrypt_traces(&zip, HAL_PASSWORD).expect("KAT decrypts");
        assert_eq!(plaintext, EXPECTED_PLAINTEXT.as_bytes());
    }

    #[test]
    fn derives_the_pinned_fernet_key() {
        // Key derivation in isolation: the base64 Fernet key must equal Python's, so a PBKDF2
        // parameter drift (rounds, hash, salt handling, url-safe alphabet) fails *here*.
        let salt = STANDARD.decode(SALT_B64).expect("salt decodes");
        let derived = pbkdf2::pbkdf2_hmac_array::<Sha256, 32>(
            HAL_PASSWORD.as_bytes(),
            &salt,
            HAL_PBKDF2_ROUNDS,
        );
        assert_eq!(URL_SAFE.encode(derived), EXPECTED_KEY);
    }

    #[test]
    fn rejects_a_wrong_password() {
        // A wrong password derives a wrong key; Fernet's MAC must reject it rather than return
        // plausible bytes.
        let zip = zip_with(&[("traces.json.encrypted", kat_envelope().as_bytes())]);
        assert!(matches!(
            decrypt_traces(&zip, "not-the-password"),
            Err(HalDecryptError::Decrypt)
        ));
    }

    #[test]
    fn rejects_a_zip_without_the_encrypted_member() {
        let zip = zip_with(&[("readme.txt", b"no encrypted member here")]);
        assert!(matches!(
            decrypt_traces(&zip, HAL_PASSWORD),
            Err(HalDecryptError::Member { count: 0 })
        ));
    }

    #[test]
    fn rejects_more_than_one_encrypted_member() {
        // HAL's format is exactly one dump per zip; two is ambiguous, not a pick-the-first.
        let env = kat_envelope();
        let zip = zip_with(&[
            ("a.json.encrypted", env.as_bytes()),
            ("b.json.encrypted", env.as_bytes()),
        ]);
        assert!(matches!(
            decrypt_traces(&zip, HAL_PASSWORD),
            Err(HalDecryptError::Member { count: 2 })
        ));
    }

    #[test]
    fn rejects_a_member_that_is_not_an_envelope() {
        let zip = zip_with(&[("traces.json.encrypted", br#"{"not":"an envelope"}"#)]);
        assert!(matches!(
            decrypt_traces(&zip, HAL_PASSWORD),
            Err(HalDecryptError::Envelope(_))
        ));
    }

    #[test]
    fn rejects_a_non_base64_salt() {
        let member =
            format!(r#"{{"salt":"!!not base64!!","encrypted_data":"{ENCRYPTED_DATA_B64}"}}"#);
        let zip = zip_with(&[("traces.json.encrypted", member.as_bytes())]);
        assert!(matches!(
            decrypt_traces(&zip, HAL_PASSWORD),
            Err(HalDecryptError::Base64 { field: "salt", .. })
        ));
    }

    #[test]
    fn rejects_bytes_that_are_not_a_zip() {
        assert!(matches!(
            decrypt_traces(b"plainly not a zip archive", HAL_PASSWORD),
            Err(HalDecryptError::Zip(_))
        ));
    }

    // -----------------------------------------------------------------------------------------
    // Fetch tests (#41 S4b slice 2). All offline except the one `#[ignore]`d live pull.
    // -----------------------------------------------------------------------------------------

    /// A fixed payload standing in for a reference zip's bytes, and its real SHA-256 (computed
    /// once with Python's `hashlib`, pinned here the same way the decrypt KAT vector is), so the
    /// verify path is exercised against a genuine content hash, never a self-referential one.
    const TEST_PAYLOAD: &[u8] = b"amberfork-bench hal-fetch offline test payload";
    const TEST_SHA256: &str = "ec568329ac23ace8cd5ec11a93b6370be100cb583d09ff80bcda12a0051552f4";

    /// Canned-response [`HalHttp`] with a request log, so orchestration tests assert exactly which
    /// URLs were hit — and, like the real client, never write more than `max_bytes` into the sink.
    struct FakeHalHttp {
        responses: std::collections::HashMap<String, Vec<u8>>,
        requests: std::cell::RefCell<Vec<String>>,
    }

    impl FakeHalHttp {
        fn new(responses: &[(&str, &[u8])]) -> Self {
            Self {
                responses: responses
                    .iter()
                    .map(|(url, body)| ((*url).to_string(), (*body).to_vec()))
                    .collect(),
                requests: std::cell::RefCell::new(Vec::new()),
            }
        }

        fn requested(&self) -> Vec<String> {
            self.requests.borrow().clone()
        }
    }

    impl HalHttp for FakeHalHttp {
        fn get_to(
            &self,
            url: &str,
            max_bytes: u64,
            sink: &mut dyn Write,
        ) -> Result<u64, HttpError> {
            self.requests.borrow_mut().push(url.to_string());
            let body = self.responses.get(url).ok_or_else(|| HttpError {
                url: url.to_string(),
                msg: "no canned response".to_string(),
            })?;
            let cap = usize::try_from(max_bytes).unwrap_or(usize::MAX);
            let take = body.len().min(cap);
            sink.write_all(&body[..take]).map_err(|err| HttpError {
                url: url.to_string(),
                msg: err.to_string(),
            })?;
            Ok(take as u64)
        }
    }

    /// A synthetic reference zip pointing at a `_UPLOAD.zip` filename, with a caller-chosen size
    /// and pinned hash so each verify-path test can force exactly one condition.
    fn test_zip(sha256: &'static str, bytes: u64) -> HalZip {
        HalZip {
            model: "test-model",
            file: "test_reference_UPLOAD.zip",
            bytes,
            sha256,
        }
    }

    #[test]
    fn hal_manifest_is_well_formed() {
        assert_eq!(HAL_ODR_ZIPS.len(), 16);
        assert_eq!(HAL_REVISION.len(), 40, "revision must be a full sha");
        assert!(
            HAL_REVISION.chars().all(|c| c.is_ascii_hexdigit()),
            "revision must be hex"
        );
        let mut files = std::collections::BTreeSet::new();
        let mut models = std::collections::BTreeSet::new();
        let mut prev = 0u64;
        for zip in &HAL_ODR_ZIPS {
            assert!(files.insert(zip.file), "duplicate file {}", zip.file);
            assert!(models.insert(zip.model), "duplicate model {}", zip.model);
            assert!(
                zip.file.starts_with("gaia_hf_open_deep_research"),
                "unexpected file {}",
                zip.file
            );
            assert!(
                zip.file.ends_with("_UPLOAD.zip"),
                "unexpected file {}",
                zip.file
            );
            assert!(
                !zip.file.contains('/') && !zip.file.contains(".."),
                "file must be a single component: {}",
                zip.file
            );
            assert_eq!(zip.sha256.len(), 64, "sha256 is 64 hex chars: {}", zip.file);
            assert!(
                zip.sha256
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "sha256 must be lowercase hex: {}",
                zip.file
            );
            assert!(zip.bytes > 0, "{} has zero size", zip.file);
            assert!(
                zip.bytes >= prev,
                "manifest must be sorted smallest-first: {}",
                zip.file
            );
            prev = zip.bytes;
        }
    }

    #[test]
    fn resolve_url_has_the_pinned_shape() {
        assert_eq!(
            resolve_url(&HAL_ODR_ZIPS[0]),
            "https://huggingface.co/datasets/agent-evals/hal_traces/resolve/\
             e7dcedc82b4f4bc819a170fd6616bdb44841c71e/\
             gaia_hf_open_deep_research_o3mini20250131_high_1744843485_UPLOAD.zip"
        );
    }

    #[test]
    fn fetch_hal_zip_downloads_verifies_and_lands() {
        let zip = test_zip(TEST_SHA256, TEST_PAYLOAD.len() as u64);
        let url = resolve_url(&zip);
        let fake = FakeHalHttp::new(&[(url.as_str(), TEST_PAYLOAD)]);
        let out = tempfile::tempdir().expect("tempdir");

        let stat = fetch_hal_zip(&fake, &zip, out.path()).expect("fetch succeeds");

        assert!(stat.downloaded);
        assert_eq!(stat.file, zip.file);
        assert_eq!(
            std::fs::read(out.path().join(zip.file)).expect("landed"),
            TEST_PAYLOAD
        );
        assert!(
            !out.path().join("test_reference_UPLOAD.part").exists(),
            "no leftover .part after a clean land"
        );
        assert_eq!(fake.requested(), vec![url]);
    }

    #[test]
    fn fetch_hal_zip_skips_a_cached_file() {
        let zip = test_zip(TEST_SHA256, TEST_PAYLOAD.len() as u64);
        let out = tempfile::tempdir().expect("tempdir");
        std::fs::write(out.path().join(zip.file), b"already here").expect("precache");
        let fake = FakeHalHttp::new(&[]);

        let stat = fetch_hal_zip(&fake, &zip, out.path()).expect("skip succeeds");

        assert!(
            !stat.downloaded,
            "a cached zip is reported as not downloaded"
        );
        assert!(
            fake.requested().is_empty(),
            "a cached zip is never re-fetched"
        );
        assert_eq!(
            std::fs::read(out.path().join(zip.file)).expect("kept"),
            b"already here",
            "a cached file is never overwritten"
        );
    }

    #[test]
    fn fetch_hal_zip_rejects_a_sha_mismatch() {
        // Right length, wrong pinned hash: the body is not what the manifest promised.
        let zip = test_zip(
            "0000000000000000000000000000000000000000000000000000000000000000",
            TEST_PAYLOAD.len() as u64,
        );
        let url = resolve_url(&zip);
        let fake = FakeHalHttp::new(&[(url.as_str(), TEST_PAYLOAD)]);
        let out = tempfile::tempdir().expect("tempdir");

        let err = fetch_hal_zip(&fake, &zip, out.path()).expect_err("must reject");

        assert!(matches!(err, HalFetchError::Sha256 { .. }));
        assert!(
            !out.path().join(zip.file).exists(),
            "an unverified body never lands under the final name"
        );
        assert!(!out.path().join("test_reference_UPLOAD.part").exists());
    }

    #[test]
    fn fetch_hal_zip_rejects_a_truncated_download() {
        // The manifest promises more bytes than the server delivers.
        let zip = test_zip(TEST_SHA256, TEST_PAYLOAD.len() as u64 + 100);
        let url = resolve_url(&zip);
        let fake = FakeHalHttp::new(&[(url.as_str(), TEST_PAYLOAD)]);
        let out = tempfile::tempdir().expect("tempdir");

        let err = fetch_hal_zip(&fake, &zip, out.path()).expect_err("must reject");

        assert!(matches!(err, HalFetchError::ShortRead { .. }));
        assert!(!out.path().join(zip.file).exists());
        assert!(!out.path().join("test_reference_UPLOAD.part").exists());
    }

    #[test]
    fn fetch_hal_zip_fails_loudly_on_http_error() {
        let zip = test_zip(TEST_SHA256, TEST_PAYLOAD.len() as u64);
        let fake = FakeHalHttp::new(&[]); // no canned response for the resolve URL
        let out = tempfile::tempdir().expect("tempdir");

        let err = fetch_hal_zip(&fake, &zip, out.path()).expect_err("must fail");

        assert!(matches!(err, HalFetchError::Http(_)));
        assert!(
            !out.path().join("test_reference_UPLOAD.part").exists(),
            "a failed download leaves no partial file"
        );
    }

    #[test]
    fn fetch_hal_zips_writes_provenance() {
        let zip_a = HalZip {
            model: "model-a",
            file: "ref_a_UPLOAD.zip",
            bytes: TEST_PAYLOAD.len() as u64,
            sha256: TEST_SHA256,
        };
        let zip_b = HalZip {
            model: "model-b",
            file: "ref_b_UPLOAD.zip",
            bytes: TEST_PAYLOAD.len() as u64,
            sha256: TEST_SHA256,
        };
        let fake = FakeHalHttp::new(&[
            (resolve_url(&zip_a).as_str(), TEST_PAYLOAD),
            (resolve_url(&zip_b).as_str(), TEST_PAYLOAD),
        ]);
        let out = tempfile::tempdir().expect("tempdir");

        let stats = fetch_hal_zips(&fake, &[zip_a, zip_b], out.path()).expect("fetch succeeds");

        assert_eq!(stats.len(), 2);
        let prov = std::fs::read_to_string(out.path().join("provenance.json"))
            .expect("provenance written");
        assert!(prov.contains(HAL_REVISION), "records the pinned revision");
        assert!(
            prov.contains("ref_a_UPLOAD.zip") && prov.contains("ref_b_UPLOAD.zip"),
            "records each fetched file"
        );
        assert!(prov.contains(TEST_SHA256), "records the content hash");
        assert!(out.path().join("ref_a_UPLOAD.zip").is_file());
        assert!(out.path().join("ref_b_UPLOAD.zip").is_file());
    }

    /// The operator's end-to-end check: pull the smallest real reference zip and run the whole
    /// path this slice exists to enable — fetch (with SHA-256 verify) → decrypt → canonical Runs.
    #[test]
    #[ignore = "network: pulls the ~106 MB smallest reference zip from Hugging Face"]
    fn network_fetch_smallest_zip_decrypts_and_converts() {
        let out = tempfile::tempdir().expect("tempdir");
        let smallest = &HAL_ODR_ZIPS[0];

        let stat = fetch_hal_zip(&HfClient, smallest, out.path()).expect("live fetch works");
        assert!(stat.downloaded);

        let zip = std::fs::read(out.path().join(smallest.file)).expect("zip cached");
        let json = decrypt_traces(&zip, HAL_PASSWORD).expect("decrypts");
        let runs = amberfork_ingest::hal::convert_str(
            std::str::from_utf8(&json).expect("decrypted dump is utf-8"),
        )
        .expect("converts to canonical Runs");
        assert!(!runs.is_empty(), "at least one GAIA task");
        assert!(
            runs.iter().any(|converted| converted.meta.passed),
            "at least one passing reference run"
        );
        assert!(
            runs.iter().any(|converted| !converted.run.steps.is_empty()),
            "reference runs carry turns"
        );
    }
}
