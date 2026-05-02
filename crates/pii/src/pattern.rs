//! Named regex with confidence score; eagerly compiled at construction.

use std::fmt;

use crate::error::PatternError;
use crate::score::Score;

/// Named regex pattern with a base confidence score.
///
/// Backed by the linear-time `regex` crate (RE2 semantics). Compiled eagerly
/// so a bad pattern is rejected at construction, not at match time.
#[derive(Clone)]
pub struct Pattern {
    name: String,
    regex: String,
    score: Score,
    pub(crate) compiled: regex::Regex,
}

impl fmt::Debug for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pattern")
            .field("name", &self.name)
            .field("regex", &self.regex)
            .field("score", &self.score)
            .finish_non_exhaustive()
    }
}

impl Pattern {
    /// Build a pattern.
    ///
    /// # Errors
    ///
    /// Returns [`PatternError::InvalidRegex`] when the source fails to compile.
    pub fn new(name: impl Into<String>, regex_src: impl Into<String>, score: Score) -> Result<Self, PatternError> {
        let regex_src = regex_src.into();
        let compiled = regex::Regex::new(&regex_src).map_err(PatternError::from_regex)?;
        Ok(Self {
            name: name.into(),
            regex: regex_src,
            score,
            compiled,
        })
    }

    /// Pattern's human-readable name; surfaced in [`crate::AnalysisExplanation`].
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Regex source (the string the pattern was constructed with).
    #[must_use]
    pub fn regex(&self) -> &str {
        &self.regex
    }

    /// Base confidence score, before any validator promotion.
    #[must_use]
    pub fn score(&self) -> Score {
        self.score
    }
}

mod serde_impl {
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

    use super::Pattern;
    use crate::score::Score;

    #[derive(Serialize, Deserialize)]
    struct Wire {
        name: String,
        regex: String,
        score: Score,
    }

    impl Serialize for Pattern {
        fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
            Wire {
                name: self.name.clone(),
                regex: self.regex.clone(),
                score: self.score,
            }
            .serialize(ser)
        }
    }

    impl<'de> Deserialize<'de> for Pattern {
        fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
            let w = Wire::deserialize(de)?;
            Pattern::new(w.name, w.regex, w.score).map_err(D::Error::custom)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Pattern;
    use crate::error::PatternError;
    use crate::score::Score;

    fn s(v: f32) -> Score {
        Score::new(v).expect("valid score")
    }

    #[test]
    fn rejects_invalid_regex() {
        let err = Pattern::new("bad", "(unclosed", s(0.5)).unwrap_err();
        assert!(matches!(err, PatternError::InvalidRegex(_)));
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn accepts_valid_regex() {
        let p = Pattern::new("digits", r"\b\d+\b", s(0.5)).unwrap();
        assert_eq!(p.score().as_f32(), 0.5);
    }

    #[test]
    fn rejects_lookbehind() {
        // The `regex` crate does not support lookbehind; the pattern is rejected.
        let err = Pattern::new("bad_lb", r"(?<!a)b", s(0.5)).unwrap_err();
        assert!(matches!(err, PatternError::InvalidRegex(_)));
    }
}
