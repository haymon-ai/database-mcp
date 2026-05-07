//! Phone-number national-format grammar validator.
//!
//! Strips separators, then accepts the cleaned digit form against
//! per-region grammars (`E.164`, `US`/NANP, `UK`, `DE`). Rejects any
//! `0`-prefixed digit run shorter than 11 cleaned digits — the
//! leading-zero false-positive class fixed by issue #147.

use crate::recognizer::ValidationOutcome;

/// Phone-number national-format grammar validator.
///
/// Accepts E.164 with `+` prefix, NANP 10/11-digit shapes, UK national
/// 11-digit `0[1-9]…`, DE national 11–13-digit `0[1-9]…`, and the
/// matching `44` / `49` international forms without `+`.
pub(super) fn validate(candidate: &str) -> ValidationOutcome {
    let mut digits = [0u8; 16];
    let mut len = 0usize;
    let mut had_plus = false;
    let mut seen_first = false;
    for &b in candidate.as_bytes() {
        if !seen_first {
            seen_first = true;
            if b == b'+' {
                had_plus = true;
                continue;
            }
        }
        if b.is_ascii_digit() {
            if len >= digits.len() {
                return ValidationOutcome::Invalid;
            }
            digits[len] = b - b'0';
            len += 1;
        }
    }
    let d = &digits[..len];

    if had_plus {
        return if (8..=15).contains(&len) {
            ValidationOutcome::Unknown
        } else {
            ValidationOutcome::Invalid
        };
    }

    // US/NANP: 10 digits, area-code first digit in [2-9].
    if len == 10 && d[0] >= 2 {
        return ValidationOutcome::Unknown;
    }
    // US/NANP with country code: 11 digits, leading `1`, area-code first in [2-9].
    if len == 11 && d[0] == 1 && d[1] >= 2 {
        return ValidationOutcome::Unknown;
    }
    // UK / DE national form: 11–13 digits starting `0[1-9]`.
    if (11..=13).contains(&len) && d[0] == 0 && (1..=9).contains(&d[1]) {
        return ValidationOutcome::Unknown;
    }
    // UK international form (no `+`): 12–13 digits starting `44` then `0?[1-9]…`.
    if (12..=13).contains(&len) && d[0] == 4 && d[1] == 4 {
        let rest = &d[2..];
        let start = usize::from(!rest.is_empty() && rest[0] == 0);
        if rest.len() > start && (1..=9).contains(&rest[start]) {
            return ValidationOutcome::Unknown;
        }
    }
    // DE international form (no `+`): 9–14 digits starting `49`.
    if (9..=14).contains(&len) && d[0] == 4 && d[1] == 9 {
        return ValidationOutcome::Unknown;
    }

    ValidationOutcome::Invalid
}

#[cfg(test)]
mod tests {
    use super::validate;
    use crate::recognizer::ValidationOutcome;

    fn is_valid(s: &str) -> bool {
        // Phone validator filters: returns `Unknown` for accept (leaves score at 0.4)
        // and `Invalid` for reject. `Valid` is intentionally never returned to avoid
        // promoting phone hits over higher-scored entities (e.g. NHS_NUMBER).
        validate(s) != ValidationOutcome::Invalid
    }

    #[test]
    fn issue_147_negatives_rejected() {
        // The four reproduction strings from issue #147.
        assert!(!is_valid("000-12-3456"));
        assert!(!is_valid("07-1234567"));
        assert!(!is_valid("046 454 287"));
        assert!(!is_valid("01234567"));
    }

    #[test]
    fn issue_147_extra_negatives_rejected() {
        // 10-digit leading-zero shape suggested as a corpus negative.
        assert!(!is_valid("0461234567"));
    }

    #[test]
    fn e164_accepted() {
        assert!(is_valid("+14155552671"));
        assert!(is_valid("+44 20 7946 0958"));
        assert!(is_valid("+49 30 12345678"));
    }

    #[test]
    fn nanp_local_form_accepted() {
        assert!(is_valid("(415) 555-2671"));
        assert!(is_valid("4155552671"));
        assert!(is_valid("1-415-555-2671"));
    }

    #[test]
    fn uk_national_form_accepted() {
        assert!(is_valid("02012345678"));
        assert!(is_valid("020 7946 0958"));
    }

    #[test]
    fn de_national_form_accepted() {
        assert!(is_valid("030 12345678"));
        assert!(is_valid("0151 12345678"));
    }

    #[test]
    fn too_short_rejected() {
        assert!(!is_valid("12"));
        assert!(!is_valid("+12"));
        assert!(!is_valid("1234567"));
    }

    #[test]
    fn too_long_rejected() {
        // Beyond E.164's 15-digit ceiling.
        assert!(!is_valid("+1234567890123456"));
    }

    #[test]
    fn nanp_invalid_area_code_rejected() {
        // Area code starts with 0 — invalid in NANP.
        assert!(!is_valid("0461234567"));
        assert!(!is_valid("(046) 123-4567"));
    }
}
