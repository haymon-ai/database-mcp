//! Deny-list helper that compiles literal terms into a single recognizer.

use crate::error::RecognizerError;
use crate::pattern::Pattern;
use crate::score::Score;

use super::{EntityType, PatternRecognizer};

/// Build a recognizer that matches whole-word occurrences of the supplied `terms`.
///
/// Compiles to a single `regex` pattern of the form `\b(?:term1|term2)\b`. Terms are
/// regex-escaped so metacharacters are matched literally. The `\b` word boundary
/// guarantees substrings of larger words do not match (e.g. `OBSIDIAN` does not match
/// inside `OBSIDIANITE`).
///
/// # Errors
///
/// * [`RecognizerError::EmptyPatternList`] when `terms` is empty or when the produced
///   regex fails to compile (an internal-bug case).
pub fn deny_list_recognizer<S: AsRef<str>>(
    entity_type: EntityType,
    terms: &[S],
    score: Score,
) -> Result<PatternRecognizer, RecognizerError> {
    if terms.is_empty() {
        return Err(RecognizerError::EmptyPatternList);
    }
    let alternation = terms
        .iter()
        .map(|t| regex::escape(t.as_ref()))
        .collect::<Vec<_>>()
        .join("|");
    let regex_src = format!(r"\b(?:{alternation})\b");
    let pattern = Pattern::new("deny_list", regex_src, score).map_err(|_| RecognizerError::EmptyPatternList)?;
    PatternRecognizer::new(entity_type, vec![pattern])
}
