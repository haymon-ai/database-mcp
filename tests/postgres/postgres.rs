//! Functional integration tests for PostgreSQL.
//!
//! Uses `#[sqlx::test]` for per-test database isolation. Each test gets a fresh
//! temporary database with schema and seed data from `migrations/0_setup.sql`.
//!
//! ```bash
//! ./tests/run.sh --filter postgres
//! ```

use sql_mcp::db::backend::Backend;
use sql_mcp::db::postgres::PostgresBackend;
use sqlx::PgPool;

const MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("tests/postgres/migrations");

fn backend(pool: PgPool, read_only: bool) -> Backend {
    Backend::Postgres(PostgresBackend::from_pool(pool, read_only))
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn it_lists_databases(pool: PgPool) -> anyhow::Result<()> {
    let result = backend(pool, false).tool_list_databases().await?;
    let dbs: Vec<String> = serde_json::from_str(&result)?;
    assert!(!dbs.is_empty(), "Expected at least one database");
    Ok(())
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn it_lists_tables(pool: PgPool) -> anyhow::Result<()> {
    let result = backend(pool, false).tool_list_tables("").await?;
    let tables: Vec<String> = serde_json::from_str(&result)?;
    for expected in ["users", "posts", "tags", "post_tags"] {
        assert!(
            tables.iter().any(|t| t == expected),
            "Missing '{expected}' in: {tables:?}"
        );
    }
    Ok(())
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn it_gets_table_schema(pool: PgPool) -> anyhow::Result<()> {
    let result = backend(pool, false)
        .tool_get_table_schema("", "users")
        .await?;
    let schema: serde_json::Value = serde_json::from_str(&result)?;
    let columns: Vec<String> = schema
        .as_object()
        .expect("object")
        .keys()
        .cloned()
        .collect();
    for col in ["id", "name", "email", "created_at"] {
        assert!(
            columns.iter().any(|c| c == col),
            "Missing '{col}' in: {columns:?}"
        );
    }
    Ok(())
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn it_gets_table_relations(pool: PgPool) -> anyhow::Result<()> {
    let result = backend(pool, false)
        .tool_get_table_schema_with_relations("", "posts")
        .await?;
    assert!(
        result.contains("user_id") || result.contains("users"),
        "Expected foreign key reference in: {result}"
    );
    Ok(())
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn it_executes_sql(pool: PgPool) -> anyhow::Result<()> {
    let result = backend(pool, false)
        .tool_execute_sql("SELECT * FROM users ORDER BY id", "", None)
        .await?;
    let rows: Vec<serde_json::Value> = serde_json::from_str(&result)?;
    assert_eq!(rows.len(), 3, "Expected 3 users, got {}", rows.len());
    Ok(())
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn it_blocks_writes_in_read_only_mode(pool: PgPool) {
    let result = backend(pool, true)
        .tool_execute_sql(
            "INSERT INTO users (name, email) VALUES ('Hacker', 'hack@evil.com')",
            "",
            None,
        )
        .await;
    assert!(
        result.is_err(),
        "Expected error for write in read-only mode"
    );
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn it_creates_database(pool: PgPool) -> anyhow::Result<()> {
    let b = backend(pool, false);
    let result = b.tool_create_database("mcp_new").await?;
    assert!(!result.is_empty());
    let list = b.tool_list_databases().await?;
    let dbs: Vec<String> = serde_json::from_str(&list)?;
    assert!(dbs.iter().any(|db| db == "mcp_new"), "New db not in list");
    Ok(())
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn it_has_consistent_seed_data(pool: PgPool) -> anyhow::Result<()> {
    let b = backend(pool, false);

    async fn check(b: &Backend, table: &str, expected: usize) {
        let sql = format!("SELECT CAST(COUNT(*) AS CHAR) as cnt FROM {table}");
        let result = b
            .tool_execute_sql(&sql, "", None)
            .await
            .unwrap_or_else(|e| panic!("count {table}: {e}"));
        let rows: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        let count_str = rows[0]
            .get("cnt")
            .and_then(|v| v.as_str())
            .or_else(|| {
                rows[0]
                    .as_object()
                    .and_then(|o| o.values().next())
                    .and_then(|v| v.as_str())
            })
            .unwrap_or_else(|| panic!("No count for {table}: {:?}", rows[0]));
        let count: usize = count_str.parse().unwrap();
        assert_eq!(count, expected, "{table}: expected {expected}, got {count}");
    }

    check(&b, "users", 3).await;
    check(&b, "posts", 5).await;
    check(&b, "tags", 4).await;
    check(&b, "post_tags", 6).await;
    Ok(())
}
