//! Shared MCP tool definitions and route constructors.
//!
//! Provides reusable [`ToolRoute`] factory functions for tools common
//! across database backends. Each backend imports the routes it needs
//! and assembles its own [`ToolRouter`].

use std::sync::Arc;

use rmcp::handler::server::common::{FromContextPart, schema_for_empty_input, schema_for_type};
use rmcp::handler::server::router::tool::{ToolRoute, ToolRouter};
use rmcp::handler::server::tool::ToolCallContext;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::Tool;
use rmcp::schemars::JsonSchema;
use serde_json::Map as JsonObject;

use super::{CreateDatabaseRequest, GetTableSchemaRequest, ListTablesRequest, QueryRequest, Server};

/// Returns the JSON Schema for `Parameters<T>`.
fn schema_for<T: JsonSchema + 'static>() -> Arc<JsonObject<String, serde_json::Value>> {
    schema_for_type::<Parameters<T>>()
}

/// Route for the `list_databases` tool.
#[must_use]
pub fn list_databases_route() -> ToolRoute<Server> {
    ToolRoute::new_dyn(
        Tool::new(
            "list_databases",
            "List all accessible databases on the connected database server. Call this first to discover available database names.",
            schema_for_empty_input(),
        ),
        |ctx: ToolCallContext<'_, Server>| {
            let server = ctx.service;
            Box::pin(async move { server.list_databases().await })
        },
    )
}

/// Route for the `list_tables` tool.
#[must_use]
pub fn list_tables_route() -> ToolRoute<Server> {
    ToolRoute::new_dyn(
        Tool::new(
            "list_tables",
            "List all tables in a specific database. Requires database_name from list_databases.",
            schema_for::<ListTablesRequest>(),
        ),
        |mut ctx: ToolCallContext<'_, Server>| {
            let params = Parameters::<ListTablesRequest>::from_context_part(&mut ctx);
            let server = ctx.service;
            Box::pin(async move {
                let params = params?;
                server.list_tables(params).await
            })
        },
    )
}

/// Route for the `get_table_schema` tool.
#[must_use]
pub fn get_table_schema_route() -> ToolRoute<Server> {
    ToolRoute::new_dyn(
        Tool::new(
            "get_table_schema",
            "Get column definitions (type, nullable, key, default) for a table. Requires database_name and table_name.",
            schema_for::<GetTableSchemaRequest>(),
        ),
        |mut ctx: ToolCallContext<'_, Server>| {
            let params = Parameters::<GetTableSchemaRequest>::from_context_part(&mut ctx);
            let server = ctx.service;
            Box::pin(async move {
                let params = params?;
                server.get_table_schema(params).await
            })
        },
    )
}

/// Route for the `get_table_schema_with_relations` tool.
#[must_use]
pub fn get_table_schema_with_relations_route() -> ToolRoute<Server> {
    ToolRoute::new_dyn(
        Tool::new(
            "get_table_schema_with_relations",
            "Get column definitions plus foreign key relationships for a table. Requires database_name and table_name.",
            schema_for::<GetTableSchemaRequest>(),
        ),
        |mut ctx: ToolCallContext<'_, Server>| {
            let params = Parameters::<GetTableSchemaRequest>::from_context_part(&mut ctx);
            let server = ctx.service;
            Box::pin(async move {
                let params = params?;
                server.get_table_schema_with_relations(params).await
            })
        },
    )
}

/// Route for the `read_query` tool.
#[must_use]
pub fn read_query_route() -> ToolRoute<Server> {
    ToolRoute::new_dyn(
        Tool::new(
            "read_query",
            "Execute a read-only SQL query (SELECT, SHOW, DESCRIBE, USE, EXPLAIN).",
            schema_for::<QueryRequest>(),
        ),
        |mut ctx: ToolCallContext<'_, Server>| {
            let params = Parameters::<QueryRequest>::from_context_part(&mut ctx);
            let server = ctx.service;
            Box::pin(async move {
                let params = params?;
                server.read_query(params).await
            })
        },
    )
}

/// Route for the `write_query` tool.
#[must_use]
pub fn write_query_route() -> ToolRoute<Server> {
    ToolRoute::new_dyn(
        Tool::new(
            "write_query",
            "Execute a write SQL query (INSERT, UPDATE, DELETE, CREATE, ALTER, DROP).",
            schema_for::<QueryRequest>(),
        ),
        |mut ctx: ToolCallContext<'_, Server>| {
            let params = Parameters::<QueryRequest>::from_context_part(&mut ctx);
            let server = ctx.service;
            Box::pin(async move {
                let params = params?;
                server.write_query(params).await
            })
        },
    )
}

/// Route for the `create_database` tool.
#[must_use]
pub fn create_database_route() -> ToolRoute<Server> {
    ToolRoute::new_dyn(
        Tool::new(
            "create_database",
            "Create a new database. Not supported for SQLite.",
            schema_for::<CreateDatabaseRequest>(),
        ),
        |mut ctx: ToolCallContext<'_, Server>| {
            let params = Parameters::<CreateDatabaseRequest>::from_context_part(&mut ctx);
            let server = ctx.service;
            Box::pin(async move {
                let params = params?;
                server.create_database(params).await
            })
        },
    )
}

/// Builds a [`ToolRouter`] with the common tool set.
///
/// All backends share the same 5 read tools. Write tools are added
/// conditionally based on `read_only` and `supports_create_database`.
#[must_use]
pub fn build_common_tool_router(read_only: bool, supports_create_database: bool) -> ToolRouter<Server> {
    let mut router = ToolRouter::new();
    router.add_route(list_databases_route());
    router.add_route(list_tables_route());
    router.add_route(get_table_schema_route());
    router.add_route(get_table_schema_with_relations_route());
    router.add_route(read_query_route());

    if !read_only {
        router.add_route(write_query_route());
        if supports_create_database {
            router.add_route(create_database_route());
        }
    }

    router
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_databases_route_has_correct_name_and_empty_schema() {
        let route = list_databases_route();
        assert_eq!(route.attr.name.as_ref(), "list_databases");
        assert!(
            route
                .attr
                .description
                .as_deref()
                .is_some_and(|d| d.contains("List all accessible databases")),
            "description should mention listing databases"
        );
        // Empty input schema should have type "object" with empty or no required properties
        let schema = &route.attr.input_schema;
        assert_eq!(schema.get("type").and_then(|v| v.as_str()), Some("object"));
    }

    #[test]
    fn list_tables_route_has_correct_name_and_schema() {
        let route = list_tables_route();
        assert_eq!(route.attr.name.as_ref(), "list_tables");
        let schema = &route.attr.input_schema;
        let props = schema.get("properties").and_then(|v| v.as_object());
        assert!(
            props.is_some_and(|p| p.contains_key("database_name")),
            "schema should have database_name property"
        );
    }

    #[test]
    fn get_table_schema_route_has_correct_name_and_schema() {
        let route = get_table_schema_route();
        assert_eq!(route.attr.name.as_ref(), "get_table_schema");
        let schema = &route.attr.input_schema;
        let props = schema.get("properties").and_then(|v| v.as_object());
        assert!(
            props.is_some_and(|p| p.contains_key("database_name") && p.contains_key("table_name")),
            "schema should have database_name and table_name properties"
        );
    }

    #[test]
    fn get_table_schema_with_relations_route_has_correct_name() {
        let route = get_table_schema_with_relations_route();
        assert_eq!(route.attr.name.as_ref(), "get_table_schema_with_relations");
        let schema = &route.attr.input_schema;
        let props = schema.get("properties").and_then(|v| v.as_object());
        assert!(
            props.is_some_and(|p| p.contains_key("database_name") && p.contains_key("table_name")),
            "schema should have database_name and table_name properties"
        );
    }

    #[test]
    fn read_query_route_has_correct_name_and_schema() {
        let route = read_query_route();
        assert_eq!(route.attr.name.as_ref(), "read_query");
        assert!(
            route
                .attr
                .description
                .as_deref()
                .is_some_and(|d| d.contains("read-only")),
            "description should mention read-only"
        );
        let schema = &route.attr.input_schema;
        let props = schema.get("properties").and_then(|v| v.as_object());
        assert!(
            props.is_some_and(|p| p.contains_key("sql_query") && p.contains_key("database_name")),
            "schema should have sql_query and database_name properties"
        );
    }

    #[test]
    fn write_query_route_has_correct_name_and_schema() {
        let route = write_query_route();
        assert_eq!(route.attr.name.as_ref(), "write_query");
        assert!(
            route.attr.description.as_deref().is_some_and(|d| d.contains("write")),
            "description should mention write"
        );
        let schema = &route.attr.input_schema;
        let props = schema.get("properties").and_then(|v| v.as_object());
        assert!(
            props.is_some_and(|p| p.contains_key("sql_query") && p.contains_key("database_name")),
            "schema should have sql_query and database_name properties"
        );
    }

    #[test]
    fn create_database_route_has_correct_name_and_schema() {
        let route = create_database_route();
        assert_eq!(route.attr.name.as_ref(), "create_database");
        assert!(
            route.attr.description.as_deref().is_some_and(|d| d.contains("SQLite")),
            "description should mention SQLite not supported"
        );
        let schema = &route.attr.input_schema;
        let props = schema.get("properties").and_then(|v| v.as_object());
        assert!(
            props.is_some_and(|p| p.contains_key("database_name")),
            "schema should have database_name property"
        );
    }

    // --- build_common_tool_router tests ---

    /// Helper to collect tool names from a router.
    fn tool_names(read_only: bool, supports_create_db: bool) -> Vec<String> {
        build_common_tool_router(read_only, supports_create_db)
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect()
    }

    #[test]
    fn common_router_read_only_returns_5_read_tools() {
        let names = tool_names(true, true);
        assert_eq!(names.len(), 5);
        assert!(names.contains(&"list_databases".to_string()));
        assert!(names.contains(&"list_tables".to_string()));
        assert!(names.contains(&"get_table_schema".to_string()));
        assert!(names.contains(&"get_table_schema_with_relations".to_string()));
        assert!(names.contains(&"read_query".to_string()));
    }

    #[test]
    fn common_router_read_only_excludes_write_tools() {
        let names = tool_names(true, true);
        assert!(!names.contains(&"write_query".to_string()));
        assert!(!names.contains(&"create_database".to_string()));
    }

    #[test]
    fn common_router_read_only_without_create_db_returns_5_tools() {
        // SQLite read-only: same 5 tools regardless of create_db flag
        let names = tool_names(true, false);
        assert_eq!(names.len(), 5);
        assert!(!names.contains(&"write_query".to_string()));
        assert!(!names.contains(&"create_database".to_string()));
    }

    #[test]
    fn common_router_read_write_with_create_db_returns_7_tools() {
        // MySQL/Postgres non-read-only
        let names = tool_names(false, true);
        assert_eq!(names.len(), 7);
        assert!(names.contains(&"write_query".to_string()));
        assert!(names.contains(&"create_database".to_string()));
    }

    #[test]
    fn common_router_read_write_without_create_db_returns_6_tools() {
        // SQLite non-read-only
        let names = tool_names(false, false);
        assert_eq!(names.len(), 6);
        assert!(names.contains(&"write_query".to_string()));
        assert!(!names.contains(&"create_database".to_string()));
    }

    // --- route schema tests ---

    #[test]
    fn read_and_write_query_share_same_schema_shape() {
        let read = read_query_route();
        let write = write_query_route();
        let read_props = read.attr.input_schema.get("properties").and_then(|v| v.as_object());
        let write_props = write.attr.input_schema.get("properties").and_then(|v| v.as_object());
        assert!(read_props.is_some());
        assert_eq!(
            read_props.map(|p| p.keys().collect::<std::collections::BTreeSet<_>>()),
            write_props.map(|p| p.keys().collect::<std::collections::BTreeSet<_>>()),
            "read_query and write_query should have the same input schema properties"
        );
    }
}
