//! UK NHS-number mod-11 checksum validator.

use super::us_ssn::collect_digits;
use crate::recognizer::{ValidationOutcome, Validator};

/// UK NHS number mod-11 validator.
///
/// Strips spaces / dashes; expects exactly 10 digits. Weights `[10..=2]` over
/// the first 9 digits; check digit = `(11 - sum%11) % 11`. A computed check
/// of `10` invalidates per the NHS specification.
#[derive(Debug, Default, Clone, Copy)]
pub struct Mod11NhsValidator;

impl Validator for Mod11NhsValidator {
    fn validate(&self, candidate: &str) -> ValidationOutcome {
        let Some(digits) = collect_digits::<10>(candidate) else {
            return ValidationOutcome::Invalid;
        };
        let sum: u32 = digits[..9]
            .iter()
            .zip([10u32, 9, 8, 7, 6, 5, 4, 3, 2])
            .map(|(d, w)| d * w)
            .sum();
        let remainder = sum % 11;
        let check = match remainder {
            0 => 0,
            10 => return ValidationOutcome::Invalid,
            n => 11 - n,
        };
        ValidationOutcome::from_bool(check == digits[9])
    }
}
