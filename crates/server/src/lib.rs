//! Backend-agnostic MCP server and tool routing.
//!
//! Provides [`Server`] which implements the MCP `ServerHandler` trait,
//! and [`McpBackend`] which backends implement to register their tools.

pub mod handler;
pub mod server;
pub mod traits;

pub use server::Server;
pub use traits::McpBackend;
