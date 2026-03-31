//! Backend registration trait for MCP tool providers.
//!
//! Defines [`McpBackend`], the minimal interface that database backends
//! implement to register their tools onto a [`Server`].

use rmcp::handler::server::router::tool::ToolRouter;

use crate::server::Server;

/// Trait for MCP-capable backends that register their own tools.
///
/// Implementors capture their own state (connection pools, config) into
/// tool handler closures during registration. The server never accesses
/// backend internals directly.
pub trait McpBackend: Send + Sync {
    /// Register this backend's tools onto the given tool router.
    fn register_tools(&self, router: &mut ToolRouter<Server>);
}
