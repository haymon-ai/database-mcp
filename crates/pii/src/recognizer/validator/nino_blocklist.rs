//! UK NINO prefix blocklist validator.

use crate::recognizer::{ValidationOutcome, Validator};

/// UK NINO blocklist validator.
///
/// Rejects the closed prefix set `{BG, GB, KN, NK, NT, TN, ZZ}` plus any
/// prefix whose second character is `O`. Suffix letter (when present) must be
/// in `{A, B, C, D}`.
#[derive(Debug, Default, Clone, Copy)]
pub struct NinoBlocklistValidator;

const NINO_BLOCKED_PREFIXES: &[&str] = &["BG", "GB", "KN", "NK", "NT", "TN", "ZZ"];

impl Validator for NinoBlocklistValidator {
    fn validate(&self, candidate: &str) -> ValidationOutcome {
        // NINO is 8 or 9 chars after stripping `-`/` `; stack-buffer the cleaned form.
        let mut buf = [0u8; 9];
        let mut len = 0usize;
        for &b in candidate.as_bytes() {
            if !b.is_ascii_alphanumeric() {
                continue;
            }
            if len == buf.len() {
                return ValidationOutcome::Invalid;
            }
            buf[len] = b.to_ascii_uppercase();
            len += 1;
        }
        if len != 8 && len != 9 {
            return ValidationOutcome::Invalid;
        }
        let cleaned = &buf[..len];
        if !cleaned[0].is_ascii_alphabetic() || !cleaned[1].is_ascii_alphabetic() || cleaned[1] == b'O' {
            return ValidationOutcome::Invalid;
        }
        let prefix = [cleaned[0], cleaned[1]];
        if NINO_BLOCKED_PREFIXES.iter().any(|p| p.as_bytes() == prefix) {
            return ValidationOutcome::Invalid;
        }
        if !cleaned[2..8].iter().all(u8::is_ascii_digit) {
            return ValidationOutcome::Invalid;
        }
        if len == 9 && !matches!(cleaned[8], b'A' | b'B' | b'C' | b'D') {
            return ValidationOutcome::Invalid;
        }
        ValidationOutcome::Valid
    }
}
