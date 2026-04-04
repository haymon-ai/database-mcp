//! Generic MCP server wrapper and shared utilities.
//!
//! [`Server`] provides a single [`ServerHandler`] implementation
//! that works with any database backend. The [`Backend`] trait
//! defines the tool registration contract each backend must fulfill.
//! Also provides helper functions used by per-backend implementations.

use rmcp::ServerHandler;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::model::{ErrorData, Implementation, ServerCapabilities, ServerInfo};
use tracing::info;

/// Contract for backend types that supply MCP tools.
///
/// Each database backend implements this trait to register its
/// available tools. Construction is handled by each backend's own
/// `new()` method, not by this trait.
pub trait Backend: Clone + std::fmt::Debug + Send + Sync + 'static {
    /// Provides the tool router with the appropriate tools for this backend.
    ///
    /// Write tools are excluded when the backend is in read-only mode.
    fn provide_tool_router(&self) -> ToolRouter<Server<Self>>;
}

/// Generic MCP server wrapping a backend.
///
/// Provides a single [`ServerHandler`] implementation shared by all
/// backends. Use [`Server::new`] to construct a server from an
/// already-connected backend.
#[derive(Clone, Debug)]
pub struct Server<T> {
    /// The database backend.
    pub backend: T,
    tool_router: ToolRouter<Self>,
}

impl<T: Backend> Server<T> {
    /// Creates a new server from a connected backend.
    ///
    /// Calls [`Backend::provide_tool_router`] to register the
    /// backend's tools.
    pub fn new(backend: T) -> Self {
        let tool_router = backend.provide_tool_router();
        Self { backend, tool_router }
    }
}

impl<T: Backend> ServerHandler for Server<T> {
    fn get_info(&self) -> ServerInfo {
        server_info()
    }

    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        let tool_name = request.name.clone();
        info!("TOOL: {tool_name} called. arguments={:?}", request.arguments);

        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        let result = self.tool_router.call(tcc).await;

        match &result {
            Ok(r) => {
                let byte_len: usize = r.content.iter().map(|c| format!("{c:?}").len()).sum();
                info!("TOOL: {tool_name} completed. response_bytes={byte_len}");
            }
            Err(e) => {
                info!("TOOL: {tool_name} failed. error={}", e.message);
            }
        }

        result
    }

    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, rmcp::ErrorData> {
        Ok(rmcp::model::ListToolsResult {
            tools: self.tool_router.list_all(),
            meta: None,
            next_cursor: None,
        })
    }

    fn get_tool(&self, name: &str) -> Option<rmcp::model::Tool> {
        self.tool_router.get(name).cloned()
    }
}

/// Converts a displayable error into an MCP [`ErrorData`].
pub fn map_error(e: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(e.to_string(), None)
}

/// Returns the shared [`ServerInfo`] for all server implementations.
///
/// Provides consistent server identity, capabilities, and instructions
/// across all database backends.
#[must_use]
pub fn server_info() -> ServerInfo {
    ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
        .with_server_info(Implementation::new(
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(
            "Database MCP Server - provides database exploration and query tools for MySQL, MariaDB, PostgreSQL, and SQLite",
        )
}
