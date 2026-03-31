//! MySQL/MariaDB backend crate.
//!
//! Provides [`MysqlBackend`] implementing the [`server::McpBackend`] trait.

pub mod mysql;

pub use mysql::MysqlBackend;
