//! Shared `BigDecimal` → JSON conversion for the per-backend row decoders.
//!
//! `bigdecimal_to_json` enforces the value-driven JSON shape rule for
//! fixed-point and arbitrary-precision numerics: integers fitting in `i64`
//! emit as integer JSON numbers, values whose canonical decimal form has
//! ≤15 digits emit as JSON numbers via `Number::from_f64`, and everything
//! else emits as the decimal string. The same database value always
//! produces the same JSON shape regardless of column or backend.

use bigdecimal::{BigDecimal, ToPrimitive};
use serde_json::Value;

/// Maximum significant decimal digits that round-trip through `f64`.
///
/// IEEE 754 binary64 holds 15–17 decimal digits depending on value; 15 is
/// the conservative bound that guarantees a clean round-trip for every
/// value. Beyond this, the canonical decimal string is the only lossless
/// JSON shape.
const F64_SAFE_DIGITS: u64 = 15;

/// Converts a `BigDecimal` to the canonical JSON shape for numeric values.
///
/// Integer-valued decimals that fit in `i64` emit as integer JSON numbers.
/// Other values emit as `Value::Number` when their canonical decimal form
/// has at most `F64_SAFE_DIGITS` digits, else as `Value::String` carrying
/// the decimal text.
pub(crate) fn bigdecimal_to_json(value: &BigDecimal) -> Value {
    let normalized = value.normalized();

    if normalized.is_integer()
        && let Some(as_i64) = normalized.to_i64()
    {
        return Value::from(as_i64);
    }

    // `digits()` counts mantissa only — for negative scale (e.g. `1e30` =
    // mantissa 1, scale -29) add the trailing zeros so huge values don't
    // slip past the gate and emit as lossy JSON numbers.
    let scale = normalized.fractional_digit_count();
    let canonical_digits = normalized.digits() + scale.min(0).unsigned_abs();
    if canonical_digits <= F64_SAFE_DIGITS
        && let Some(as_f64) = normalized.to_f64()
        && let Some(num) = serde_json::Number::from_f64(as_f64)
    {
        return Value::from(num);
    }

    // `Display` (uses scientific form past internal thresholds) keeps the
    // output bounded for pathological NUMERICs; `to_plain_string` would
    // expand 1e131072 to a 131-KB string per row.
    Value::String(normalized.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(s: &str) -> BigDecimal {
        BigDecimal::from_str(s).expect("valid decimal literal")
    }

    #[test]
    fn small_fixed_point_is_number() {
        assert_eq!(bigdecimal_to_json(&dec("0.5")), Value::from(0.5));
        assert_eq!(bigdecimal_to_json(&dec("42")), Value::from(42));
        assert_eq!(bigdecimal_to_json(&dec("-1.25")), Value::from(-1.25));
    }

    #[test]
    fn trailing_zeros_normalize_consistently() {
        let a = bigdecimal_to_json(&dec("1.20"));
        let b = bigdecimal_to_json(&dec("1.2"));
        assert_eq!(a, b);
    }

    #[test]
    fn value_beyond_f64_precision_is_string() {
        let v = bigdecimal_to_json(&dec("12345678901234567890.1234567890"));
        assert_eq!(v, Value::String("12345678901234567890.123456789".to_string()));
    }

    #[test]
    fn small_decimal_with_many_significant_digits_is_string() {
        let v = bigdecimal_to_json(&dec("0.123456789012345678901234567890"));
        let Value::String(s) = v else {
            panic!("expected string for high-precision small decimal");
        };
        assert!(s.starts_with("0.12345678901234567890"));
    }

    #[test]
    fn shape_is_deterministic_per_value() {
        let v1 = bigdecimal_to_json(&dec("99999999999999999999.99"));
        let v2 = bigdecimal_to_json(&dec("99999999999999999999.99"));
        assert_eq!(v1, v2);
        assert!(matches!(v1, Value::String(_)));
    }

    #[test]
    fn integer_value_uses_integer_branch_regardless_of_digit_count() {
        // i64::MAX has 19 digits but is still an integer that fits, so the
        // integer fast-path wins over the digit gate.
        let v = bigdecimal_to_json(&dec(&i64::MAX.to_string()));
        assert_eq!(v, Value::Number(i64::MAX.into()));
    }

    #[test]
    fn boundary_15_digit_fraction_is_number() {
        let v = bigdecimal_to_json(&dec("12345678901234.5"));
        assert!(matches!(v, Value::Number(_)));
    }

    #[test]
    fn boundary_16_digit_fraction_is_string() {
        let v = bigdecimal_to_json(&dec("12345678901234.56"));
        assert!(matches!(v, Value::String(_)));
    }

    #[test]
    fn small_fraction_with_few_digits_is_number() {
        // 0.1 has 1 significant digit; safe in f64 even though the binary
        // representation is approximate. JSON serialization is shortest
        // round-trip via ryu, so the wire form matches the database value.
        assert_eq!(bigdecimal_to_json(&dec("0.1")), Value::from(0.1));
        assert_eq!(bigdecimal_to_json(&dec("0.10")), Value::from(0.1));
        assert_eq!(bigdecimal_to_json(&dec("0.30")), Value::from(0.3));
    }

    #[test]
    fn high_precision_fraction_is_string() {
        let v = bigdecimal_to_json(&dec("0.123456789012345678"));
        assert!(matches!(v, Value::String(_)));
    }

    #[test]
    fn huge_magnitude_with_few_significant_digits_is_string() {
        // 1 followed by 30 zeros (DECIMAL(38,0)). Mantissa has 1 digit but
        // magnitude exceeds 2^53, so canonical form must emit as string —
        // `Display` writes scientific form past its zero-threshold.
        let v = bigdecimal_to_json(&dec("1000000000000000000000000000000"));
        assert!(matches!(v, Value::String(_)));
    }
}
