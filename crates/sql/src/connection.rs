//! Connection abstraction shared across database backends.
//!
//! Defines [`Connection`] — the single entry point every backend tool
//! handler uses to run SQL, and [`PoolProvider`] — the backend-specific
//! pool resolution trait that powers the blanket [`Connection`] impl.

use database_mcp_server::AppError;
use serde_json::Value;
use sqlx::Executor;
use sqlx_to_json::QueryResult as _;

use crate::timeout::execute_with_timeout;

/// Unified query surface every backend tool handler uses.
///
/// Three methods cover all SQL operations: [`execute`](Connection::execute),
/// [`fetch`](Connection::fetch), and [`fetch_optional`](Connection::fetch_optional).
///
/// Backends should implement [`PoolProvider`] instead of this trait
/// directly; a blanket impl covers the shared query logic.
///
/// # Errors
///
/// All methods may return:
///
/// - [`AppError::InvalidIdentifier`] — `database` failed identifier validation.
/// - [`AppError::Connection`] — the underlying driver failed.
/// - [`AppError::QueryTimeout`] — the query exceeded the configured timeout.
pub trait Connection: Send + Sync {
    /// Runs a statement that returns no meaningful rows.
    ///
    /// # Errors
    ///
    /// See trait-level documentation.
    fn execute(&self, query: &str, database: Option<&str>) -> impl Future<Output = Result<u64, AppError>> + Send;

    /// Runs a statement and collects every result row as JSON.
    ///
    /// # Errors
    ///
    /// See trait-level documentation.
    fn fetch(&self, query: &str, database: Option<&str>) -> impl Future<Output = Result<Vec<Value>, AppError>> + Send;

    /// Runs a statement and returns at most one result row as JSON.
    ///
    /// # Errors
    ///
    /// See trait-level documentation.
    fn fetch_optional(
        &self,
        query: &str,
        database: Option<&str>,
    ) -> impl Future<Output = Result<Option<Value>, AppError>> + Send;
}

/// Backend-specific pool resolution.
///
/// Each database backend implements this trait to provide its own pool
/// lookup strategy (e.g. per-database caching for MySQL/PostgreSQL,
/// single pool for `SQLite`). The blanket [`Connection`] impl uses these
/// methods to run queries against the resolved pool.
///
/// # Errors
///
/// [`pool`](PoolProvider::pool) may return:
///
/// - [`AppError::InvalidIdentifier`] — the target database name failed
///   identifier validation.
pub trait PoolProvider: Send + Sync {
    /// The sqlx database driver type (e.g. `sqlx::MySql`).
    type DB: sqlx::Database;

    /// Resolves the connection pool for the given target database.
    ///
    /// # Errors
    ///
    /// - [`AppError::InvalidIdentifier`] — `target` failed validation.
    fn pool(&self, target: Option<&str>) -> impl Future<Output = Result<sqlx::Pool<Self::DB>, AppError>> + Send;

    /// Returns the configured query timeout in seconds, if any.
    fn query_timeout(&self) -> Option<u64>;
}

impl<T> Connection for T
where
    T: PoolProvider,
    for<'c> &'c mut <T::DB as sqlx::Database>::Connection: Executor<'c, Database = T::DB>,
    <T::DB as sqlx::Database>::Row: sqlx_to_json::RowExt,
    <T::DB as sqlx::Database>::QueryResult: sqlx_to_json::QueryResult,
{
    async fn execute(&self, query: &str, database: Option<&str>) -> Result<u64, AppError> {
        let pool = self.pool(database).await?;
        let sql = query.to_owned();
        execute_with_timeout(self.query_timeout(), query, async move {
            let mut conn = pool.acquire().await?;
            let result = (&mut *conn).execute(sql.as_str()).await?;
            Ok::<_, sqlx::Error>(result.rows_affected())
        })
        .await
    }

    async fn fetch(&self, query: &str, database: Option<&str>) -> Result<Vec<Value>, AppError> {
        let pool = self.pool(database).await?;
        let sql = query.to_owned();
        execute_with_timeout(self.query_timeout(), query, async move {
            let mut conn = pool.acquire().await?;
            let rows = (&mut *conn).fetch_all(sql.as_str()).await?;
            Ok::<_, sqlx::Error>(rows.iter().map(sqlx_to_json::RowExt::to_json).collect())
        })
        .await
    }

    async fn fetch_optional(&self, query: &str, database: Option<&str>) -> Result<Option<Value>, AppError> {
        let pool = self.pool(database).await?;
        let sql = query.to_owned();
        execute_with_timeout(self.query_timeout(), query, async move {
            let mut conn = pool.acquire().await?;
            let row = (&mut *conn).fetch_optional(sql.as_str()).await?;
            Ok::<_, sqlx::Error>(row.as_ref().map(sqlx_to_json::RowExt::to_json))
        })
        .await
    }
}
