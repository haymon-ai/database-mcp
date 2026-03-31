//! MCP server struct and constructor.
//!
//! Defines [`Server`] which holds the tool router populated by backends.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{ErrorData, Tool};

use crate::traits::McpBackend;

/// Converts a displayable error into an MCP [`ErrorData`].
pub fn map_error(e: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(e.to_string(), None)
}

/// Backend-agnostic MCP server that hosts registered tools.
#[derive(Clone)]
pub struct Server {
    pub(crate) tool_router: ToolRouter<Self>,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server").finish_non_exhaustive()
    }
}

impl Server {
    /// Creates a new MCP server with an empty tool router.
    ///
    /// Call [`register`](Self::register) to populate tools before starting transport.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tool_router: ToolRouter::new(),
        }
    }

    /// Registers a backend's tools onto this server.
    ///
    /// The backend populates the tool router with its tool definitions
    /// and handler closures.
    pub fn register(&mut self, backend: &impl McpBackend) {
        backend.register_tools(&mut self.tool_router);
    }

    /// Looks up a tool by name in the router.
    #[must_use]
    pub fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tool_router.get(name).cloned()
    }
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}
