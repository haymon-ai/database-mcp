//! `SQLite` backend implementation via sqlx.
//!
//! Implements [`McpBackend`] for `SQLite` file-based databases.

use std::collections::HashMap;
use std::sync::Arc;

use backend::error::AppError;
use backend::identifier::validate_identifier;
use backend::types::{GetTableSchemaRequest, ListTablesRequest, QueryRequest};
use config::DatabaseConfig;
use rmcp::handler::server::common::{FromContextPart, schema_for_type};
use rmcp::handler::server::router::tool::{ToolRoute, ToolRouter};
use rmcp::handler::server::tool::ToolCallContext;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, Tool, ToolAnnotations};
use rmcp::schemars::JsonSchema;
use serde_json::{Map as JsonObject, Value, json};
use server::server::map_error;
use server::{McpBackend, Server};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow};
use sqlx::{Row, SqlitePool};
use sqlx_to_json::RowExt;
use tracing::info;

/// Builds [`SqliteConnectOptions`] from a [`DatabaseConfig`].
fn connect_options(config: &DatabaseConfig) -> SqliteConnectOptions {
    let name = config.name.as_deref().unwrap_or_default();
    SqliteConnectOptions::new().filename(name)
}

/// `SQLite` file-based database backend.
#[derive(Clone)]
pub struct SqliteBackend {
    pool: SqlitePool,
    pub read_only: bool,
}

impl std::fmt::Debug for SqliteBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteBackend")
            .field("read_only", &self.read_only)
            .finish_non_exhaustive()
    }
}

impl SqliteBackend {
    /// Creates a lazy in-memory backend for tests.
    #[cfg(test)]
    pub(crate) fn in_memory(read_only: bool) -> Self {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_lazy("sqlite::memory:")
            .expect("in-memory SQLite");
        Self { pool, read_only }
    }

    /// Creates a new `SQLite` backend from configuration.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if the database file cannot be opened.
    pub async fn new(config: &DatabaseConfig) -> Result<Self, AppError> {
        let name = config.name.as_deref().unwrap_or_default();
        let pool = SqlitePoolOptions::new()
            .max_connections(1) // SQLite is single-writer
            .connect_with(connect_options(config))
            .await
            .map_err(|e| AppError::Connection(format!("Failed to open SQLite: {e}")))?;

        info!("SQLite connection initialized: {name}");

        Ok(Self {
            pool,
            read_only: config.read_only,
        })
    }
}

impl SqliteBackend {
    /// Wraps `name` in double quotes for safe use in `SQLite` SQL statements.
    ///
    /// Escapes internal double quotes by doubling them.
    fn quote_identifier(name: &str) -> String {
        let escaped = name.replace('"', "\"\"");
        format!("\"{escaped}\"")
    }
}

/// Returns the JSON Schema for `Parameters<T>`.
fn schema_for<T: JsonSchema + 'static>() -> Arc<JsonObject<String, serde_json::Value>> {
    schema_for_type::<Parameters<T>>()
}

impl SqliteBackend {
    /// Lists all tables in a database.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if the identifier is invalid or the query fails.
    pub async fn list_tables(&self, _database: &str) -> Result<Vec<String>, AppError> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Query(e.to_string()))?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    /// Returns column definitions with foreign key relationships.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if validation fails or the query errors.
    pub async fn get_table_schema(&self, _database: &str, table: &str) -> Result<Value, AppError> {
        validate_identifier(table)?;

        // 1. Get basic schema
        let rows: Vec<SqliteRow> = sqlx::query(&format!("PRAGMA table_info({})", Self::quote_identifier(table)))
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::Query(e.to_string()))?;

        if rows.is_empty() {
            return Err(AppError::TableNotFound(table.to_string()));
        }

        let mut columns: HashMap<String, Value> = HashMap::new();
        for row in &rows {
            let col_name: String = row.try_get("name").unwrap_or_default();
            let col_type: String = row.try_get("type").unwrap_or_default();
            let notnull: i32 = row.try_get("notnull").unwrap_or(0);
            let default: Option<String> = row.try_get("dflt_value").ok();
            let pk: i32 = row.try_get("pk").unwrap_or(0);
            columns.insert(
                col_name,
                json!({
                    "type": col_type,
                    "nullable": notnull == 0,
                    "key": if pk > 0 { "PRI" } else { "" },
                    "default": default,
                    "extra": Value::Null,
                    "foreign_key": Value::Null,
                }),
            );
        }

        // 2. Get FK info via PRAGMA
        let fk_rows: Vec<SqliteRow> =
            sqlx::query(&format!("PRAGMA foreign_key_list({})", Self::quote_identifier(table)))
                .fetch_all(&self.pool)
                .await
                .map_err(|e| AppError::Query(e.to_string()))?;

        for fk_row in &fk_rows {
            let from_col: String = fk_row.try_get("from").unwrap_or_default();
            if let Some(col_info) = columns.get_mut(&from_col)
                && let Some(obj) = col_info.as_object_mut()
            {
                let ref_table: String = fk_row.try_get("table").unwrap_or_default();
                let ref_col: String = fk_row.try_get("to").unwrap_or_default();
                let on_update: String = fk_row.try_get("on_update").unwrap_or_default();
                let on_delete: String = fk_row.try_get("on_delete").unwrap_or_default();
                obj.insert(
                    "foreign_key".to_string(),
                    json!({
                        "constraint_name": Value::Null,
                        "referenced_table": ref_table,
                        "referenced_column": ref_col,
                        "on_update": on_update,
                        "on_delete": on_delete,
                    }),
                );
            }
        }

        Ok(json!({
            "table_name": table,
            "columns": columns,
        }))
    }

    /// Executes a SQL query and returns rows as JSON.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if the query fails.
    pub async fn execute_query(&self, sql: &str, _database: Option<&str>) -> Result<Value, AppError> {
        let rows: Vec<SqliteRow> = sqlx::query(sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::Query(e.to_string()))?;
        Ok(Value::Array(rows.iter().map(RowExt::to_json).collect()))
    }
}

impl McpBackend for SqliteBackend {
    #[allow(clippy::too_many_lines)]
    fn register_tools(&self, router: &mut ToolRouter<Server>) {
        // No list_databases — SQLite is single-database

        // list_tables
        let b = self.clone();
        router.add_route(ToolRoute::new_dyn(
            Tool::new(
                "list_tables",
                "List all tables in a specific database. Requires database_name from list_databases.",
                schema_for::<ListTablesRequest>(),
            )
            .with_annotations(
                ToolAnnotations::new()
                    .read_only(true)
                    .destructive(false)
                    .idempotent(true)
                    .open_world(false),
            ),
            move |mut ctx: ToolCallContext<'_, Server>| {
                let params = Parameters::<ListTablesRequest>::from_context_part(&mut ctx);
                let b = b.clone();
                Box::pin(async move {
                    let params = params?;
                    let database_name = &params.0.database_name;
                    info!("TOOL: list_tables called. database_name={database_name}");
                    let table_list = b.list_tables(database_name).await.map_err(map_error)?;
                    info!("TOOL: list_tables completed. Tables found: {}", table_list.len());
                    let json = serde_json::to_string_pretty(&table_list).unwrap_or_else(|_| "[]".into());
                    Ok(CallToolResult::success(vec![Content::text(json)]))
                })
            },
        ));

        // get_table_schema
        let b = self.clone();
        router.add_route(ToolRoute::new_dyn(
            Tool::new(
                "get_table_schema",
                "Get column definitions (type, nullable, key, default) and foreign key relationships for a table. Requires database_name and table_name.",
                schema_for::<GetTableSchemaRequest>(),
            )
            .with_annotations(ToolAnnotations::new().read_only(true).destructive(false).idempotent(true).open_world(false)),
            move |mut ctx: ToolCallContext<'_, Server>| {
                let params = Parameters::<GetTableSchemaRequest>::from_context_part(&mut ctx);
                let b = b.clone();
                Box::pin(async move {
                    let params = params?;
                    let database_name = &params.0.database_name;
                    let table_name = &params.0.table_name;
                    info!("TOOL: get_table_schema called. database_name={database_name}, table_name={table_name}");
                    let schema = b.get_table_schema(database_name, table_name).await.map_err(map_error)?;
                    info!("TOOL: get_table_schema completed");
                    let json = serde_json::to_string_pretty(&schema).unwrap_or_else(|_| "{}".into());
                    Ok(CallToolResult::success(vec![Content::text(json)]))
                })
            },
        ));

        // read_query
        let b = self.clone();
        router.add_route(ToolRoute::new_dyn(
            Tool::new(
                "read_query",
                "Execute a read-only SQL query (SELECT, SHOW, DESCRIBE, USE, EXPLAIN).",
                schema_for::<QueryRequest>(),
            )
            .with_annotations(
                ToolAnnotations::new()
                    .read_only(true)
                    .destructive(false)
                    .idempotent(true)
                    .open_world(true),
            ),
            move |mut ctx: ToolCallContext<'_, Server>| {
                let params = Parameters::<QueryRequest>::from_context_part(&mut ctx);
                let b = b.clone();
                Box::pin(async move {
                    let params = params?;
                    let sql_query = &params.0.sql_query;
                    let database_name = &params.0.database_name;
                    info!(
                        "TOOL: execute_sql called. database_name={database_name}, sql_query={}",
                        &sql_query[..sql_query.len().min(100)]
                    );

                    {
                        let dialect = sqlparser::dialect::SQLiteDialect {};
                        backend::validation::validate_read_only_with_dialect(sql_query, &dialect).map_err(map_error)?;
                    }

                    let db = if database_name.is_empty() {
                        None
                    } else {
                        Some(database_name.as_str())
                    };
                    let results = b.execute_query(sql_query, db).await.map_err(map_error)?;
                    let row_count = results.as_array().map_or(0, Vec::len);
                    info!("TOOL: execute_sql completed. Rows returned: {row_count}");
                    let json = serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".into());
                    Ok(CallToolResult::success(vec![Content::text(json)]))
                })
            },
        ));

        // Write tools — only if not read-only
        if !self.read_only {
            // write_query
            let b = self.clone();
            router.add_route(ToolRoute::new_dyn(
                Tool::new(
                    "write_query",
                    "Execute a write SQL query (INSERT, UPDATE, DELETE, CREATE, ALTER, DROP).",
                    schema_for::<QueryRequest>(),
                )
                .with_annotations(
                    ToolAnnotations::new()
                        .read_only(false)
                        .destructive(true)
                        .idempotent(false)
                        .open_world(true),
                ),
                move |mut ctx: ToolCallContext<'_, Server>| {
                    let params = Parameters::<QueryRequest>::from_context_part(&mut ctx);
                    let b = b.clone();
                    Box::pin(async move {
                        let params = params?;
                        let sql_query = &params.0.sql_query;
                        let database_name = &params.0.database_name;
                        info!(
                            "TOOL: execute_sql called. database_name={database_name}, sql_query={}",
                            &sql_query[..sql_query.len().min(100)]
                        );

                        let db = if database_name.is_empty() {
                            None
                        } else {
                            Some(database_name.as_str())
                        };
                        let results = b.execute_query(sql_query, db).await.map_err(map_error)?;
                        let row_count = results.as_array().map_or(0, Vec::len);
                        info!("TOOL: execute_sql completed. Rows returned: {row_count}");
                        let json = serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".into());
                        Ok(CallToolResult::success(vec![Content::text(json)]))
                    })
                },
            ));

            // No create_database — SQLite doesn't support it
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::DatabaseBackend;

    #[test]
    fn quote_identifier_wraps_in_double_quotes() {
        assert_eq!(SqliteBackend::quote_identifier("users"), "\"users\"");
        assert_eq!(SqliteBackend::quote_identifier("eu-docker"), "\"eu-docker\"");
    }

    #[test]
    fn quote_identifier_escapes_double_quotes() {
        assert_eq!(SqliteBackend::quote_identifier("test\"db"), "\"test\"\"db\"");
        assert_eq!(SqliteBackend::quote_identifier("a\"b\"c"), "\"a\"\"b\"\"c\"");
    }

    #[test]
    fn try_from_sets_filename() {
        let config = DatabaseConfig {
            backend: DatabaseBackend::Sqlite,
            name: Some("test.db".into()),
            ..DatabaseConfig::default()
        };
        let opts = connect_options(&config);

        assert_eq!(opts.get_filename().to_str().expect("valid path"), "test.db");
    }

    #[test]
    fn try_from_empty_name_defaults() {
        let config = DatabaseConfig {
            backend: DatabaseBackend::Sqlite,
            name: None,
            ..DatabaseConfig::default()
        };
        let opts = connect_options(&config);

        // Empty string filename — validated elsewhere by Config::validate()
        assert_eq!(opts.get_filename().to_str().expect("valid path"), "");
    }

    // Row-to-JSON conversion tests live in crates/sqlx_to_json.
    // These tests cover the array-level wrapping done by execute_query.

    /// Helper: creates an in-memory `SQLite` pool for unit tests.
    async fn mem_pool() -> SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory SQLite")
    }

    /// Helper: runs a query and converts all rows via [`RowExt::to_json`].
    async fn query_json(pool: &SqlitePool, sql: &str) -> Value {
        let rows: Vec<SqliteRow> = sqlx::query(sql).fetch_all(pool).await.expect("query failed");
        Value::Array(rows.iter().map(RowExt::to_json).collect())
    }

    #[tokio::test]
    async fn execute_query_empty_result() {
        let pool = mem_pool().await;
        sqlx::query("CREATE TABLE t (v INTEGER)").execute(&pool).await.unwrap();

        let rows = query_json(&pool, "SELECT v FROM t").await;
        assert_eq!(rows, Value::Array(vec![]));
    }

    #[tokio::test]
    async fn execute_query_multiple_rows() {
        let pool = mem_pool().await;
        sqlx::query("CREATE TABLE t (id INTEGER, name TEXT, score REAL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO t VALUES (1, 'alice', 9.5), (2, 'bob', 8.0)")
            .execute(&pool)
            .await
            .unwrap();

        let rows = query_json(&pool, "SELECT id, name, score FROM t ORDER BY id").await;
        assert_eq!(rows.as_array().expect("should be array").len(), 2);

        assert_eq!(rows[0]["id"], Value::Number(1.into()));
        assert_eq!(rows[0]["name"], Value::String("alice".into()));
        assert!(rows[0]["score"].is_number());

        assert_eq!(rows[1]["id"], Value::Number(2.into()));
        assert_eq!(rows[1]["name"], Value::String("bob".into()));
    }
}
