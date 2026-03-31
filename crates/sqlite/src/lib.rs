//! `SQLite` backend crate.
//!
//! Provides [`SqliteBackend`] implementing the [`server::McpBackend`] trait.

pub mod sqlite;

pub use sqlite::SqliteBackend;
