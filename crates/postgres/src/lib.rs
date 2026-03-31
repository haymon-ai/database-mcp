//! `PostgreSQL` backend crate.
//!
//! Provides [`PostgresBackend`] implementing the [`server::McpBackend`] trait.

pub mod postgres;

pub use postgres::PostgresBackend;
