//! SQLite-specific MCP tool request and response types.
//!
//! Unlike `MySQL` and `PostgreSQL`, `SQLite` operates on a single file and
//! has no database selection. These types omit the `database` field present
//! in the shared server types, and add the SQLite-specific [`TableEntries`]
//! two-shape payload used by the enriched `listTables` tool.

use dbmcp_server::pagination::Cursor;
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Request for the `dropTable` tool.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DropTableRequest {
    /// Name of the table to drop. Must contain only alphanumeric characters and underscores.
    pub table: String,
}

/// Request for the `SQLite` `listTables` tool — supports optional search filter and detailed mode.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListTablesRequest {
    /// Opaque pagination cursor. Omit (or pass `null`) for the first page.
    /// On subsequent calls, pass the `nextCursor` returned by the previous
    /// response verbatim. Cursors are opaque — do not parse, modify, or persist.
    #[serde(default)]
    pub cursor: Option<Cursor>,
    /// Optional case-insensitive filter on table names. The input is used within a `LIKE`
    /// clause: `%` matches any sequence of characters and `_` matches any single character.
    #[serde(default)]
    pub search: Option<String>,
    /// When `true`, each returned entry is a full metadata object (columns,
    /// constraints, indexes, triggers); when `false` or omitted, each entry
    /// is the bare table-name string.
    #[serde(default)]
    pub detailed: bool,
}

/// Response for the `SQLite` `listTables` tool.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListTablesResponse {
    /// Page of matching tables. Shape depends on the request's `detailed` flag.
    pub tables: TableEntries,
    /// Opaque cursor pointing to the next page. Absent when this is the final page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<Cursor>,
}

/// Two-shape table listing payload: bare names in brief mode, name-keyed map in detailed mode.
///
/// Chosen by the handler based on [`ListTablesRequest::detailed`]. Serialises untagged: brief
/// mode becomes a JSON array of strings, detailed mode becomes a JSON object whose keys are
/// table names and whose values are the per-table metadata.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum TableEntries {
    /// Brief mode: sorted array of bare table-name strings.
    Brief(Vec<String>),
    /// Detailed mode: name-keyed map; insertion order matches the SQL `ORDER BY` sort.
    Detailed(IndexMap<String, Value>),
}

impl TableEntries {
    /// Number of entries in the page, regardless of variant.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Brief(v) => v.len(),
            Self::Detailed(m) => m.len(),
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

    /// Returns the detailed-mode map of name → metadata, or `None` in brief mode.
    #[must_use]
    pub fn as_detailed(&self) -> Option<&IndexMap<String, Value>> {
        if let Self::Detailed(m) = self { Some(m) } else { None }
    }
}

/// Request for the `listViews` tool.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListViewsRequest {
    /// Opaque pagination cursor. Omit (or pass `null`) for the first page.
    /// On subsequent calls, pass the `nextCursor` returned by the previous
    /// response verbatim. Cursors are opaque — do not parse, modify, or persist.
    #[serde(default)]
    pub cursor: Option<Cursor>,
}

/// Request for the `listTriggers` tool.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListTriggersRequest {
    /// Opaque pagination cursor. Omit (or pass `null`) for the first page.
    /// On subsequent calls, pass the `nextCursor` returned by the previous
    /// response verbatim. Cursors are opaque — do not parse, modify, or persist.
    #[serde(default)]
    pub cursor: Option<Cursor>,
}

/// Request for the `writeQuery` tool.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueryRequest {
    /// The SQL query to execute.
    pub query: String,
}

/// Request for the `readQuery` tool.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReadQueryRequest {
    /// The SQL query to execute.
    pub query: String,
    /// Opaque pagination cursor. Omit (or pass `null`) for the first page.
    /// On subsequent calls, pass the `nextCursor` returned by the previous
    /// response verbatim. Cursors are opaque — do not parse, modify, or persist.
    /// Ignored for `EXPLAIN` statements.
    #[serde(default)]
    pub cursor: Option<Cursor>,
}

/// Request for the `explainQuery` tool.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExplainQueryRequest {
    /// The SQL query to explain.
    pub query: String,
}

#[cfg(test)]
mod tests {
    use super::{IndexMap, ListTablesRequest, ListTablesResponse, TableEntries};
    use serde_json::{Value, json};

    #[test]
    fn list_tables_request_defaults_to_brief_mode_without_search() {
        let req: ListTablesRequest = serde_json::from_str("{}").expect("empty object should parse");
        assert!(req.search.is_none());
        assert!(!req.detailed, "detailed must default to false");
    }

    #[test]
    fn list_tables_request_accepts_search_and_detailed() {
        let req: ListTablesRequest = serde_json::from_str(r#"{"search": "post", "detailed": true}"#).expect("parse");
        assert_eq!(req.search.as_deref(), Some("post"));
        assert!(req.detailed);
    }

    #[test]
    fn table_entries_brief_serializes_as_bare_string_array() {
        let entries = TableEntries::Brief(vec!["posts".into(), "users".into()]);
        let value = serde_json::to_value(&entries).expect("serialize brief");
        assert_eq!(value, json!(["posts", "users"]));
    }

    #[test]
    fn table_entries_detailed_serializes_as_keyed_object() {
        let entries = TableEntries::Detailed(IndexMap::from([("posts".into(), json!({"kind": "TABLE"}))]));
        let value = serde_json::to_value(&entries).expect("serialize detailed");
        assert_eq!(value, json!({"posts": {"kind": "TABLE"}}));
    }

    #[test]
    fn table_entries_detailed_empty_serializes_as_empty_object() {
        let value = serde_json::to_value(TableEntries::Detailed(IndexMap::new())).expect("serialize");
        assert_eq!(value, json!({}));
    }

    #[test]
    fn table_entries_detailed_preserves_insertion_order() {
        let map = IndexMap::from([
            ("c".into(), json!({})),
            ("a".into(), json!({})),
            ("b".into(), json!({})),
        ]);
        let serialized = serde_json::to_string(&TableEntries::Detailed(map)).expect("serialize");
        let positions = ["\"c\"", "\"a\"", "\"b\""].map(|k| serialized.find(k).expect(k));
        assert!(positions.is_sorted(), "insertion order not preserved: {serialized}");
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

    /// SC-008: detailed keyed payload must be strictly smaller than the prior array-of-objects
    /// form for a representative 10-table fixture. The saving is one `"name": "<table>",`
    /// fragment per entry; the contractual claim is the strict reduction, not a specific %.
    #[test]
    fn sc008_detailed_payload_strictly_smaller_than_array_form() {
        let metadata = json!({
            "schema": "main",
            "kind": "TABLE",
            "owner": null,
            "comment": null,
            "columns": [
                {"name": "id",         "dataType": "INTEGER", "ordinalPosition": 1, "nullable": false, "default": null, "comment": null},
                {"name": "created_at", "dataType": "TEXT",    "ordinalPosition": 2, "nullable": false, "default": "CURRENT_TIMESTAMP", "comment": null},
                {"name": "updated_at", "dataType": "TEXT",    "ordinalPosition": 3, "nullable": true,  "default": null, "comment": null},
            ],
            "constraints": [
                {"name": "PRIMARY", "type": "PRIMARY KEY", "columns": ["id"], "definition": "PRIMARY KEY (\"id\")"},
            ],
            "indexes": [
                {"name": "sqlite_autoindex_t_1", "columns": ["id"], "unique": true, "primary": true, "method": "btree", "definition": "CREATE UNIQUE INDEX \"sqlite_autoindex_t_1\" ON \"t\"(\"id\")"},
            ],
            "triggers": []
        });
        let tables = [
            "customers",
            "orders",
            "order_items",
            "products",
            "inventory",
            "suppliers",
            "shipments",
            "invoices",
            "payments",
            "audits",
        ];

        let new_map: IndexMap<String, Value> = tables.iter().map(|n| ((*n).into(), metadata.clone())).collect();
        let old: Vec<Value> = tables
            .iter()
            .map(|n| {
                let mut v = metadata.clone();
                v["name"] = json!(n);
                v
            })
            .collect();

        let new_len = serde_json::to_vec(&TableEntries::Detailed(new_map))
            .expect("serialize new")
            .len();
        let old_len = serde_json::to_vec(&old).expect("serialize old").len();
        assert!(new_len < old_len, "payload not smaller: new={new_len} old={old_len}");
    }
}
