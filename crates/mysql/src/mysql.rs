//! MySQL/MariaDB backend implementation via sqlx.
//!
//! Implements [`McpBackend`] for `MySQL` and `MariaDB` databases
//! using sqlx's `MySqlPool`. Each backend instance registers its
//! own MCP tools onto the server during startup.

use std::collections::HashMap;
use std::sync::Arc;

use backend::error::AppError;
use backend::identifier::validate_identifier;
use backend::types::{CreateDatabaseRequest, GetTableSchemaRequest, ListTablesRequest, QueryRequest};
use config::DatabaseConfig;
use rmcp::handler::server::common::{FromContextPart, schema_for_empty_input, schema_for_type};
use rmcp::handler::server::router::tool::{ToolRoute, ToolRouter};
use rmcp::handler::server::tool::ToolCallContext;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, Tool, ToolAnnotations};
use rmcp::schemars::JsonSchema;
use serde_json::{Map as JsonObject, Value, json};
use server::server::map_error;
use server::{McpBackend, Server};
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlRow, MySqlSslMode};
use sqlx::{Executor, MySqlPool, Row};
use sqlx_to_json::RowExt;
use tracing::{error, info};

/// Builds [`MySqlConnectOptions`] from a [`DatabaseConfig`].
fn connect_options(config: &DatabaseConfig) -> MySqlConnectOptions {
    let mut opts = MySqlConnectOptions::new()
        .host(&config.host)
        .port(config.port)
        .username(&config.user);

    if let Some(ref password) = config.password {
        opts = opts.password(password);
    }
    if let Some(ref name) = config.name
        && !name.is_empty()
    {
        opts = opts.database(name);
    }
    if let Some(ref charset) = config.charset {
        opts = opts.charset(charset);
    }

    if config.ssl {
        opts = if config.ssl_verify_cert {
            opts.ssl_mode(MySqlSslMode::VerifyCa)
        } else {
            opts.ssl_mode(MySqlSslMode::Required)
        };
        if let Some(ref ca) = config.ssl_ca {
            opts = opts.ssl_ca(ca);
        }
        if let Some(ref cert) = config.ssl_cert {
            opts = opts.ssl_client_cert(cert);
        }
        if let Some(ref key) = config.ssl_key {
            opts = opts.ssl_client_key(key);
        }
    }

    opts
}

/// MySQL/MariaDB database backend.
#[derive(Clone)]
pub struct MysqlBackend {
    pool: MySqlPool,
    pub read_only: bool,
}

impl std::fmt::Debug for MysqlBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MysqlBackend")
            .field("read_only", &self.read_only)
            .finish_non_exhaustive()
    }
}

impl MysqlBackend {
    /// Creates a new `MySQL` backend from configuration.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Connection`] if the connection fails.
    pub async fn new(config: &DatabaseConfig) -> Result<Self, AppError> {
        let pool = MySqlPoolOptions::new()
            .max_connections(config.max_pool_size)
            .connect_with(connect_options(config))
            .await
            .map_err(|e| AppError::Connection(format!("Failed to connect to MySQL: {e}")))?;

        info!("MySQL connection pool initialized (max size: {})", config.max_pool_size);

        let backend = Self {
            pool,
            read_only: config.read_only,
        };

        if config.read_only {
            backend.warn_if_file_privilege().await;
        }

        Ok(backend)
    }

    async fn warn_if_file_privilege(&self) {
        let result: Result<(), AppError> = async {
            let current_user: Option<String> = sqlx::query_scalar("SELECT CURRENT_USER()")
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| AppError::Query(e.to_string()))?;

            let Some(current_user) = current_user else {
                return Ok(());
            };

            let quoted_user = if let Some((user, host)) = current_user.split_once('@') {
                format!("'{user}'@'{host}'")
            } else {
                format!("'{current_user}'")
            };

            let grants: Vec<String> = sqlx::query_scalar(&format!("SHOW GRANTS FOR {quoted_user}"))
                .fetch_all(&self.pool)
                .await
                .map_err(|e| AppError::Query(e.to_string()))?;

            let has_file_priv = grants.iter().any(|grant| {
                let upper = grant.to_uppercase();
                upper.contains("FILE") && upper.contains("ON *.*")
            });

            if has_file_priv {
                error!(
                    "Connected database user has the global FILE privilege. \
                     Revoke FILE for the database user you are connecting as."
                );
            }

            Ok(())
        }
        .await;

        if let Err(e) = result {
            tracing::debug!("Unable to determine whether FILE privilege is enabled: {e}");
        }
    }

    /// Wraps `name` in backticks for safe use in `MySQL` SQL statements.
    ///
    /// Escapes internal backticks by doubling them.
    fn quote_identifier(name: &str) -> String {
        let escaped = name.replace('`', "``");
        format!("`{escaped}`")
    }

    /// Wraps a value in single quotes for use as a SQL string literal.
    ///
    /// Escapes internal single quotes by doubling them.
    fn quote_string(value: &str) -> String {
        let escaped = value.replace('\'', "''");
        format!("'{escaped}'")
    }

    /// Executes raw SQL and converts rows to JSON maps.
    ///
    /// Uses the text protocol via `Executor::fetch_all(&str)` instead of prepared
    /// statements, because `MySQL` 9+ doesn't support SHOW commands as prepared
    /// statements, and the text protocol returns all values as strings.
    async fn query_to_json(&self, sql: &str, database: Option<&str>) -> Result<Value, AppError> {
        // Acquire a single connection so USE and the query run on the same session
        let mut conn = self
            .pool
            .acquire()
            .await
            .map_err(|e| AppError::Connection(e.to_string()))?;

        // Switch database if needed
        if let Some(db) = database {
            validate_identifier(db)?;
            let use_sql = format!("USE {}", Self::quote_identifier(db));
            conn.execute(use_sql.as_str())
                .await
                .map_err(|e| AppError::Query(e.to_string()))?;
        }

        let rows: Vec<MySqlRow> = conn.fetch_all(sql).await.map_err(|e| AppError::Query(e.to_string()))?;
        Ok(Value::Array(rows.iter().map(RowExt::to_json).collect()))
    }
}

/// Returns the JSON Schema for `Parameters<T>`.
fn schema_for<T: JsonSchema + 'static>() -> Arc<JsonObject<String, serde_json::Value>> {
    schema_for_type::<Parameters<T>>()
}

impl MysqlBackend {
    /// Lists all accessible databases.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if the query fails.
    pub async fn list_databases(&self) -> Result<Vec<String>, AppError> {
        let results = self
            .query_to_json(
                "SELECT SCHEMA_NAME AS name FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME",
                None,
            )
            .await?;
        let rows = results.as_array().map_or([].as_slice(), Vec::as_slice);
        Ok(rows
            .iter()
            .filter_map(|row| row.get("name").and_then(|v| v.as_str().map(String::from)))
            .collect())
    }

    /// Lists all tables in a database.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if the identifier is invalid or the query fails.
    pub async fn list_tables(&self, database: &str) -> Result<Vec<String>, AppError> {
        validate_identifier(database)?;
        let sql = format!(
            "SELECT TABLE_NAME AS name FROM information_schema.TABLES WHERE TABLE_SCHEMA = {} ORDER BY TABLE_NAME",
            Self::quote_string(database)
        );
        let results = self.query_to_json(&sql, None).await?;
        let rows = results.as_array().map_or([].as_slice(), Vec::as_slice);
        Ok(rows
            .iter()
            .filter_map(|row| row.get("name").and_then(|v| v.as_str().map(String::from)))
            .collect())
    }

    /// Returns column definitions with foreign key relationships.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if validation fails or the query errors.
    pub async fn get_table_schema(&self, database: &str, table: &str) -> Result<Value, AppError> {
        validate_identifier(database)?;
        validate_identifier(table)?;

        // 1. Get basic schema
        let describe_sql = format!(
            "DESCRIBE {}.{}",
            Self::quote_identifier(database),
            Self::quote_identifier(table)
        );
        let schema_results = self.query_to_json(&describe_sql, None).await?;
        let schema_rows = schema_results.as_array().map_or([].as_slice(), Vec::as_slice);

        if schema_rows.is_empty() {
            return Err(AppError::TableNotFound(format!("{database}.{table}")));
        }

        let mut columns: HashMap<String, Value> = HashMap::new();
        for row in schema_rows {
            if let Some(col_name) = row.get("Field").and_then(|v| v.as_str()) {
                columns.insert(
                    col_name.to_string(),
                    json!({
                        "type": row.get("Type").unwrap_or(&Value::Null),
                        "nullable": row.get("Null").and_then(|v| v.as_str()).is_some_and(|s| s.to_uppercase() == "YES"),
                        "key": row.get("Key").unwrap_or(&Value::Null),
                        "default": row.get("Default").unwrap_or(&Value::Null),
                        "extra": row.get("Extra").unwrap_or(&Value::Null),
                        "foreign_key": null,
                    }),
                );
            }
        }

        // 2. Get FK relationships
        let fk_sql = r"
            SELECT
                kcu.COLUMN_NAME as column_name,
                kcu.CONSTRAINT_NAME as constraint_name,
                kcu.REFERENCED_TABLE_NAME as referenced_table,
                kcu.REFERENCED_COLUMN_NAME as referenced_column,
                rc.UPDATE_RULE as on_update,
                rc.DELETE_RULE as on_delete
            FROM information_schema.KEY_COLUMN_USAGE kcu
            INNER JOIN information_schema.REFERENTIAL_CONSTRAINTS rc
                ON kcu.CONSTRAINT_NAME = rc.CONSTRAINT_NAME
                AND kcu.CONSTRAINT_SCHEMA = rc.CONSTRAINT_SCHEMA
            WHERE kcu.TABLE_SCHEMA = ?
              AND kcu.TABLE_NAME = ?
              AND kcu.REFERENCED_TABLE_NAME IS NOT NULL
            ORDER BY kcu.CONSTRAINT_NAME, kcu.ORDINAL_POSITION
        ";

        let fk_rows: Vec<MySqlRow> = sqlx::query(fk_sql)
            .bind(database)
            .bind(table)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::Query(e.to_string()))?;

        for fk_row in &fk_rows {
            let col_name: Option<String> = fk_row.try_get("column_name").ok();
            if let Some(col_name) = col_name
                && let Some(col_info) = columns.get_mut(&col_name)
                && let Some(obj) = col_info.as_object_mut()
            {
                let constraint_name: Option<String> = fk_row.try_get("constraint_name").ok();
                let referenced_table: Option<String> = fk_row.try_get("referenced_table").ok();
                let referenced_column: Option<String> = fk_row.try_get("referenced_column").ok();
                let on_update: Option<String> = fk_row.try_get("on_update").ok();
                let on_delete: Option<String> = fk_row.try_get("on_delete").ok();
                obj.insert(
                    "foreign_key".to_string(),
                    json!({
                        "constraint_name": constraint_name,
                        "referenced_table": referenced_table,
                        "referenced_column": referenced_column,
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
    pub async fn execute_query(&self, sql: &str, database: Option<&str>) -> Result<Value, AppError> {
        self.query_to_json(sql, database).await
    }

    /// Creates a database if it doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if read-only or the query fails.
    pub async fn create_database(&self, name: &str) -> Result<Value, AppError> {
        if self.read_only {
            return Err(AppError::ReadOnlyViolation);
        }
        validate_identifier(name)?;

        // Check existence — use Vec<u8> because MySQL 9 returns BINARY columns
        let exists: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT SCHEMA_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME = ?")
                .bind(name)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| AppError::Query(e.to_string()))?;

        if exists.is_some() {
            return Ok(json!({
                "status": "exists",
                "message": format!("Database '{name}' already exists."),
                "database_name": name,
            }));
        }

        sqlx::query(&format!(
            "CREATE DATABASE IF NOT EXISTS {}",
            Self::quote_identifier(name)
        ))
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Query(e.to_string()))?;

        Ok(json!({
            "status": "success",
            "message": format!("Database '{name}' created successfully."),
            "database_name": name,
        }))
    }
}

impl McpBackend for MysqlBackend {
    #[allow(clippy::too_many_lines)]
    fn register_tools(&self, router: &mut ToolRouter<Server>) {
        // list_databases — always (MySQL supports multi-db)
        let b = self.clone();
        router.add_route(ToolRoute::new_dyn(
            Tool::new(
                "list_databases",
                "List all accessible databases on the connected database server. Call this first to discover available database names.",
                schema_for_empty_input(),
            )
            .with_annotations(ToolAnnotations::new().read_only(true).destructive(false).idempotent(true).open_world(false)),
            move |_ctx: ToolCallContext<'_, Server>| {
                let b = b.clone();
                Box::pin(async move {
                    info!("TOOL: list_databases called");
                    let db_list = b.list_databases().await.map_err(map_error)?;
                    info!("TOOL: list_databases completed. Databases found: {}", db_list.len());
                    let json = serde_json::to_string_pretty(&db_list).unwrap_or_else(|_| "[]".into());
                    Ok(CallToolResult::success(vec![Content::text(json)]))
                })
            },
        ));

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
                    let table_list = match b.list_tables(database_name).await {
                        Ok(t) => t,
                        Err(e) => {
                            error!("TOOL ERROR: list_tables failed for database_name={database_name}: {e}");
                            return Err(map_error(e));
                        }
                    };
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

                    // Validate read-only with MySQL dialect
                    {
                        let dialect = sqlparser::dialect::MySqlDialect {};
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

            // create_database
            let b = self.clone();
            router.add_route(ToolRoute::new_dyn(
                Tool::new(
                    "create_database",
                    "Create a new database. Not supported for SQLite.",
                    schema_for::<CreateDatabaseRequest>(),
                )
                .with_annotations(
                    ToolAnnotations::new()
                        .read_only(false)
                        .destructive(false)
                        .idempotent(false)
                        .open_world(false),
                ),
                move |mut ctx: ToolCallContext<'_, Server>| {
                    let params = Parameters::<CreateDatabaseRequest>::from_context_part(&mut ctx);
                    let b = b.clone();
                    Box::pin(async move {
                        let params = params?;
                        let database_name = &params.0.database_name;
                        info!("TOOL: create_database called for database: '{database_name}'");
                        let result = b.create_database(database_name).await.map_err(map_error)?;
                        info!("TOOL: create_database completed");
                        let json = serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".into());
                        Ok(CallToolResult::success(vec![Content::text(json)]))
                    })
                },
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::DatabaseBackend;

    fn base_config() -> DatabaseConfig {
        DatabaseConfig {
            backend: DatabaseBackend::Mysql,
            host: "db.example.com".into(),
            port: 3307,
            user: "admin".into(),
            password: Some("s3cret".into()),
            name: Some("mydb".into()),
            ..DatabaseConfig::default()
        }
    }

    #[test]
    fn quote_identifier_wraps_in_backticks() {
        assert_eq!(MysqlBackend::quote_identifier("users"), "`users`");
        assert_eq!(MysqlBackend::quote_identifier("eu-docker"), "`eu-docker`");
    }

    #[test]
    fn quote_identifier_escapes_backticks() {
        assert_eq!(MysqlBackend::quote_identifier("test`db"), "`test``db`");
        assert_eq!(MysqlBackend::quote_identifier("a`b`c"), "`a``b``c`");
    }

    #[test]
    fn try_from_basic_config() {
        let config = base_config();
        let opts = connect_options(&config);

        assert_eq!(opts.get_host(), "db.example.com");
        assert_eq!(opts.get_port(), 3307);
        assert_eq!(opts.get_username(), "admin");
        assert_eq!(opts.get_database(), Some("mydb"));
    }

    #[test]
    fn try_from_with_charset() {
        let config = DatabaseConfig {
            charset: Some("utf8mb4".into()),
            ..base_config()
        };
        let opts = connect_options(&config);

        assert_eq!(opts.get_charset(), "utf8mb4");
    }

    #[test]
    fn try_from_with_ssl_required() {
        let config = DatabaseConfig {
            ssl: true,
            ssl_verify_cert: false,
            ..base_config()
        };
        let opts = connect_options(&config);

        assert!(
            matches!(opts.get_ssl_mode(), MySqlSslMode::Required),
            "expected Required, got {:?}",
            opts.get_ssl_mode()
        );
    }

    #[test]
    fn try_from_with_ssl_verify_ca() {
        let config = DatabaseConfig {
            ssl: true,
            ssl_verify_cert: true,
            ..base_config()
        };
        let opts = connect_options(&config);

        assert!(
            matches!(opts.get_ssl_mode(), MySqlSslMode::VerifyCa),
            "expected VerifyCa, got {:?}",
            opts.get_ssl_mode()
        );
    }

    #[test]
    fn try_from_without_password() {
        let config = DatabaseConfig {
            password: None,
            ..base_config()
        };
        let opts = connect_options(&config);

        // Should not panic — password is simply omitted
        assert_eq!(opts.get_host(), "db.example.com");
    }

    #[test]
    fn try_from_without_database_name() {
        let config = DatabaseConfig {
            name: None,
            ..base_config()
        };
        let opts = connect_options(&config);

        assert_eq!(opts.get_database(), None);
    }
}
