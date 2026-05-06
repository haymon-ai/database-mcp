//! US ITIN middle-block range validator.

use super::us_ssn::collect_digits;
use crate::recognizer::{ValidationOutcome, Validator};

/// US ITIN middle-block range validator.
///
/// Format `9XX-NN-NNNN`. Middle digits MUST be in `70-88 ∪ 90-92 ∪ 94-99` per IRS rules.
#[derive(Debug, Default, Clone, Copy)]
pub struct ItinRangeValidator;

impl Validator for ItinRangeValidator {
    fn validate(&self, candidate: &str) -> ValidationOutcome {
        let Some(d) = collect_digits::<9>(candidate) else {
            return ValidationOutcome::Invalid;
        };
        if d[0] != 9 {
            return ValidationOutcome::Invalid;
        }
        let middle = d[3] * 10 + d[4];
        let valid = (70..=88).contains(&middle) || (90..=92).contains(&middle) || (94..=99).contains(&middle);
        ValidationOutcome::from_bool(valid)
    }
}
