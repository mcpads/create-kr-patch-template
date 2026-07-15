//! MCP stdio server exposing the platform-neutral `patch-guard` judgments as
//! tools. See `tools` for the tool surface and `mcp` for the transport.

pub mod mcp;
pub mod tools;

pub use mcp::{handle_line, serve_stdio};
pub use tools::{Dispatch, Judgment, ToolDef, dispatch, tool_defs};
