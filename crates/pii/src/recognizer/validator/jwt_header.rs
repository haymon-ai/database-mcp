//! JWT header structural validator (signature NOT verified).

use base64::Engine;

use crate::recognizer::{ValidationOutcome, Validator};

/// JWT header validator.
///
/// Accepts the candidate iff splitting on `.` yields three segments and the
/// first segment base64url-decodes to a JSON object containing a string `alg`
/// field. Does NOT verify the signature.
#[derive(Debug, Default, Clone, Copy)]
pub struct JwtHeaderValidator;

impl Validator for JwtHeaderValidator {
    fn validate(&self, candidate: &str) -> ValidationOutcome {
        let mut parts = candidate.split('.');
        let (Some(header), Some(_), Some(_), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
            return ValidationOutcome::Invalid;
        };
        let Ok(decoded) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(header) else {
            return ValidationOutcome::Invalid;
        };
        let Ok(json) = serde_json::from_slice::<serde_json::Value>(&decoded) else {
            return ValidationOutcome::Invalid;
        };
        ValidationOutcome::from_bool(matches!(json.get("alg"), Some(serde_json::Value::String(_))))
    }
}
