//! Sweep-line overlap resolution for analyzer and anonymizer results.
//!
//! Algorithm per FR-011 / research §R6:
//!
//! 1. Sort by `(start, -score, registration_order)`.
//! 2. Walk left to right; for each new span, compare against the current dominant
//!    span. Drop strictly-contained duplicates; on cross-type overlap, higher
//!    score wins; ties broken by longer span, then by recognizer-registration
//!    order (the position the result already has in the input vector).
//! 3. Emit survivors in original-position order.

use crate::result::RecognizerResult;

/// Resolve overlaps in `results`, consuming and returning the surviving spans.
#[must_use]
pub fn resolve(mut results: Vec<RecognizerResult>) -> Vec<RecognizerResult> {
    if results.len() <= 1 {
        return results;
    }
    // Pair each result with its original index so we can break ties by registration order.
    let mut indexed: Vec<(usize, RecognizerResult)> = results.drain(..).enumerate().collect();

    indexed.sort_by(|a, b| {
        a.1.start
            .cmp(&b.1.start)
            .then_with(|| {
                b.1.score
                    .as_f32()
                    .partial_cmp(&a.1.score.as_f32())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.0.cmp(&b.0))
    });

    let mut survivors: Vec<(usize, RecognizerResult)> = Vec::with_capacity(indexed.len());

    for (idx, candidate) in indexed {
        let mut keep = true;
        survivors.retain(|(_, existing)| {
            if !overlaps(existing, &candidate) {
                return true;
            }
            match dominate(existing, &candidate) {
                Dominance::Existing | Dominance::Equal => {
                    keep = false;
                    true
                }
                Dominance::Candidate => false,
            }
        });
        if keep {
            survivors.push((idx, candidate));
        }
    }

    survivors.sort_by_key(|(idx, _)| *idx);
    survivors.into_iter().map(|(_, r)| r).collect()
}

fn overlaps(a: &RecognizerResult, b: &RecognizerResult) -> bool {
    a.start < b.end && b.start < a.end
}

enum Dominance {
    Existing,
    Candidate,
    Equal,
}

fn dominate(existing: &RecognizerResult, candidate: &RecognizerResult) -> Dominance {
    // Same entity type and one strictly contains the other → drop contained.
    if existing.entity_type == candidate.entity_type {
        if existing.start <= candidate.start && existing.end >= candidate.end {
            return Dominance::Existing;
        }
        if candidate.start <= existing.start && candidate.end >= existing.end {
            return Dominance::Candidate;
        }
    }
    // Cross-type or partial-overlap same-type: higher score wins.
    let by_score = existing
        .score
        .as_f32()
        .partial_cmp(&candidate.score.as_f32())
        .unwrap_or(std::cmp::Ordering::Equal);
    match by_score {
        std::cmp::Ordering::Greater => Dominance::Existing,
        std::cmp::Ordering::Less => Dominance::Candidate,
        std::cmp::Ordering::Equal => {
            let existing_len = existing.end - existing.start;
            let candidate_len = candidate.end - candidate.start;
            match existing_len.cmp(&candidate_len) {
                std::cmp::Ordering::Greater => Dominance::Existing,
                std::cmp::Ordering::Less => Dominance::Candidate,
                // Tie on score and length: registration order already enforced by sort,
                // so the existing survivor wins.
                std::cmp::Ordering::Equal => Dominance::Equal,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::resolve;
    use crate::recognizer::{EntityType, ValidationOutcome};
    use crate::result::{AnalysisExplanation, RecognizerResult};
    use crate::score::Score;

    fn rr(et: &str, start: usize, end: usize, score: f32) -> RecognizerResult {
        let s = Score::new(score).unwrap();
        RecognizerResult {
            entity_type: EntityType::new(et.to_owned()),
            start,
            end,
            score: s,
            explanation: AnalysisExplanation {
                recognizer_name: Cow::Owned(et.to_owned()),
                pattern_name: None,
                original_score: s,
                validation: ValidationOutcome::Unknown,
                final_score: s,
            },
        }
    }

    #[test]
    fn drops_strictly_contained_same_type() {
        let r = resolve(vec![rr("E", 0, 10, 0.5), rr("E", 2, 5, 0.5)]);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].start, 0);
    }

    #[test]
    fn higher_score_wins_cross_type() {
        let r = resolve(vec![rr("A", 0, 5, 0.3), rr("B", 0, 5, 0.9)]);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].entity_type.as_str(), "B");
    }

    #[test]
    fn longer_span_wins_on_score_tie() {
        let r = resolve(vec![rr("A", 0, 8, 0.5), rr("B", 0, 5, 0.5)]);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].entity_type.as_str(), "A");
    }

    #[test]
    fn registration_order_breaks_full_tie() {
        // Same start, same end, same score → first registered wins.
        let r = resolve(vec![rr("A", 0, 5, 0.5), rr("B", 0, 5, 0.5)]);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].entity_type.as_str(), "A");
    }

    #[test]
    fn non_overlapping_kept() {
        let r = resolve(vec![rr("A", 0, 5, 0.5), rr("B", 10, 15, 0.5)]);
        assert_eq!(r.len(), 2);
    }
}
