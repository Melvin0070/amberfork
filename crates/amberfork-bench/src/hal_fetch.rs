//! Unwrapping the encrypted HAL reference-trace dumps into the plaintext JSON the
//! [`amberfork_ingest::hal`] adapter reads (issue #41 S4b).
//!
//! HAL (`hal.cs.princeton.edu`) publishes each agent config's traces as a **Fernet-encrypted
//! JSON envelope inside a zip** (one `gaia_hf_open_deep_research_<model>_…_UPLOAD.zip` per
//! backing model, on Hugging Face — notebook 047). This module turns those bytes back into the
//! decrypted traces JSON; the pinned Hugging Face *download* of the zip is the next slice, so
//! for now the input is a zip already on disk (an operator's hand-download, exactly as the S4b
//! feasibility spike did).
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
//! JSON-only loader, while decryption (and, next slice, the network fetch) is data acquisition —
//! it belongs beside [`crate::fetch`] with the zip/crypto deps, and it hands its plaintext output
//! straight to `amberfork_ingest::hal::convert_str`. Crypto is never hand-rolled: [`fernet`]
//! provides the AES-CBC + HMAC-SHA256 authenticated decryption, so a wrong password or a
//! corrupted dump fails loudly at the MAC ([`HalDecryptError::Decrypt`]) rather than returning
//! garbage.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE};
use serde::Deserialize;
use sha2::Sha256;
use std::fmt;
use std::io::{Cursor, Read};

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
}
