//! Error types for the `dbmcp-pii` crate.

use thiserror::Error;

/// Errors that surface when constructing a [`crate::Pattern`] or [`crate::Score`].
#[derive(Debug, Error)]
pub enum PatternError {
    /// `regex`-engine compile error.
    #[error("invalid regex: {0}")]
    InvalidRegex(Box<regex::Error>),
    /// Score was non-finite or outside `[0.0, 1.0]`.
    #[error("invalid score: {value} (must be a finite value in [0.0, 1.0])")]
    InvalidScore {
        /// Offending input value.
        value: f32,
    },
}

impl PatternError {
    pub(crate) fn from_regex(e: regex::Error) -> Self {
        Self::InvalidRegex(Box::new(e))
    }
}

/// Errors that surface when constructing a [`crate::PatternRecognizer`] or a deny-list helper.
#[derive(Debug, Error)]
pub enum RecognizerError {
    /// Caller supplied no patterns and no deny-list terms.
    #[error("recognizer requires at least one pattern or deny-list term")]
    EmptyPatternList,
}

/// Reserved namespace for analyzer-level errors.
///
/// Empty in v1; marked `#[non_exhaustive]` so future variants do not break callers.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AnalyzerError {}

/// Errors that surface when constructing or applying an [`crate::Operator`].
#[derive(Debug, Error)]
pub enum OperatorError {
    /// `hash_key` was provided but empty; reject up front rather than silently downgrade to bare digest.
    #[error("hash_key must be non-empty when provided")]
    EmptyHashKey,
}
