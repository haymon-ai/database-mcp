//! PEM private-key block type validator.

use crate::recognizer::{ValidationOutcome, Validator};

/// PEM private-key block type validator: BEGIN-type MUST equal END-type.
#[derive(Debug, Default, Clone, Copy)]
pub struct PrivateKeyTypeValidator;

impl Validator for PrivateKeyTypeValidator {
    fn validate(&self, candidate: &str) -> ValidationOutcome {
        let Some(begin) = candidate.find("-----BEGIN ") else {
            return ValidationOutcome::Invalid;
        };
        let body_start = begin + "-----BEGIN ".len();
        let Some(begin_close) = candidate[body_start..].find("-----") else {
            return ValidationOutcome::Invalid;
        };
        let begin_label = candidate[body_start..body_start + begin_close].trim();
        let Some(end) = candidate.rfind("-----END ") else {
            return ValidationOutcome::Invalid;
        };
        let end_body_start = end + "-----END ".len();
        let Some(end_close) = candidate[end_body_start..].find("-----") else {
            return ValidationOutcome::Invalid;
        };
        let end_label = candidate[end_body_start..end_body_start + end_close].trim();
        ValidationOutcome::from_bool(begin_label == end_label && begin_label.contains("PRIVATE KEY"))
    }
}
