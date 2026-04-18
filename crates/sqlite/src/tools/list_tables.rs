//! MCP tool: `list_tables`.

use std::borrow::Cow;
use std::sync::Arc;

use database_mcp_server::pagination::Cursor;
use database_mcp_server::types::ListTablesResponse;

use database_mcp_sql::Connection as _;
use rmcp::handler::server::router::tool::{AsyncTool, ToolBase};
use rmcp::model::{ErrorData, JsonObject, ToolAnnotations};

use crate::SqliteHandler;
use crate::types::ListTablesRequest;

/// Marker type for the `list_tables` MCP tool.
pub(crate) struct ListTablesTool;

impl ListTablesTool {
    const NAME: &'static str = "list_tables";
    const TITLE: &'static str = "List Tables";
    const DESCRIPTION: &'static str = r#"List all tables in the connected SQLite database. Use this tool to discover what tables are available before using other tools.

<usecase>
ALWAYS call this tool FIRST when:
- You need to explore what tables exist in the database
- You need a table name for get_table_schema or query tools
- The user asks what data is available
</usecase>

<examples>
✓ "What tables are in this database?"
✓ "Does a users table exist?" → list_tables to check
✗ "Show me the columns of users" → use get_table_schema instead
</examples>

<what_it_returns>
A sorted JSON array of table name strings.
</what_it_returns>

<pagination>
This tool paginates its response. If more tables exist than fit in one page, the response includes a `nextCursor` string — call `list_tables` again with that string as the `cursor` argument to fetch the next page. Iterate until `nextCursor` is absent.

Cursors are opaque: do not parse, modify, or persist them across sessions. Passing a malformed or stale cursor returns a JSON-RPC error (code -32602); recover by retrying without a cursor to restart from the first page.

Note: tables created or dropped between paginated calls may cause the same table to appear twice or to be skipped. Re-enumerate from a fresh call for a consistent snapshot.
</pagination>"#;
}

impl ToolBase for ListTablesTool {
    type Parameter = ListTablesRequest;
    type Output = ListTablesResponse;
    type Error = ErrorData;

    fn name() -> Cow<'static, str> {
        Self::NAME.into()
    }

    fn title() -> Option<String> {
        Some(Self::TITLE.into())
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(Self::DESCRIPTION.into())
    }

    fn input_schema() -> Option<Arc<JsonObject>> {
        None
    }

    fn annotations() -> Option<ToolAnnotations> {
        Some(
            ToolAnnotations::new()
                .read_only(true)
                .destructive(false)
                .idempotent(true)
                .open_world(false),
        )
    }
}

impl AsyncTool<SqliteHandler> for ListTablesTool {
    async fn invoke(handler: &SqliteHandler, params: Self::Parameter) -> Result<Self::Output, Self::Error> {
        handler.list_tables(&params).await
    }
}

impl SqliteHandler {
    /// Lists one page of tables in the connected database.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorData`] with code `-32602` if `request.cursor` is
    /// malformed, or an internal-error [`ErrorData`] if the underlying
    /// query fails.
    pub async fn list_tables(&self, request: &ListTablesRequest) -> Result<ListTablesResponse, ErrorData> {
        let offset = request.cursor.map_or(0, |c| c.offset);
        let page_size = usize::from(self.config.page_size);
        let fetch_limit = page_size + 1;
        let sql = format!(
            r"
            SELECT name
            FROM sqlite_master
            WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
            ORDER BY name
            LIMIT {fetch_limit} OFFSET {offset}",
        );
        let mut tables: Vec<String> = self.connection.fetch_scalar(sql.as_str(), None).await?;
        let next_cursor = if tables.len() > page_size {
            tables.truncate(page_size);
            Some(Cursor {
                offset: offset + page_size as u64,
            })
        } else {
            None
        };
        Ok(ListTablesResponse { tables, next_cursor })
    }
}

#[cfg(test)]
mod tests {
    use super::ListTablesTool;

    #[test]
    fn description_documents_pagination() {
        let desc = ListTablesTool::DESCRIPTION;
        assert!(desc.contains("nextCursor"), "description must mention `nextCursor`");
        assert!(desc.contains("cursor"), "description must document cursor semantics");
        assert!(
            desc.contains("-32602"),
            "description must mention the invalid-cursor error code"
        );
    }

    #[test]
    fn description_does_not_state_specific_page_size() {
        assert!(
            !ListTablesTool::DESCRIPTION.contains("100"),
            "description must not hard-state `100` tables — page size is operator-configurable"
        );
    }
}
