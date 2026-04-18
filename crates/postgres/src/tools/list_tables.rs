//! MCP tool: `list_tables`.

use std::borrow::Cow;

use database_mcp_server::pagination::Cursor;
use database_mcp_server::types::{ListTablesRequest, ListTablesResponse};
use database_mcp_sql::Connection as _;
use database_mcp_sql::sanitize::validate_ident;
use rmcp::handler::server::router::tool::{AsyncTool, ToolBase};
use rmcp::model::{ErrorData, ToolAnnotations};

use crate::PostgresHandler;

/// Marker type for the `list_tables` MCP tool.
pub(crate) struct ListTablesTool;

impl ListTablesTool {
    const NAME: &'static str = "list_tables";
    const TITLE: &'static str = "List Tables";
    const DESCRIPTION: &'static str = r#"List all tables in a specific database. Requires `database_name` — call `list_databases` first to discover available databases.

<usecase>
Use when:
- Exploring a database to find relevant tables
- Verifying a table exists before querying or inspecting it
- The user asks what tables are in a database
</usecase>

<examples>
✓ "What tables are in the mydb database?" → list_tables(database_name="mydb")
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

impl AsyncTool<PostgresHandler> for ListTablesTool {
    async fn invoke(handler: &PostgresHandler, params: Self::Parameter) -> Result<Self::Output, Self::Error> {
        handler.list_tables(&params).await
    }
}

impl PostgresHandler {
    /// Lists one page of tables in a database.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorData`] with code `-32602` if `request.cursor` is
    /// malformed, or an internal-error [`ErrorData`] if `database_name`
    /// is invalid or the underlying query fails.
    pub async fn list_tables(&self, request: &ListTablesRequest) -> Result<ListTablesResponse, ErrorData> {
        let ListTablesRequest { database_name, cursor } = request;

        let db = Some(database_name.trim()).filter(|s| !s.is_empty());
        if let Some(name) = &db {
            validate_ident(name)?;
        }
        let offset = cursor.map_or(0, |c| c.offset);
        let page_size = usize::from(self.config.page_size);
        let fetch_limit = page_size + 1;
        let sql = format!(
            r"
            SELECT tablename
            FROM pg_tables
            WHERE schemaname = 'public'
            ORDER BY tablename
            LIMIT {fetch_limit} OFFSET {offset}",
        );
        let mut tables: Vec<String> = self.connection.fetch_scalar(sql.as_str(), db).await?;
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
