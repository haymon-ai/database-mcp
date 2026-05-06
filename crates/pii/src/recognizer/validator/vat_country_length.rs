//! EU / UK VAT-number country-length validator.

use crate::recognizer::{ValidationOutcome, Validator};

/// EU / UK VAT-number country-length validator.
///
/// Format `<ISO2><alphanumeric>`. Checks the alphanumeric body length against
/// a per-country window. Unknown prefix → [`ValidationOutcome::Unknown`] so
/// niche/new countries are not over-rejected.
#[derive(Debug, Default, Clone, Copy)]
pub struct VatCountryLengthValidator;

const VAT_COUNTRY_LENGTHS: &[(&str, u32, u32)] = &[
    ("AT", 9, 9),   // U + 8 digits
    ("BE", 10, 10), // 10 digits
    ("BG", 9, 10),
    ("CY", 9, 9),
    ("CZ", 8, 10),
    ("DE", 9, 9),
    ("DK", 8, 8),
    ("EE", 9, 9),
    ("EL", 9, 9), // Greece (alt code)
    ("GR", 9, 9),
    ("ES", 9, 9),
    ("FI", 8, 8),
    ("FR", 11, 11),
    ("GB", 9, 12), // 9 short, 12 long
    ("HR", 11, 11),
    ("HU", 8, 8),
    ("IE", 8, 9),
    ("IT", 11, 11),
    ("LT", 9, 12),
    ("LU", 8, 8),
    ("LV", 11, 11),
    ("MT", 8, 8),
    ("NL", 12, 12),
    ("PL", 10, 10),
    ("PT", 9, 9),
    ("RO", 2, 10),
    ("SE", 12, 12),
    ("SI", 8, 8),
    ("SK", 10, 10),
    ("XI", 9, 12), // Northern Ireland post-Brexit
];

impl Validator for VatCountryLengthValidator {
    fn validate(&self, candidate: &str) -> ValidationOutcome {
        // ISO2 prefix + up to 12-char body fits in 14 bytes.
        let mut buf = [0u8; 14];
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
        if len < 3 {
            return ValidationOutcome::Invalid;
        }
        let prefix = [buf[0], buf[1]];
        let body_len = u32::try_from(len - 2).unwrap_or(u32::MAX);
        for &(code, lo, hi) in VAT_COUNTRY_LENGTHS {
            if code.as_bytes() == prefix {
                return ValidationOutcome::from_bool((lo..=hi).contains(&body_len));
            }
        }
        ValidationOutcome::Unknown
    }
}
