//! Shared MCP server utilities and request types.
//!
//! Provides [`types`] for tool request/response schemas,
//! [`pagination`] cursor helpers, the [`redact`] PII redaction
//! pipeline, and the [`Server`] wrapper plus [`server_info`] used by
//! per-backend server implementations.

pub mod pagination;
pub mod redact;
mod server;
pub mod types;

pub use pagination::{Cursor, Pager};
pub use redact::{RedactionError, RedactionStats, Redactor};
pub use server::{Server, server_info};
