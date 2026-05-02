//! Built-in validators: `Luhn`, `IBAN` mod-97, and IP-address parse-validation.

use std::net::IpAddr;
use std::str::FromStr;

use super::{ValidationOutcome, Validator};

/// Default validator that abstains on every input.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopValidator;

impl Validator for NoopValidator {
    fn validate(&self, _candidate: &str) -> ValidationOutcome {
        ValidationOutcome::Unknown
    }
}

/// Luhn checksum validator for credit-card numbers.
///
/// Strips spaces and dashes before checking, matching Presidio's
/// `replacement_pairs = [("-", ""), (" ", "")]`.
#[derive(Debug, Default, Clone, Copy)]
pub struct LuhnValidator;

impl LuhnValidator {
    fn luhn_ok(digits: &[u8]) -> bool {
        let mut sum: u32 = 0;
        let mut alt = false;
        for &d in digits.iter().rev() {
            let mut n = u32::from(d);
            if alt {
                n *= 2;
                if n > 9 {
                    n -= 9;
                }
            }
            sum += n;
            alt = !alt;
        }
        sum.is_multiple_of(10)
    }
}

impl Validator for LuhnValidator {
    fn validate(&self, candidate: &str) -> ValidationOutcome {
        let digits: Vec<u8> = candidate
            .chars()
            .filter(|c| !matches!(*c, '-' | ' '))
            .filter_map(|c| c.to_digit(10))
            .map(|d| u8::try_from(d).expect("base-10 digit fits in u8"))
            .collect();
        if !(12..=19).contains(&digits.len()) {
            return ValidationOutcome::Invalid;
        }
        if Self::luhn_ok(&digits) {
            ValidationOutcome::Valid
        } else {
            ValidationOutcome::Invalid
        }
    }
}

/// IBAN mod-97 validator. Accepts upper-case input; whitespace stripped before checking.
#[derive(Debug, Default, Clone, Copy)]
pub struct IbanValidator;

impl IbanValidator {
    /// Streaming mod-97: walk the rearranged IBAN character-by-character, folding into
    /// `remainder` directly. No intermediate string, no chunked parse.
    fn mod97_stream<I: Iterator<Item = char>>(chars: I) -> Option<u32> {
        let mut remainder: u32 = 0;
        for c in chars {
            if let Some(d) = c.to_digit(10) {
                remainder = (remainder * 10 + d) % 97;
            } else if c.is_ascii_uppercase() {
                let v = u32::from(c as u8 - b'A' + 10);
                remainder = (remainder * 100 + v) % 97;
            } else {
                return None;
            }
        }
        Some(remainder)
    }
}

impl Validator for IbanValidator {
    fn validate(&self, candidate: &str) -> ValidationOutcome {
        let cleaned: Vec<char> = candidate
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| c.to_ascii_uppercase())
            .collect();
        if !(15..=34).contains(&cleaned.len()) {
            return ValidationOutcome::Invalid;
        }
        // Rearranged = tail (chars 4..) followed by head (chars 0..4).
        let rearranged = cleaned[4..].iter().chain(cleaned[..4].iter()).copied();
        match Self::mod97_stream(rearranged) {
            Some(1) => ValidationOutcome::Valid,
            _ => ValidationOutcome::Invalid,
        }
    }
}

/// US Social Security Number validator. Rejects reserved area / group / serial values
/// — replaces the negative-lookahead constructs Presidio's regex used.
#[derive(Debug, Default, Clone, Copy)]
pub struct UsSsnValidator;

impl Validator for UsSsnValidator {
    fn validate(&self, candidate: &str) -> ValidationOutcome {
        let digits: Vec<u8> = candidate
            .chars()
            .filter_map(|c| c.to_digit(10))
            .map(|d| u8::try_from(d).expect("base-10 digit fits in u8"))
            .collect();
        if digits.len() != 9 {
            return ValidationOutcome::Invalid;
        }
        let area = u32::from(digits[0]) * 100 + u32::from(digits[1]) * 10 + u32::from(digits[2]);
        let group = u32::from(digits[3]) * 10 + u32::from(digits[4]);
        let serial =
            u32::from(digits[5]) * 1000 + u32::from(digits[6]) * 100 + u32::from(digits[7]) * 10 + u32::from(digits[8]);
        if area == 0 || area == 666 || area >= 900 || group == 0 || serial == 0 {
            return ValidationOutcome::Invalid;
        }
        ValidationOutcome::Valid
    }
}

/// IP-address validator that delegates to [`std::net::IpAddr::from_str`].
///
/// CIDR-like suffixes (`/24`, `/64`) are stripped before parsing; only the
/// address portion is parse-validated. A bare IPv6 zone identifier (`%eth0`)
/// is also stripped because `from_str` rejects it on stable today.
#[derive(Debug, Default, Clone, Copy)]
pub struct IpAddressValidator;

impl Validator for IpAddressValidator {
    fn validate(&self, candidate: &str) -> ValidationOutcome {
        // Strip CIDR suffix `/N` and IPv6 zone identifier `%zone` in one split.
        let trimmed = candidate.split(['/', '%']).next().unwrap_or("");
        if IpAddr::from_str(trimmed).is_ok() {
            ValidationOutcome::Valid
        } else {
            ValidationOutcome::Invalid
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{IbanValidator, IpAddressValidator, LuhnValidator, ValidationOutcome, Validator};

    #[test]
    fn luhn_valid_visa() {
        assert_eq!(LuhnValidator.validate("4111-1111-1111-1111"), ValidationOutcome::Valid);
    }

    #[test]
    fn luhn_invalid_visa() {
        assert_eq!(
            LuhnValidator.validate("4111-1111-1111-1112"),
            ValidationOutcome::Invalid
        );
    }

    #[test]
    fn luhn_rejects_short() {
        assert_eq!(LuhnValidator.validate("4111111"), ValidationOutcome::Invalid);
    }

    #[test]
    fn iban_valid_de() {
        // Wikipedia example
        assert_eq!(
            IbanValidator.validate("DE89 3704 0044 0532 0130 00"),
            ValidationOutcome::Valid
        );
    }

    #[test]
    fn iban_invalid_check_digits() {
        assert_eq!(
            IbanValidator.validate("DE00 3704 0044 0532 0130 00"),
            ValidationOutcome::Invalid
        );
    }

    #[test]
    fn ip_valid_v4() {
        assert_eq!(IpAddressValidator.validate("192.168.1.1"), ValidationOutcome::Valid);
    }

    #[test]
    fn ip_invalid_v4() {
        assert_eq!(IpAddressValidator.validate("192.168.1.999"), ValidationOutcome::Invalid);
    }

    #[test]
    fn ip_valid_v6() {
        assert_eq!(IpAddressValidator.validate("::1"), ValidationOutcome::Valid);
    }

    #[test]
    fn ip_with_cidr_suffix() {
        assert_eq!(IpAddressValidator.validate("10.0.0.0/24"), ValidationOutcome::Valid);
    }
}
