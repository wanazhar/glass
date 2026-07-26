//! Model Context Protocol (MCP) server.
//!
//! JSON-RPC 2.0 stdio server implementing the MCP 2024-11-05 protocol.
//! Provides browser automation tools, prompts, and resources to AI clients
//! with policy-gated execution and bounded response sizes.

pub mod prompts;
pub mod resources;
pub mod server;
