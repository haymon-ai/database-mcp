//! SQL quoting and validation for identifiers and literals.

use database_mcp_server::AppError;
use sqlparser::dialect::Dialect;

/// Wraps `value` in the dialect's identifier quote character.
///
/// Derives the quote character from [`Dialect::identifier_quote_style`],
/// falling back to `"` (ANSI double-quote) when the dialect returns `None`.
/// Escapes internal occurrences of the quote character by doubling them.
#[must_use]
pub fn quote_ident(value: &str, dialect: &impl Dialect) -> String {
    let q = dialect.identifier_quote_style(value).unwrap_or('"');
    let mut out = String::with_capacity(value.len() + 2);
    out.push(q);
    for ch in value.chars() {
        if ch == q {
            out.push(q);
        }
        out.push(ch);
    }
    out.push(q);
    out
}

/// Wraps `value` in single quotes for use as a SQL string literal.
///
/// Escapes internal single quotes by doubling them.
#[must_use]
pub fn quote_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            out.push('\'');
        }
        out.push(ch);
    }
    out.push('\'');
    out
}

/// Validates that `name` is a non-empty identifier without control characters.
///
/// # Errors
///
/// Returns [`AppError::InvalidIdentifier`] if the name is empty,
/// whitespace-only, or contains control characters.
pub fn validate_ident(name: &str) -> Result<(), AppError> {
    if name.trim().is_empty() || name.chars().any(char::is_control) {
        return Err(AppError::InvalidIdentifier(name.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlparser::dialect::{MySqlDialect, PostgreSqlDialect, SQLiteDialect};

    use super::*;

    #[test]
    fn accepts_standard_names() {
        assert!(validate_ident("users").is_ok());
        assert!(validate_ident("my_table").is_ok());
        assert!(validate_ident("DB_123").is_ok());
    }

    #[test]
    fn accepts_hyphenated_names() {
        assert!(validate_ident("eu-docker").is_ok());
        assert!(validate_ident("access-logs").is_ok());
    }

    #[test]
    fn accepts_special_chars() {
        assert!(validate_ident("my.db").is_ok());
        assert!(validate_ident("123db").is_ok());
        assert!(validate_ident("café").is_ok());
        assert!(validate_ident("a b").is_ok());
    }

    #[test]
    fn rejects_empty() {
        assert!(validate_ident("").is_err());
    }

    #[test]
    fn rejects_whitespace_only() {
        assert!(validate_ident("   ").is_err());
        assert!(validate_ident("\t").is_err());
    }

    #[test]
    fn rejects_control_chars() {
        assert!(validate_ident("test\x00db").is_err());
        assert!(validate_ident("test\ndb").is_err());
        assert!(validate_ident("test\x1Fdb").is_err());
    }

    #[test]
    fn quote_with_postgres_dialect() {
        let d = PostgreSqlDialect {};
        assert_eq!(quote_ident("users", &d), "\"users\"");
        assert_eq!(quote_ident("eu-docker", &d), "\"eu-docker\"");
        assert_eq!(quote_ident("test\"db", &d), "\"test\"\"db\"");
    }

    #[test]
    fn quote_with_mysql_dialect() {
        let d = MySqlDialect {};
        assert_eq!(quote_ident("users", &d), "`users`");
        assert_eq!(quote_ident("test`db", &d), "`test``db`");
    }

    #[test]
    fn quote_with_sqlite_dialect() {
        let d = SQLiteDialect {};
        assert_eq!(quote_ident("users", &d), "`users`");
        assert_eq!(quote_ident("test`db", &d), "`test``db`");
    }

    #[test]
    fn quote_literal_escapes_single_quotes() {
        assert_eq!(quote_literal("my_db"), "'my_db'");
        assert_eq!(quote_literal(""), "''");
        assert_eq!(quote_literal("it's"), "'it''s'");
        assert_eq!(quote_literal("a'b'c"), "'a''b''c'");
    }
}
