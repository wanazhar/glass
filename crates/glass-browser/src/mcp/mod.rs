//! Model Context Protocol (MCP) server.
//!
//! JSON-RPC 2.0 stdio server implementing the MCP 2024-11-05 protocol.
//! Provides browser automation tools, prompts, and resources to MCP clients
//! with policy-gated execution and bounded response sizes.

pub(crate) mod agent_tools;
pub mod prompts;
pub mod resources;
pub(crate) mod secrets;
pub mod server;
