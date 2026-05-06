//! Analyzer engine: registry + entry point + analyze-time options.

use std::collections::HashSet;

use dbmcp_config::{PiiCategory, PiiConfig};

use crate::error::AnalyzerBuildError;
use crate::overlap;
use crate::recognizer::{Category, EntityType, Recognizer, Severity};
use crate::result::RecognizerResult;
use crate::score::Score;

/// Per-call overrides handed to [`Analyzer::analyze`].
///
/// `min_score` defaults to [`crate::MIN_SCORE`] (via [`Score::default`]); set higher to
/// drop low-confidence matches before overlap resolution.
#[derive(Debug, Clone, Default)]
pub struct AnalyzeOptions {
    /// Restrict the engine to recognizers whose `supported_entities` intersect this set.
    pub entity_allow_list: Option<HashSet<EntityType>>,
    /// Drop results whose score is below this floor before overlap resolution.
    pub min_score: Score,
}

/// Registry of recognizers and the public entry point for PII analysis.
#[derive(Default)]
pub struct Analyzer {
    recognizers: Vec<Box<dyn Recognizer>>,
}

impl std::fmt::Debug for Analyzer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Analyzer")
            .field(
                "recognizers",
                &self.recognizers.iter().map(|r| r.name()).collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl Analyzer {
    /// Build an analyzer with no recognizers; caller registers their own.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build an analyzer pre-loaded with the eight v1 default recognizers.
    #[must_use]
    pub fn with_defaults() -> Self {
        let recognizers = crate::recognizer::pattern::all()
            .into_iter()
            .map(|r| Box::new(r) as Box<dyn Recognizer>)
            .collect();
        Self { recognizers }
    }

    /// Register a recognizer at the end of the registry.
    pub fn register(&mut self, recognizer: Box<dyn Recognizer>) -> &mut Self {
        self.recognizers.push(recognizer);
        self
    }

    /// Analyze `text`, returning merged + overlap-resolved results.
    #[must_use]
    pub fn analyze(&self, text: &str, opts: &AnalyzeOptions) -> Vec<RecognizerResult> {
        let results = self
            .recognizers
            .iter()
            .filter(|r| match &opts.entity_allow_list {
                Some(allow) => r.supported_entities().iter().any(|e| allow.contains(e)),
                None => true,
            })
            .flat_map(|r| r.analyze(text, opts))
            .filter(|r| r.score >= opts.min_score)
            .collect();
        overlap::resolve(results)
    }

    /// Construct a fresh [`Builder`] for category / severity routing.
    #[must_use]
    pub fn builder() -> Builder {
        Builder::default()
    }

    /// Iterate the registry's recognizers in registration order.
    pub fn recognizers(&self) -> impl Iterator<Item = &dyn Recognizer> + '_ {
        self.recognizers.iter().map(std::convert::AsRef::as_ref)
    }

    /// Resolve a [`PiiConfig`] to an [`Analyzer`].
    ///
    /// - `categories` unset → [`Analyzer::with_defaults`] (the legacy 8 v1
    ///   recognizers; FR-302 backward-compat).
    /// - `categories` set → builder filters the registry by category set.
    /// - On builder error, falls back to `with_defaults` and logs at `warn`
    ///   level so a misconfiguration never disables redaction silently.
    #[must_use]
    pub fn from_pii_config(config: &PiiConfig) -> Self {
        let Some(cats) = config.categories.as_ref() else {
            return Self::with_defaults();
        };
        match Self::builder()
            .categories(cats.iter().copied().map(map_category))
            .build()
        {
            Ok(a) => a,
            Err(err) => {
                tracing::warn!(
                    target: "dbmcp::pii",
                    error = %err,
                    "PII analyzer build failed; falling back to with_defaults()"
                );
                Self::with_defaults()
            }
        }
    }
}

fn map_category(c: PiiCategory) -> Category {
    match c {
        PiiCategory::Personal => Category::Personal,
        PiiCategory::Financial => Category::Financial,
        PiiCategory::Government => Category::Government,
        PiiCategory::Contact => Category::Contact,
        PiiCategory::Network => Category::Network,
        PiiCategory::DigitalIdentity => Category::DigitalIdentity,
        PiiCategory::Crypto => Category::Crypto,
    }
}

/// Typed builder that filters the merged `all() ∪ all_extended()` registry by
/// category / severity floor.
///
/// See `specs/095-pii-recognizer-catalog/data-model.md` for the resolution
/// rules. `Analyzer::with_defaults()` stays frozen at the original 8
/// recognizers regardless of this builder.
#[derive(Default)]
pub struct Builder {
    categories: Option<Vec<Category>>,
    min_severity: Option<Severity>,
    custom: Vec<Box<dyn Recognizer>>,
    allow_empty_categories: bool,
}

impl std::fmt::Debug for Builder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Builder")
            .field("categories", &self.categories)
            .field("min_severity", &self.min_severity)
            .field("custom_count", &self.custom.len())
            .field("allow_empty_categories", &self.allow_empty_categories)
            .finish()
    }
}

impl Builder {
    /// Set the effective category set the analyzer filters by.
    #[must_use]
    pub fn categories(mut self, cats: impl IntoIterator<Item = Category>) -> Self {
        let mut out: Vec<Category> = Vec::new();
        for c in cats {
            if !out.contains(&c) {
                out.push(c);
            }
        }
        self.categories = Some(out);
        self
    }

    /// Set the severity floor for the analyzer.
    #[must_use]
    pub fn min_severity(mut self, severity: Severity) -> Self {
        self.min_severity = Some(severity);
        self
    }

    /// Append a user-supplied recognizer; participates in the same filter as built-ins.
    #[must_use]
    pub fn custom_recognizer(mut self, recognizer: impl Recognizer + 'static) -> Self {
        self.custom.push(Box::new(recognizer));
        self
    }

    /// When `true`, [`Builder::build`] does not error if a requested category
    /// resolves to zero recognizers in the current registry.
    #[must_use]
    pub fn allow_empty_categories(mut self, allow: bool) -> Self {
        self.allow_empty_categories = allow;
        self
    }

    /// Build the [`Analyzer`] applying the resolved filters.
    ///
    /// # Errors
    ///
    /// Returns [`AnalyzerBuildError::EmptyCategory`] if a requested category
    /// has zero recognizers tagging it (and `allow_empty_categories(true)` was
    /// not set).
    pub fn build(self) -> Result<Analyzer, AnalyzerBuildError> {
        let effective_cats = self.categories;
        let floor = self.min_severity;

        // If neither categories nor floor nor custom is set, fall through to
        // with_defaults() — the 8 v1 recognizers, no filter.
        if effective_cats.is_none() && floor.is_none() && self.custom.is_empty() {
            return Ok(Analyzer::with_defaults());
        }

        // Merge built-in registries (v1 + extended), filter, then box surviving entries.
        // Boxing post-filter avoids ~30 dropped allocations on narrow category requests.
        let cat_ok = |c: Category| effective_cats.as_ref().is_none_or(|cats| cats.contains(&c));
        let sev_ok = |s: Severity| floor.is_none_or(|min| s >= min);
        let mut kept: Vec<Box<dyn Recognizer>> = crate::recognizer::pattern::all()
            .into_iter()
            .filter(|r| cat_ok(r.category()) && sev_ok(r.severity()))
            .map(|r| Box::new(r) as Box<dyn Recognizer>)
            .collect();
        kept.extend(
            self.custom
                .into_iter()
                .filter(|r| cat_ok(r.category()) && sev_ok(r.severity())),
        );

        // If any requested category contributed zero recognizers, error
        // unless allow_empty_categories(true). Linear scan is fine for ≤7
        // requested categories against ~24 recognizers (~168 ops max).
        if !self.allow_empty_categories
            && let Some(cats) = &effective_cats
        {
            for &cat in cats {
                if !kept.iter().any(|r| r.category() == cat) {
                    return Err(AnalyzerBuildError::EmptyCategory(cat));
                }
            }
        }

        Ok(Analyzer { recognizers: kept })
    }
}
