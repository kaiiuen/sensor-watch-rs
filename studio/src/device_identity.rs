//! Bounded device identity and profile matching.
//!
//! The transport supplies only the masked `ID UID=` fingerprint. It is useful
//! for selecting a board profile, but it is not authentication or authorization.

use serde::{Deserialize, Serialize};

pub const FINGERPRINT_HEX_LEN: usize = 32;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceIdentity {
    pub fingerprint: Option<String>,
    pub source: String,
    pub board: Option<String>,
    pub revision: Option<String>,
    pub confidence: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceProfile {
    pub fingerprint: String,
    pub board: String,
    pub revision: String,
}

impl Default for DeviceProfile {
    fn default() -> Self {
        Self {
            fingerprint: String::new(),
            board: String::new(),
            revision: String::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileMatch {
    Exact,
    ConfirmedMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentityError {
    MalformedReply,
    UnknownIdentity,
    MismatchRequiresConfirmation,
}

fn normalized(value: &str) -> String {
    value
        .trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase()
}

pub fn normalize_profile(profile: &DeviceProfile) -> DeviceProfile {
    DeviceProfile {
        fingerprint: normalized(&profile.fingerprint),
        board: normalized(&profile.board),
        revision: normalized(&profile.revision),
    }
}

/// Parse one bounded firmware identity line. Raw UID values are deliberately
/// rejected; only the fixed-size masked fingerprint is accepted.
pub fn parse_reply(reply: &str) -> Result<DeviceIdentity, IdentityError> {
    if reply.len() > 256 || !reply.starts_with("ID ") {
        return Err(IdentityError::MalformedReply);
    }
    let mut identity = DeviceIdentity::default();
    for field in reply[3..].split_whitespace() {
        let Some((key, value)) = field.split_once('=') else {
            return Err(IdentityError::MalformedReply);
        };
        match key {
            "UID"
                if value.len() == FINGERPRINT_HEX_LEN
                    && value.bytes().all(|b| b.is_ascii_hexdigit()) =>
            {
                identity.fingerprint = Some(value.to_ascii_uppercase())
            }
            "UID" => return Err(IdentityError::MalformedReply),
            "SOURCE" => identity.source = value.to_string(),
            "BOARD" if value != "unknown" => identity.board = Some(value.to_string()),
            "BOARD" => {}
            "REV" if value != "unknown" => identity.revision = Some(value.to_string()),
            "REV" => {}
            "CONF" => identity.confidence = value.to_string(),
            "NOTE" if value == "identifier-not-authentication" => {}
            _ => return Err(IdentityError::MalformedReply),
        }
    }
    if identity.fingerprint.is_none()
        || identity.source.is_empty()
        || identity.confidence.is_empty()
    {
        return Err(IdentityError::UnknownIdentity);
    }
    Ok(identity)
}

/// Match all available identity dimensions. A mismatch never silently selects
/// or overrides a profile; callers must provide an explicit confirmation.
pub fn match_profile(
    identity: &DeviceIdentity,
    profile: &DeviceProfile,
    confirmed: bool,
) -> Result<ProfileMatch, IdentityError> {
    let Some(fingerprint) = &identity.fingerprint else {
        return Err(IdentityError::UnknownIdentity);
    };
    let profile = normalize_profile(profile);
    let exact = normalized(fingerprint) == profile.fingerprint
        && identity
            .board
            .as_deref()
            .map(normalized)
            .unwrap_or_default()
            == profile.board
        && identity
            .revision
            .as_deref()
            .map(normalized)
            .unwrap_or_default()
            == profile.revision;
    if exact {
        Ok(ProfileMatch::Exact)
    } else if confirmed {
        Ok(ProfileMatch::ConfirmedMismatch)
    } else {
        Err(IdentityError::MismatchRequiresConfirmation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const REPLY: &str = "ID UID=00112233445566778899AABBCCDDEEFF SOURCE=SAM-L22-signature-row BOARD=Green REV=R2 CONF=high NOTE=identifier-not-authentication";

    #[test]
    fn parses_and_normalizes_identity() {
        let identity = parse_reply(REPLY).unwrap();
        assert_eq!(
            identity.fingerprint.as_deref(),
            Some("00112233445566778899AABBCCDDEEFF")
        );
        assert_eq!(
            normalize_profile(&DeviceProfile {
                fingerprint: " aa ".into(),
                board: "green".into(),
                revision: " r2 ".into()
            })
            .board,
            "GREEN"
        );
    }

    #[test]
    fn malformed_replies_are_rejected() {
        assert_eq!(
            parse_reply("ID UID=00 SOURCE=x CONF=y"),
            Err(IdentityError::MalformedReply)
        );
        assert_eq!(
            parse_reply("ID UID=00112233445566778899AABBCCDDEEFF SOURCE=x CONF=y EXTRA=z"),
            Err(IdentityError::MalformedReply)
        );
    }

    #[test]
    fn unknown_identity_is_not_matchable() {
        assert_eq!(parse_reply("ID SOURCE=unavailable BOARD=unknown REV=unknown CONF=unknown NOTE=identifier-not-authentication"), Err(IdentityError::UnknownIdentity));
    }

    #[test]
    fn mismatch_requires_explicit_confirmation() {
        let identity = parse_reply(REPLY).unwrap();
        let profile = DeviceProfile {
            fingerprint: identity.fingerprint.clone().unwrap(),
            board: "Blue".into(),
            revision: "R2".into(),
        };
        assert_eq!(
            match_profile(&identity, &profile, false),
            Err(IdentityError::MismatchRequiresConfirmation)
        );
        assert_eq!(
            match_profile(&identity, &profile, true),
            Ok(ProfileMatch::ConfirmedMismatch)
        );
    }

    #[test]
    fn profiles_persist_through_json() {
        let profile = DeviceProfile {
            fingerprint: "AABB".into(),
            board: "Green".into(),
            revision: "R1".into(),
        };
        let restored: DeviceProfile =
            serde_json::from_str(&serde_json::to_string(&profile).unwrap()).unwrap();
        assert_eq!(restored, profile);
    }
}
