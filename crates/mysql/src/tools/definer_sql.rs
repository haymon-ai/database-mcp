//! Shared SQL fragment for canonical DEFINER reconstruction.
//!
//! `information_schema.TRIGGERS.DEFINER` carries a `'user'@'host'` value. Both
//! `list_tables`'s `triggers_info` CTE and `list_triggers`'s detailed-mode SQL
//! reconstruct a `CREATE TRIGGER` text that needs the canonical
//! `SHOW CREATE TRIGGER` form for the DEFINER clause:
//!
//! ```text
//! DEFINER=`<user>`@`<host>`
//! ```
//!
//! User and host are backtick-quoted separately, embedded backticks doubled.
//! `SUBSTRING_INDEX(..., '@', 1)` returns the user part and
//! `SUBSTRING_INDEX(..., '@', -1)` returns the host part.

/// Builds a canonical DEFINER SQL fragment for the given column expression.
///
/// `definer_col` is the SQL column reference (e.g., `"tr.DEFINER"` or `"DEFINER"`).
/// The returned fragment is a comma-joined sequence of string literals and
/// `REPLACE(SUBSTRING_INDEX(...))` calls suitable for use inside a
/// `CONCAT(...)` argument list.
///
/// # Examples
///
/// ```ignore
/// let frag = definer_canonical_sql("tr.DEFINER");
/// // frag is suitable for inlining into:
/// //   CONCAT('CREATE ', <frag>, ' TRIGGER ...')
/// ```
pub(crate) fn definer_canonical_sql(definer_col: &str) -> String {
    format!(
        "'DEFINER=`', \
         REPLACE(SUBSTRING_INDEX({definer_col}, '@', 1), '`', '``'), \
         '`@`', \
         REPLACE(SUBSTRING_INDEX({definer_col}, '@', -1), '`', '``'), \
         '`'"
    )
}

#[cfg(test)]
mod tests {
    use super::definer_canonical_sql;

    #[test]
    fn fragment_renders_for_qualified_column() {
        let f = definer_canonical_sql("tr.DEFINER");
        assert!(f.contains("'DEFINER=`'"));
        assert!(f.contains("SUBSTRING_INDEX(tr.DEFINER, '@', 1)"));
        assert!(f.contains("SUBSTRING_INDEX(tr.DEFINER, '@', -1)"));
        assert!(f.contains("'`@`'"));
    }

    #[test]
    fn fragment_renders_for_bare_column() {
        let f = definer_canonical_sql("DEFINER");
        assert!(f.contains("'DEFINER=`'"));
        assert!(f.contains("SUBSTRING_INDEX(DEFINER, '@', 1)"));
        assert!(f.contains("SUBSTRING_INDEX(DEFINER, '@', -1)"));
    }

    #[test]
    fn fragment_doubles_embedded_backticks_in_user_and_host() {
        let f = definer_canonical_sql("tr.DEFINER");
        let occurrences = f.matches("REPLACE(SUBSTRING_INDEX").count();
        assert_eq!(occurrences, 2, "user + host both wrapped in REPLACE: {f}");
        assert_eq!(f.matches("'`', '``'").count(), 2);
    }
}
