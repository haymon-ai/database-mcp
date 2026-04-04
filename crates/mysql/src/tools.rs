//! MCP tool definitions for the MySQL/MariaDB backend.
//!
//! Each tool is a unit struct implementing [`ToolBase`] and [`AsyncTool`].

use std::borrow::Cow;

use database_mcp_server::map_error;
use database_mcp_server::types::{CreateDatabaseRequest, GetTableSchemaRequest, ListTablesRequest, QueryRequest};
use rmcp::handler::server::router::tool::{AsyncTool, ToolBase};
use rmcp::model::{ErrorData, ToolAnnotations};

use database_mcp_server::Server;

use super::MysqlAdapter;

/// Type alias kept module-private for brevity in tool impls.
type MysqlService = Server<MysqlAdapter>;

/// Tool to list all accessible databases.
pub(super) struct ListDatabasesTool;

impl ListDatabasesTool {
    const NAME: &'static str = "list_databases";
    const DESCRIPTION: &'static str = "List all accessible databases on the connected database server. Call this first to discover available database names.";
}

impl ToolBase for ListDatabasesTool {
    type Parameter = ();
    type Output = String;
    type Error = ErrorData;

    fn name() -> Cow<'static, str> {
        Self::NAME.into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(Self::DESCRIPTION.into())
    }

    fn input_schema() -> Option<std::sync::Arc<rmcp::model::JsonObject>> {
        None
    }

    fn output_schema() -> Option<std::sync::Arc<rmcp::model::JsonObject>> {
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

impl AsyncTool<MysqlService> for ListDatabasesTool {
    async fn invoke(service: &MysqlService, _req: ()) -> Result<String, ErrorData> {
        let result = service.backend.list_databases().await.map_err(map_error)?;
        serde_json::to_string_pretty(&result).map_err(map_error)
    }
}

/// Tool to list all tables in a database.
pub(super) struct ListTablesTool;

impl ListTablesTool {
    const NAME: &'static str = "list_tables";
    const DESCRIPTION: &'static str =
        "List all tables in a specific database. Requires database_name from list_databases.";
}

impl ToolBase for ListTablesTool {
    type Parameter = ListTablesRequest;
    type Output = String;
    type Error = ErrorData;

    fn name() -> Cow<'static, str> {
        Self::NAME.into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(Self::DESCRIPTION.into())
    }

    fn output_schema() -> Option<std::sync::Arc<rmcp::model::JsonObject>> {
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

impl AsyncTool<MysqlService> for ListTablesTool {
    async fn invoke(service: &MysqlService, req: ListTablesRequest) -> Result<String, ErrorData> {
        let result = service
            .backend
            .list_tables(&req.database_name)
            .await
            .map_err(map_error)?;
        serde_json::to_string_pretty(&result).map_err(map_error)
    }
}

/// Tool to get column definitions for a table.
pub(super) struct GetTableSchemaTool;

impl GetTableSchemaTool {
    const NAME: &'static str = "get_table_schema";
    const DESCRIPTION: &'static str = "Get column definitions (type, nullable, key, default) and foreign key relationships for a table. Requires database_name and table_name.";
}

impl ToolBase for GetTableSchemaTool {
    type Parameter = GetTableSchemaRequest;
    type Output = String;
    type Error = ErrorData;

    fn name() -> Cow<'static, str> {
        Self::NAME.into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(Self::DESCRIPTION.into())
    }

    fn output_schema() -> Option<std::sync::Arc<rmcp::model::JsonObject>> {
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

impl AsyncTool<MysqlService> for GetTableSchemaTool {
    async fn invoke(service: &MysqlService, req: GetTableSchemaRequest) -> Result<String, ErrorData> {
        let result = service
            .backend
            .get_table_schema(&req.database_name, &req.table_name)
            .await
            .map_err(map_error)?;
        serde_json::to_string_pretty(&result).map_err(map_error)
    }
}

/// Tool to execute a read-only SQL query.
pub(super) struct ReadQueryTool;

impl ReadQueryTool {
    const NAME: &'static str = "read_query";
    const DESCRIPTION: &'static str = "Execute a read-only SQL query (SELECT, SHOW, DESCRIBE, USE, EXPLAIN).";
}

impl ToolBase for ReadQueryTool {
    type Parameter = QueryRequest;
    type Output = String;
    type Error = ErrorData;

    fn name() -> Cow<'static, str> {
        Self::NAME.into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(Self::DESCRIPTION.into())
    }

    fn output_schema() -> Option<std::sync::Arc<rmcp::model::JsonObject>> {
        None
    }

    fn annotations() -> Option<ToolAnnotations> {
        Some(
            ToolAnnotations::new()
                .read_only(true)
                .destructive(false)
                .idempotent(true)
                .open_world(true),
        )
    }
}

impl AsyncTool<MysqlService> for ReadQueryTool {
    async fn invoke(service: &MysqlService, req: QueryRequest) -> Result<String, ErrorData> {
        database_mcp_sql::validation::validate_read_only_with_dialect(
            &req.sql_query,
            &sqlparser::dialect::MySqlDialect {},
        )
        .map_err(map_error)?;

        let db = if req.database_name.is_empty() {
            None
        } else {
            Some(req.database_name.as_str())
        };
        let result = service
            .backend
            .execute_query(&req.sql_query, db)
            .await
            .map_err(map_error)?;
        serde_json::to_string_pretty(&result).map_err(map_error)
    }
}

/// Tool to execute a write SQL query.
pub(super) struct WriteQueryTool;

impl WriteQueryTool {
    const NAME: &'static str = "write_query";
    const DESCRIPTION: &'static str = "Execute a write SQL query (INSERT, UPDATE, DELETE, CREATE, ALTER, DROP).";
}

impl ToolBase for WriteQueryTool {
    type Parameter = QueryRequest;
    type Output = String;
    type Error = ErrorData;

    fn name() -> Cow<'static, str> {
        Self::NAME.into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(Self::DESCRIPTION.into())
    }

    fn output_schema() -> Option<std::sync::Arc<rmcp::model::JsonObject>> {
        None
    }

    fn annotations() -> Option<ToolAnnotations> {
        Some(
            ToolAnnotations::new()
                .read_only(false)
                .destructive(true)
                .idempotent(false)
                .open_world(true),
        )
    }
}

impl AsyncTool<MysqlService> for WriteQueryTool {
    async fn invoke(service: &MysqlService, req: QueryRequest) -> Result<String, ErrorData> {
        let db = if req.database_name.is_empty() {
            None
        } else {
            Some(req.database_name.as_str())
        };
        let result = service
            .backend
            .execute_query(&req.sql_query, db)
            .await
            .map_err(map_error)?;
        serde_json::to_string_pretty(&result).map_err(map_error)
    }
}

/// Tool to create a new database.
pub(super) struct CreateDatabaseTool;

impl CreateDatabaseTool {
    const NAME: &'static str = "create_database";
    const DESCRIPTION: &'static str = "Create a new database. Not supported for SQLite.";
}

impl ToolBase for CreateDatabaseTool {
    type Parameter = CreateDatabaseRequest;
    type Output = String;
    type Error = ErrorData;

    fn name() -> Cow<'static, str> {
        Self::NAME.into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(Self::DESCRIPTION.into())
    }

    fn output_schema() -> Option<std::sync::Arc<rmcp::model::JsonObject>> {
        None
    }

    fn annotations() -> Option<ToolAnnotations> {
        Some(
            ToolAnnotations::new()
                .read_only(false)
                .destructive(false)
                .idempotent(false)
                .open_world(false),
        )
    }
}

impl AsyncTool<MysqlService> for CreateDatabaseTool {
    async fn invoke(service: &MysqlService, req: CreateDatabaseRequest) -> Result<String, ErrorData> {
        let result = service
            .backend
            .create_database(&req.database_name)
            .await
            .map_err(map_error)?;
        serde_json::to_string_pretty(&result).map_err(map_error)
    }
}
