//! Operators that rewrite a single matched span and the operator-config map.

use std::borrow::Cow;

use crate::error::OperatorError;
use crate::recognizer::EntityType;

mod hash;
mod mask;

/// Hash algorithm for [`Operator::Hash`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HashAlgorithm {
    /// SHA-256, 256-bit digest.
    Sha256,
    /// SHA-512, 512-bit digest.
    Sha512,
}

/// Mask coverage parameter for [`Operator::Mask`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChunkCount {
    /// Mask the entire span, length-preserving.
    All,
    /// Mask exactly `n` UTF-8 code points.
    N(usize),
}

/// Algorithm used to rewrite a single PII span.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Operator {
    /// Replace the span with a fixed literal.
    Replace {
        /// Literal text written into the span.
        new_value: Cow<'static, str>,
    },
    /// Mask code points with `masking_char`.
    Mask {
        /// Character emitted in place of each masked code point.
        masking_char: char,
        /// How many code points to mask.
        chars_to_mask: ChunkCount,
        /// `true` keeps the span's prefix unmasked, `false` keeps the suffix.
        from_end: bool,
    },
    /// Replace the span with the empty string.
    Redact,
    /// Replace the span with a hex digest (bare or HMAC-keyed).
    Hash {
        /// Hash algorithm to use.
        algorithm: HashAlgorithm,
        /// `Some(key)` switches to HMAC-keyed hashing; `None` is bare digest.
        ///
        /// Constructing the variant directly with `Some(empty)` violates the
        /// invariant enforced by [`Operator::hash`]; prefer the constructor.
        hash_key: Option<Vec<u8>>,
    },
}

/// Tag-only kind enum for audit-trail use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OperatorKind {
    /// [`Operator::Replace`].
    Replace,
    /// [`Operator::Mask`].
    Mask,
    /// [`Operator::Redact`].
    Redact,
    /// [`Operator::Hash`].
    Hash,
}

impl Operator {
    /// Default placeholder operator: `Replace { new_value: "<{entity_type}>" }`.
    #[must_use]
    pub fn default_for(entity_type: &EntityType) -> Self {
        Self::Replace {
            new_value: Cow::Owned(format!("<{}>", entity_type.as_str())),
        }
    }

    /// Default `Mask` per spec clarification: `'*'`, full span, length-preserving.
    #[must_use]
    pub fn default_mask() -> Self {
        Self::Mask {
            masking_char: '*',
            chars_to_mask: ChunkCount::All,
            from_end: true,
        }
    }

    /// Construct a hash operator; rejects an empty `hash_key`.
    ///
    /// # Errors
    ///
    /// Returns [`OperatorError::EmptyHashKey`] when `hash_key` is `Some(empty)`.
    pub fn hash(algorithm: HashAlgorithm, hash_key: Option<Vec<u8>>) -> Result<Self, OperatorError> {
        if matches!(hash_key.as_deref(), Some(k) if k.is_empty()) {
            return Err(OperatorError::EmptyHashKey);
        }
        Ok(Self::Hash { algorithm, hash_key })
    }

    /// Tag describing this operator's variant.
    #[must_use]
    pub const fn kind(&self) -> OperatorKind {
        match self {
            Self::Replace { .. } => OperatorKind::Replace,
            Self::Mask { .. } => OperatorKind::Mask,
            Self::Redact => OperatorKind::Redact,
            Self::Hash { .. } => OperatorKind::Hash,
        }
    }

    /// Apply the operator to one matched span.
    pub(crate) fn apply(&self, candidate: &str) -> String {
        match self {
            Self::Replace { new_value } => new_value.as_ref().to_owned(),
            Self::Mask {
                masking_char,
                chars_to_mask,
                from_end,
            } => mask::apply(candidate, *masking_char, *chars_to_mask, *from_end),
            Self::Redact => String::new(),
            Self::Hash { algorithm, hash_key } => hash::apply(candidate, *algorithm, hash_key.as_deref()),
        }
    }
}
