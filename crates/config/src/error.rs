//! Configuration validation errors and their non-empty-collection wrapper.
//!
//! [`ConfigError`] enumerates rules reachable past clap parsing.
//! [`ConfigErrors`] is a non-empty collection wrapper used as the
//! `Error` type for every per-section `TryFrom<&FooArguments> for FooConfig`
//! impl in the binary crate, and as the return type of every
//! [`DatabaseConfig::validate`] / [`HttpConfig::validate`] /
//! [`PiiConfig::validate`] call.
//!
//! [`DatabaseConfig::validate`]: crate::DatabaseConfig::validate
//! [`HttpConfig::validate`]: crate::HttpConfig::validate
//! [`PiiConfig::validate`]: crate::PiiConfig::validate

/// Errors produced by the `Arguments`-to-`Config` conversions.
///
/// Carries only rules reachable past clap parsing. Rules clap already
/// rejects (integer ranges, enum membership) are not represented here.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// `DB_NAME` is required for `SQLite`.
    #[error("DB_NAME (file path) is required for SQLite")]
    MissingSqliteDbName,

    /// SSL certificate file not found.
    #[error("{0} file not found: {1}")]
    SslCertNotFound(String, String),

    /// HTTP bind host is empty or whitespace.
    #[error("HTTP_HOST must not be empty")]
    EmptyHttpHost,
}

/// Non-empty collection of configuration errors, ordered database → http → pii.
///
/// Externally observed wrappers always carry ≥ 1 error — [`Self::from_vec`]
/// returns `None` for an empty input. Each transport's `TryFrom<&Command>
/// for Config` impl owns its own multi-section accumulation and returns
/// `Ok(value)` when nothing was collected, never an empty wrapper.
///
/// `Display` renders each contained [`ConfigError`] on its own line, joined
/// with `\n`, in the order supplied — no header, no count, no trailing newline.
#[derive(Debug, thiserror::Error)]
#[error("{}", ErrorList(&self.0))]
pub struct ConfigErrors(Vec<ConfigError>);

struct ErrorList<'a>(&'a [ConfigError]);

impl std::fmt::Display for ErrorList<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, err) in self.0.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{err}")?;
        }
        Ok(())
    }
}

impl ConfigErrors {
    /// Wraps a single [`ConfigError`].
    #[must_use]
    pub fn single(err: ConfigError) -> Self {
        Self(vec![err])
    }

    /// Wraps a non-empty `Vec`. Returns `None` when `errors` is empty.
    #[must_use]
    pub fn from_vec(errors: Vec<ConfigError>) -> Option<Self> {
        if errors.is_empty() { None } else { Some(Self(errors)) }
    }

    /// Iterates contained errors in their stored order.
    pub fn iter(&self) -> impl Iterator<Item = &ConfigError> + '_ {
        self.0.iter()
    }

    /// Returns the number of contained errors. Always ≥ 1 by construction.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Always `false` by construction — externally observed wrappers are non-empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Appends every error from `other`, preserving order.
    pub fn extend(&mut self, other: Self) {
        self.0.extend(other.0);
    }
}

impl From<ConfigError> for ConfigErrors {
    fn from(err: ConfigError) -> Self {
        Self::single(err)
    }
}

impl IntoIterator for ConfigErrors {
    type Item = ConfigError;
    type IntoIter = std::vec::IntoIter<ConfigError>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a ConfigErrors {
    type Item = &'a ConfigError;
    type IntoIter = std::slice::Iter<'a, ConfigError>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn missing_name() -> ConfigError {
        ConfigError::MissingSqliteDbName
    }

    fn missing_ca() -> ConfigError {
        ConfigError::SslCertNotFound("DB_SSL_CA".into(), "/nope/ca.pem".into())
    }

    fn missing_cert() -> ConfigError {
        ConfigError::SslCertNotFound("DB_SSL_CERT".into(), "/nope/cert.pem".into())
    }

    #[test]
    fn from_vec_empty_is_none() {
        assert!(ConfigErrors::from_vec(Vec::new()).is_none());
    }

    #[test]
    fn from_vec_non_empty_preserves_order() {
        let errors = ConfigErrors::from_vec(vec![missing_name(), missing_ca()]).expect("non-empty");
        assert_eq!(errors.len(), 2);
        let collected: Vec<_> = errors.iter().collect();
        assert!(matches!(collected[0], ConfigError::MissingSqliteDbName));
        assert!(matches!(collected[1], ConfigError::SslCertNotFound(_, _)));
    }

    #[test]
    fn single_yields_one_error() {
        let errors = ConfigErrors::single(missing_name());
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn display_n1_equals_inner_verbatim() {
        let errors = ConfigErrors::single(missing_name());
        assert_eq!(errors.to_string(), missing_name().to_string());
        assert!(!errors.to_string().ends_with('\n'));
    }

    #[test]
    fn display_n2_joined_by_newline_no_header_no_trailing_newline() {
        let errors = ConfigErrors::from_vec(vec![missing_name(), missing_ca()]).expect("non-empty");
        let rendered = errors.to_string();
        assert_eq!(
            rendered,
            format!("{}\n{}", missing_name(), missing_ca()),
            "n=2 must be joined with single \\n, no header, no trailing newline"
        );
        assert!(!rendered.ends_with('\n'));
    }

    #[test]
    fn extend_appends_in_order() {
        let mut a = ConfigErrors::single(missing_name());
        let b = ConfigErrors::from_vec(vec![missing_ca(), missing_cert()]).expect("non-empty");
        a.extend(b);
        assert_eq!(a.len(), 3);
        let collected: Vec<_> = a.iter().collect();
        assert!(matches!(collected[0], ConfigError::MissingSqliteDbName));
        assert!(matches!(
            collected[1],
            ConfigError::SslCertNotFound(name, _) if name == "DB_SSL_CA"
        ));
        assert!(matches!(
            collected[2],
            ConfigError::SslCertNotFound(name, _) if name == "DB_SSL_CERT"
        ));
    }

    #[test]
    fn into_iterator_owned_yields_in_stored_order() {
        let errors = ConfigErrors::from_vec(vec![missing_name(), missing_ca()]).expect("non-empty");
        let collected: Vec<ConfigError> = errors.into_iter().collect();
        assert!(matches!(collected[0], ConfigError::MissingSqliteDbName));
        assert!(matches!(collected[1], ConfigError::SslCertNotFound(_, _)));
    }

    #[test]
    fn into_iterator_borrowed_yields_in_stored_order() {
        let errors = ConfigErrors::from_vec(vec![missing_name(), missing_ca()]).expect("non-empty");
        let collected: Vec<&ConfigError> = (&errors).into_iter().collect();
        assert!(matches!(collected[0], ConfigError::MissingSqliteDbName));
        assert!(matches!(collected[1], ConfigError::SslCertNotFound(_, _)));
    }

    #[test]
    fn from_config_error_yields_single() {
        let errors: ConfigErrors = missing_name().into();
        assert_eq!(errors.len(), 1);
    }
}
