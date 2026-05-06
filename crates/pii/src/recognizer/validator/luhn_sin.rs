//! Luhn checksum validator gated to exactly 9 digits, used by `SIN_CA`.

use super::luhn::luhn_passes;
use super::us_ssn::collect_digits;
use crate::recognizer::{ValidationOutcome, Validator};

/// Luhn checksum gated to exactly 9 digits, used by `SIN_CA`.
///
/// The card-flavoured [`super::LuhnValidator`] requires 12–19 digits and would
/// reject a 9-digit Canadian SIN. This sibling validator runs the same Luhn
/// algorithm over exactly 9 digits.
#[derive(Debug, Default, Clone, Copy)]
pub struct LuhnSinValidator;

impl Validator for LuhnSinValidator {
    fn validate(&self, candidate: &str) -> ValidationOutcome {
        let Some(digits) = collect_digits::<9>(candidate) else {
            return ValidationOutcome::Invalid;
        };
        ValidationOutcome::from_bool(luhn_passes(digits))
    }
}
