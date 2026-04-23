//! PostgreSQL-specific MCP tool request and response types.
//!
//! These types include PostgreSQL-only parameters (like `cascade`, `search`,
//! `detailed`) and the two-shape [`TableEntries`] response body that are not
//! available on other backends.

use dbmcp_server::pagination::Cursor;
use rmcp::schemars;
use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Request for the `dropTable` tool.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DropTableRequest {
    /// Database containing the table. Defaults to the active database.
    #[serde(default)]
    pub database: Option<String>,
    /// Name of the table to drop. Must contain only alphanumeric characters and underscores.
    pub table: String,
    /// If true, use CASCADE to also drop dependent foreign key constraints. Defaults to false.
    #[serde(default)]
    pub cascade: bool,
}

/// Request for the Postgres `listTables` tool — extends the shared shape with `search` and `detailed`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListTablesRequest {
    /// Database to list tables from. Defaults to the active database.
    #[serde(default)]
    pub database: Option<String>,
    /// Opaque cursor from a prior response's `nextCursor`; omit for the first page.
    #[serde(default)]
    pub cursor: Option<Cursor>,
    /// Optional case-insensitive substring filter on table names.
    /// Wildcards `%` / `_` / `\` are treated as literal characters.
    #[serde(default)]
    pub search: Option<String>,
    /// When `true`, each returned entry is a full metadata object (columns,
    /// constraints, indexes, triggers, owner, comment, kind); when `false` or
    /// omitted, each entry is the bare table-name string.
    #[serde(default)]
    pub detailed: bool,
}

/// Response for the Postgres `listTables` tool.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListTablesResponse {
    /// Page of matching tables. Shape depends on the request's `detailed` flag.
    pub tables: TableEntries,
    /// Opaque cursor pointing to the next page. Absent when this is the final page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<Cursor>,
}

/// Two-shape table listing payload: bare names in brief mode, full objects in detailed mode.
///
/// Chosen by the handler based on [`ListTablesRequest::detailed`]. Serialises as an untagged
/// JSON array — callers see a list of strings or a list of objects, never a discriminator.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum TableEntries {
    /// Brief mode: sorted array of bare table-name strings.
    Brief(Vec<String>),
    /// Detailed mode: one object per table with full introspected metadata.
    Detailed(Vec<Value>),
}

impl TableEntries {
    /// Number of entries in the page, regardless of variant.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Brief(v) => v.len(),
            Self::Detailed(v) => v.len(),
        }
    }

    /// Whether the page contains no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the brief-mode names as a slice, or `None` in detailed mode.
    #[must_use]
    pub fn as_brief(&self) -> Option<&[String]> {
        if let Self::Brief(v) = self { Some(v) } else { None }
    }

    /// Returns the detailed-mode entries as a slice, or `None` in brief mode.
    #[must_use]
    pub fn as_detailed(&self) -> Option<&[Value]> {
        if let Self::Detailed(v) = self { Some(v) } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::{ListTablesRequest, ListTablesResponse, TableEntries};
    use serde_json::json;

    #[test]
    fn list_tables_request_defaults_to_brief_mode_without_search() {
        let req: ListTablesRequest = serde_json::from_str("{}").expect("empty object should parse");
        assert!(req.search.is_none());
        assert!(!req.detailed, "detailed must default to false");
    }

    #[test]
    fn list_tables_request_accepts_search_and_detailed() {
        let req: ListTablesRequest = serde_json::from_str(r#"{"search": "order", "detailed": true}"#).expect("parse");
        assert_eq!(req.search.as_deref(), Some("order"));
        assert!(req.detailed);
    }

    #[test]
    fn table_entries_brief_serializes_as_bare_string_array() {
        let entries = TableEntries::Brief(vec!["customers".into(), "orders".into()]);
        let value = serde_json::to_value(&entries).expect("serialize brief");
        assert_eq!(value, json!(["customers", "orders"]));
    }

    #[test]
    fn table_entries_detailed_serializes_as_object_array() {
        let entries = TableEntries::Detailed(vec![json!({"name": "orders", "kind": "TABLE"})]);
        let value = serde_json::to_value(&entries).expect("serialize detailed");
        assert_eq!(value, json!([{"name": "orders", "kind": "TABLE"}]));
    }

    #[test]
    fn table_entries_brief_empty_serializes_as_empty_array() {
        let entries = TableEntries::Brief(Vec::new());
        let value = serde_json::to_value(&entries).expect("serialize empty brief");
        assert_eq!(value, json!([]));
    }

    #[test]
    fn list_tables_response_brief_matches_legacy_wire_shape() {
        let response = ListTablesResponse {
            tables: TableEntries::Brief(vec!["a".into()]),
            next_cursor: None,
        };
        let value = serde_json::to_value(&response).expect("serialize response");
        assert_eq!(value, json!({"tables": ["a"]}));
    }
}
