//! US DEA Certificate Number checksum (Luhn-variant over 7 trailing digits).
//!
//! The DEA number is `<letter><letter|9><7 digits>`. The last digit is a
//! check digit derived from the first six per Drug Enforcement Administration
//! spec: `(2·(d1 + d3 + d5) + (d0 + d2 + d4)) mod 10 == check`. Letters and
//! the optional middle `9` are ignored by the math.

use crate::ValidationOutcome;

pub(super) fn validate(candidate: &str) -> ValidationOutcome {
    // Pattern guarantees 9 ASCII chars after the regex match: 2-char prefix
    // (letters, or letter + literal `9`) then exactly 7 digits.
    let bytes = candidate.as_bytes();
    if bytes.len() < 3 {
        return ValidationOutcome::Invalid;
    }
    let mut digits = [0u32; 7];
    let mut i = 0usize;
    for &b in &bytes[2..] {
        if !b.is_ascii_digit() {
            continue;
        }
        if i == digits.len() {
            return ValidationOutcome::Invalid;
        }
        digits[i] = u32::from(b - b'0');
        i += 1;
    }
    if i != digits.len() {
        return ValidationOutcome::Invalid;
    }
    let check = digits[6];
    let sum_odd_positions = digits[1] + digits[3] + digits[5];
    let sum_even_positions = digits[0] + digits[2] + digits[4];
    let computed = (2 * sum_odd_positions + sum_even_positions) % 10;
    ValidationOutcome::from_bool(computed == check % 10)
}

#[cfg(test)]
mod tests {
    use super::validate;
    use crate::ValidationOutcome;

    #[test]
    fn valid_dea_passes() {
        // AB1234563 — body 123456 → 2*(2+4+6) + (1+3+5) = 24+9 = 33 → 33%10 = 3 ✓
        assert_eq!(validate("AB1234563"), ValidationOutcome::Valid);
    }

    #[test]
    fn valid_dea_nine_prefix() {
        // A91234563 — same checksum math applies (the `9` is part of the prefix).
        assert_eq!(validate("A91234563"), ValidationOutcome::Valid);
    }

    #[test]
    fn bad_checksum_rejected() {
        assert_eq!(validate("AB1234560"), ValidationOutcome::Invalid);
    }

    #[test]
    fn too_short_rejected() {
        assert_eq!(validate("AB"), ValidationOutcome::Invalid);
        assert_eq!(validate("AB12345"), ValidationOutcome::Invalid);
    }
}
