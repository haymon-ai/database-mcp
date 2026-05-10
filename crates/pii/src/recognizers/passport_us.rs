//! `PASSPORT_US` recognizer (keyword-context required).

use super::Recognizer;
use crate::pattern::Pattern;
use crate::score::Score;
use crate::validators::{KeywordValidator, Validator};
use crate::{Category, Entity};

const KEYWORDS: &[&str] = &["passport", "travel document"];

/// Build the `PASSPORT_US` recognizer.
///
/// # Panics
///
/// Panics only if the bundled regex source or score literal is rejected at construction.
#[must_use]
pub fn passport_us() -> Recognizer {
    let pattern = Pattern::new("US passport", r"(?i)\b[PE]\d{6,8}\b", Score::from_static(0.4))
        .expect("static US passport pattern compiles");
    Recognizer::new(Entity::PassportUs, vec![pattern])
        .expect("non-empty pattern list")
        .with_name("PassportUsRecognizer")
        .with_validator(Validator::Keyword(KeywordValidator::new(KEYWORDS)))
        .with_category(Category::Government)
}

#[cfg(test)]
mod tests {
    use super::passport_us;

    fn matches(text: &str) -> Vec<(usize, usize)> {
        passport_us()
            .analyze(text)
            .into_iter()
            .map(|r| (r.start, r.end))
            .collect()
    }

    #[test]
    fn recognizes_passport_us() {
        let cases: &[(&str, &[(usize, usize)])] = &[
            ("passport P01234567", &[(9, 18)]),
            ("travel document E1234567", &[(16, 24)]),
            ("Passport p1234567", &[(9, 17)]),
            ("ticket P01234567", &[]),
            ("passport Q01234567", &[]),
            ("passport P12", &[]),
            ("passport P123456789", &[]),
            ("", &[]),
        ];
        for (input, expected) in cases {
            assert_eq!(matches(input), expected.to_vec(), "input {input:?}: span mismatch");
        }
    }
}
