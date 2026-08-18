//! Fail-closed desktop release acquisition.
//!
//! This crate is deliberately independent from Studio UI and process-launching
//! code. Callers provide a bounded transport and a pinned public-key ring.
//! Release metadata is signed canonical JSON; mutable repository content is never
//! consulted as a trust root.

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fmt,
    path::{Component, Path},
};
use url::Url;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_METADATA_BYTES: usize = 256 * 1024;
pub const MAX_ARTIFACT_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}
impl Version {
    pub fn parse(s: &str) -> Result<Self, Error> {
        let mut p = s.split('.');
        let v = Self {
            major: p
                .next()
                .ok_or_else(|| Error::InvalidVersion(s.into()))?
                .parse()
                .map_err(|_| Error::InvalidVersion(s.into()))?,
            minor: p
                .next()
                .ok_or_else(|| Error::InvalidVersion(s.into()))?
                .parse()
                .map_err(|_| Error::InvalidVersion(s.into()))?,
            patch: p
                .next()
                .ok_or_else(|| Error::InvalidVersion(s.into()))?
                .parse()
                .map_err(|_| Error::InvalidVersion(s.into()))?,
        };
        if p.next().is_some() {
            return Err(Error::InvalidVersion(s.into()));
        }
        Ok(v)
    }
}
impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Artifact {
    pub url: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Release {
    pub version: Version,
    pub channel: String,
    pub expires_at: u64,
    pub artifact: Artifact,
    pub key_id: String,
    pub signature: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReleaseMetadata {
    pub schema_version: u32,
    pub generated_at: u64,
    pub releases: Vec<Release>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Authentication {
    Authenticated { key_id: String },
    Unsupported,
    Untrusted(String),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Acquisition {
    pub release: Release,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Unsupported(&'static str),
    Untrusted(String),
    Json(String),
    InvalidVersion(String),
    Oversized { what: &'static str, limit: usize },
    Transport(String),
    InvalidUrl,
    Expired,
    NoCompatibleRelease,
    Downgrade,
    PathTraversal(String),
    SizeMismatch { expected: u64, actual: u64 },
    HashMismatch { expected: String, actual: String },
    InvalidHash,
    InvalidSignatureEncoding,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

/// A pinned key ring. Retired keys are rejected unless rollback/rotation is
/// explicitly enabled by the caller; unknown keys are always rejected.
#[derive(Clone, Debug, Default)]
pub struct KeyRing {
    active: BTreeMap<String, VerifyingKey>,
    retired: BTreeMap<String, VerifyingKey>,
    allow_retired: bool,
}
impl KeyRing {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn pin_active(mut self, id: impl Into<String>, key: [u8; 32]) -> Result<Self, Error> {
        self.active.insert(
            id.into(),
            VerifyingKey::from_bytes(&key)
                .map_err(|_| Error::Untrusted("invalid public key".into()))?,
        );
        Ok(self)
    }
    pub fn pin_retired(mut self, id: impl Into<String>, key: [u8; 32]) -> Result<Self, Error> {
        self.retired.insert(
            id.into(),
            VerifyingKey::from_bytes(&key)
                .map_err(|_| Error::Untrusted("invalid public key".into()))?,
        );
        Ok(self)
    }
    pub fn allow_retired(mut self, allow: bool) -> Self {
        self.allow_retired = allow;
        self
    }
    fn key(&self, id: &str) -> Result<(&VerifyingKey, bool), Authentication> {
        if let Some(k) = self.active.get(id) {
            return Ok((k, false));
        }
        if self.allow_retired {
            if let Some(k) = self.retired.get(id) {
                return Ok((k, true));
            }
        }
        Err(Authentication::Untrusted(format!(
            "key {id} is not pinned for this policy"
        )))
    }
}

/// Authenticates metadata without ever accepting a private key or repository file.
pub fn authenticate(
    metadata: &[u8],
    keys: &KeyRing,
) -> Result<(ReleaseMetadata, Authentication), Error> {
    if keys.active.is_empty() && (!keys.allow_retired || keys.retired.is_empty()) {
        return Err(Error::Unsupported("no pinned public key configured"));
    }
    if metadata.len() > MAX_METADATA_BYTES {
        return Err(Error::Oversized {
            what: "metadata",
            limit: MAX_METADATA_BYTES,
        });
    }
    let parsed: ReleaseMetadata =
        serde_json::from_slice(metadata).map_err(|e| Error::Json(e.to_string()))?;
    if parsed.schema_version != SCHEMA_VERSION {
        return Err(Error::Untrusted(format!(
            "unsupported schema {}",
            parsed.schema_version
        )));
    }
    if parsed.releases.is_empty() {
        return Err(Error::Untrusted("metadata has no releases".into()));
    }
    for release in &parsed.releases {
        let payload = signed_payload(release)?;
        let key = match keys.key(&release.key_id) {
            Ok((k, _)) => k,
            Err(Authentication::Untrusted(e)) => return Err(Error::Untrusted(e)),
            Err(Authentication::Unsupported) => unreachable!(),
            Err(Authentication::Authenticated { .. }) => unreachable!(),
        };
        let raw = B64
            .decode(&release.signature)
            .map_err(|_| Error::InvalidSignatureEncoding)?;
        let sig = Signature::from_slice(&raw).map_err(|_| Error::InvalidSignatureEncoding)?;
        key.verify(&payload, &sig)
            .map_err(|_| Error::Untrusted(format!("invalid signature for {}", release.version)))?;
        validate_artifact(&release.artifact)?;
    }
    Ok((
        parsed,
        Authentication::Authenticated {
            key_id: "metadata-key-ring".into(),
        },
    ))
}

fn signed_payload(r: &Release) -> Result<Vec<u8>, Error> {
    serde_json::to_vec(&SignedRelease {
        version: &r.version,
        channel: &r.channel,
        expires_at: r.expires_at,
        artifact: &r.artifact,
        key_id: &r.key_id,
    })
    .map_err(|e| Error::Json(e.to_string()))
}
#[derive(Serialize)]
struct SignedRelease<'a> {
    version: &'a Version,
    channel: &'a str,
    expires_at: u64,
    artifact: &'a Artifact,
    key_id: &'a str,
}
fn validate_artifact(a: &Artifact) -> Result<(), Error> {
    if a.size > MAX_ARTIFACT_BYTES as u64 {
        return Err(Error::Oversized {
            what: "artifact",
            limit: MAX_ARTIFACT_BYTES,
        });
    }
    if a.sha256.len() != 64 || !a.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::InvalidHash);
    }
    let u = Url::parse(&a.url).map_err(|_| Error::InvalidUrl)?;
    if u.scheme() != "https" && u.scheme() != "file" {
        return Err(Error::InvalidUrl);
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct SelectionPolicy {
    pub channel: String,
    pub current: Option<Version>,
    pub allow_downgrade: bool,
    pub now: u64,
}
pub fn select(meta: &ReleaseMetadata, policy: &SelectionPolicy) -> Result<Release, Error> {
    let candidates = meta.releases.iter().filter(|r| {
        r.channel == policy.channel
            && r.expires_at >= policy.now
            && (policy.allow_downgrade
                || match policy.current.as_ref() {
                    None => true,
                    Some(v) => r.version >= *v,
                })
    });
    let best = candidates
        .max_by(|a, b| a.version.cmp(&b.version))
        .cloned()
        .ok_or(Error::NoCompatibleRelease)?;
    if !policy.allow_downgrade && policy.current.as_ref().is_some_and(|v| best.version < *v) {
        return Err(Error::Downgrade);
    }
    Ok(best)
}

pub trait Transport {
    fn get(&self, url: &Url, max_bytes: usize) -> Result<Vec<u8>, Error>;
}
pub struct BoundedFileTransport;
impl Transport for BoundedFileTransport {
    fn get(&self, url: &Url, max: usize) -> Result<Vec<u8>, Error> {
        let path = url.to_file_path().map_err(|_| Error::InvalidUrl)?;
        let length = std::fs::metadata(&path)
            .map_err(|e| Error::Transport(e.to_string()))?
            .len();
        if length > max as u64 {
            return Err(Error::Oversized {
                what: "response",
                limit: max,
            });
        }
        let data = std::fs::read(path).map_err(|e| Error::Transport(e.to_string()))?;
        if data.len() > max {
            return Err(Error::Oversized {
                what: "response",
                limit: max,
            });
        }
        Ok(data)
    }
}
pub struct BoundedHttpsTransport {
    pub max_response_bytes: usize,
    pub timeout_seconds: u64,
}
impl Transport for BoundedHttpsTransport {
    fn get(&self, url: &Url, max: usize) -> Result<Vec<u8>, Error> {
        if url.scheme() != "https" {
            return Err(Error::InvalidUrl);
        }
        let limit = max.min(self.max_response_bytes);
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(self.timeout_seconds))
            .build();
        let response = agent
            .get(url.as_str())
            .call()
            .map_err(|e| Error::Transport(e.to_string()))?;
        if let Some(length) = response
            .header("Content-Length")
            .and_then(|v| v.parse::<usize>().ok())
        {
            if length > limit {
                return Err(Error::Oversized {
                    what: "response",
                    limit,
                });
            }
        }
        let mut reader = response.into_reader();
        let mut data = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            let read = std::io::Read::read(&mut reader, &mut buffer)
                .map_err(|e| Error::Transport(e.to_string()))?;
            if read == 0 {
                break;
            }
            if data.len().saturating_add(read) > limit {
                return Err(Error::Oversized {
                    what: "response",
                    limit,
                });
            }
            data.extend_from_slice(&buffer[..read]);
        }
        Ok(data)
    }
}

pub fn verify_artifact(bytes: &[u8], artifact: &Artifact) -> Result<(), Error> {
    validate_artifact(artifact)?;
    if bytes.len() as u64 != artifact.size {
        return Err(Error::SizeMismatch {
            expected: artifact.size,
            actual: bytes.len() as u64,
        });
    }
    let actual = hex(&Sha256::digest(bytes));
    if actual != artifact.sha256.to_ascii_lowercase() {
        return Err(Error::HashMismatch {
            expected: artifact.sha256.clone(),
            actual,
        });
    }
    Ok(())
}
pub fn acquire<T: Transport>(transport: &T, release: Release) -> Result<Acquisition, Error> {
    let url = Url::parse(&release.artifact.url).map_err(|_| Error::InvalidUrl)?;
    let bytes = transport.get(&url, MAX_ARTIFACT_BYTES)?;
    verify_artifact(&bytes, &release.artifact)?;
    Ok(Acquisition { release, bytes })
}

pub fn validate_relative_path(path: &Path) -> Result<(), Error> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        Err(Error::PathTraversal(path.display().to_string()))
    } else {
        Ok(())
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    fn release(key: &SigningKey, version: Version, key_id: &str) -> Release {
        let artifact = Artifact {
            url: "file:///safe/update.bin".into(),
            size: 3,
            sha256: hex(&Sha256::digest(b"abc")),
        };
        let mut r = Release {
            version,
            channel: "stable".into(),
            expires_at: 100,
            artifact,
            key_id: key_id.into(),
            signature: String::new(),
        };
        r.signature = B64.encode(key.sign(&signed_payload(&r).unwrap()).to_bytes());
        r
    }
    #[test]
    fn fixture_metadata_and_selection_are_deterministic() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let r = release(&key, Version::parse("1.2.3").unwrap(), "current");
        let m = ReleaseMetadata {
            schema_version: 1,
            generated_at: 1,
            releases: vec![r.clone()],
        };
        let raw = serde_json::to_vec(&m).unwrap();
        let ring = KeyRing::new()
            .pin_active("current", key.verifying_key().to_bytes())
            .unwrap();
        assert!(matches!(
            authenticate(&raw, &ring).unwrap().1,
            Authentication::Authenticated { .. }
        ));
        assert_eq!(
            select(
                &m,
                &SelectionPolicy {
                    channel: "stable".into(),
                    current: Some(Version::parse("1.0.0").unwrap()),
                    allow_downgrade: false,
                    now: 2
                }
            )
            .unwrap()
            .version,
            r.version
        );
    }
    #[test]
    fn no_key_is_unsupported_and_bad_signature_untrusted() {
        let key = SigningKey::from_bytes(&[8; 32]);
        let mut r = release(&key, Version::parse("1.0.0").unwrap(), "k");
        let m = ReleaseMetadata {
            schema_version: 1,
            generated_at: 1,
            releases: vec![r.clone()],
        };
        assert!(matches!(
            authenticate(&serde_json::to_vec(&m).unwrap(), &KeyRing::new()),
            Err(Error::Unsupported(_))
        ));
        r.signature = B64.encode([0u8; 64]);
        let m = ReleaseMetadata {
            releases: vec![r],
            ..m
        };
        let ring = KeyRing::new()
            .pin_active("k", key.verifying_key().to_bytes())
            .unwrap();
        assert!(matches!(
            authenticate(&serde_json::to_vec(&m).unwrap(), &ring),
            Err(Error::Untrusted(_))
        ));
    }
    #[test]
    fn expiry_downgrade_and_path_policy_are_fail_closed() {
        let key = SigningKey::from_bytes(&[9; 32]);
        let m = ReleaseMetadata {
            schema_version: 1,
            generated_at: 1,
            releases: vec![release(&key, Version::parse("1.0.0").unwrap(), "k")],
        };
        let p = SelectionPolicy {
            channel: "stable".into(),
            current: Some(Version::parse("2.0.0").unwrap()),
            allow_downgrade: false,
            now: 101,
        };
        assert!(matches!(select(&m, &p), Err(Error::NoCompatibleRelease)));
        assert!(matches!(
            validate_relative_path(Path::new("../escape")),
            Err(Error::PathTraversal(_))
        ));
    }
    #[test]
    fn rotation_requires_explicit_retired_policy() {
        let old = SigningKey::from_bytes(&[1; 32]);
        let r = release(&old, Version::parse("1.0.0").unwrap(), "old");
        let m = ReleaseMetadata {
            schema_version: 1,
            generated_at: 1,
            releases: vec![r],
        };
        let raw = serde_json::to_vec(&m).unwrap();
        let ring = KeyRing::new()
            .pin_retired("old", old.verifying_key().to_bytes())
            .unwrap();
        assert!(matches!(
            authenticate(&raw, &ring),
            Err(Error::Unsupported(_))
        ));
        let ring = KeyRing::new()
            .pin_retired("old", old.verifying_key().to_bytes())
            .unwrap()
            .allow_retired(true);
        assert!(authenticate(&raw, &ring).is_ok());
    }
    #[test]
    fn malformed_and_oversized_artifacts_are_rejected() {
        let key = SigningKey::from_bytes(&[6; 32]);
        assert!(matches!(
            authenticate(
                &vec![b' '; MAX_METADATA_BYTES + 1],
                &KeyRing::new()
                    .pin_active("k", key.verifying_key().to_bytes())
                    .unwrap()
            ),
            Err(Error::Oversized {
                what: "metadata",
                ..
            })
        ));
        assert!(matches!(
            validate_artifact(&Artifact {
                url: "http://x".into(),
                size: 1,
                sha256: "0".repeat(64)
            }),
            Err(Error::InvalidUrl)
        ));
        assert!(matches!(
            validate_artifact(&Artifact {
                url: "file:///x".into(),
                size: (MAX_ARTIFACT_BYTES as u64) + 1,
                sha256: "0".repeat(64)
            }),
            Err(Error::Oversized { .. })
        ));
        assert!(matches!(
            Version::parse("1.2"),
            Err(Error::InvalidVersion(_))
        ));
    }
}
