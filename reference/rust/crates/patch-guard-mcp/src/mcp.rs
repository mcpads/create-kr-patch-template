//! Minimal MCP server over newline-delimited JSON-RPC 2.0 on stdio.
//!
//! Only the methods a validation-tool server needs are implemented:
//! `initialize`, `ping`, `tools/list`, and `tools/call`, plus the
//! `notifications/initialized` acknowledgement. The transport is intentionally
//! dependency-free so it can be replaced alongside the reference core.

use std::io::{self, BufRead, Write};

use serde_json::{Value, json};

use crate::tools::{self, Dispatch};

/// Protocol revision advertised when the client does not request one.
const DEFAULT_PROTOCOL_VERSION: &str = "2025-06-18";
const SERVER_NAME: &str = "patch-guard-mcp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

// JSON-RPC 2.0 error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// Serve the MCP protocol on stdin/stdout until end of input.
pub fn serve_stdio() -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut line = String::new();
    loop {
        line.clear();
        if stdin.lock().read_line(&mut line)? == 0 {
            return Ok(());
        }
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle_line(&line) {
            let encoded = serde_json::to_string(&response)
                .unwrap_or_else(|_| r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"internal error"}}"#.to_owned());
            writeln!(stdout, "{encoded}")?;
            stdout.flush()?;
        }
    }
}

/// Handle one framed message. Returns `None` for notifications.
pub fn handle_line(line: &str) -> Option<Value> {
    let request: Value = match serde_json::from_str(line.trim()) {
        Ok(value) => value,
        Err(error) => {
            return Some(error_response(
                Value::Null,
                PARSE_ERROR,
                &format!("invalid JSON: {error}"),
            ));
        }
    };
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return Some(error_response(
            request.get("id").cloned().unwrap_or(Value::Null),
            INVALID_REQUEST,
            "missing method",
        ));
    };
    let id = request.get("id").cloned();
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    let is_notification = id.is_none();

    match method {
        "notifications/initialized" | "notifications/cancelled" => None,
        "initialize" => reply(id, Ok(initialize_result(&params))),
        "ping" => reply(id, Ok(json!({}))),
        "tools/list" => reply(id, Ok(tools_list_result())),
        "tools/call" => reply(id, tools_call_result(&params)),
        _ if is_notification => None,
        other => reply(
            id,
            Err(JsonRpcError {
                code: METHOD_NOT_FOUND,
                message: format!("unknown method `{other}`"),
            }),
        ),
    }
}

struct JsonRpcError {
    code: i64,
    message: String,
}

fn reply(id: Option<Value>, result: Result<Value, JsonRpcError>) -> Option<Value> {
    // Requests without an id are notifications and never receive a response.
    let id = id?;
    Some(match result {
        Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }),
        Err(error) => error_response(id, error.code, &error.message),
    })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

fn initialize_result(params: &Value) -> Value {
    let protocol_version = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_PROTOCOL_VERSION);
    json!({
        "protocolVersion": protocol_version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
        "instructions": "Language-neutral patch-guard judgments. Each tool returns {\"decision\":\"accept\"|\"reject\"}; a reject is a valid guard outcome, not a transport error.",
    })
}

fn tools_list_result() -> Value {
    let tools: Vec<Value> = tools::tool_defs()
        .into_iter()
        .map(|def| {
            json!({
                "name": def.name,
                "description": def.description,
                "inputSchema": def.input_schema,
            })
        })
        .collect();
    json!({ "tools": tools })
}

fn tools_call_result(params: &Value) -> Result<Value, JsonRpcError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcError {
            code: INVALID_PARAMS,
            message: "`name` must be a string".to_owned(),
        })?;
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    match tools::dispatch(name, &arguments) {
        Dispatch::Judged(judgment) => {
            let decision = if judgment.accept { "accept" } else { "reject" };
            let structured = json!({ "decision": decision, "report": judgment.report });
            let text = serde_json::to_string_pretty(&structured)
                .unwrap_or_else(|_| structured.to_string());
            Ok(json!({
                "content": [{ "type": "text", "text": text }],
                "structuredContent": structured,
                "isError": false,
            }))
        }
        Dispatch::InvalidParams(message) => Err(JsonRpcError {
            code: INVALID_PARAMS,
            message,
        }),
        Dispatch::UnknownTool => Err(JsonRpcError {
            code: INVALID_PARAMS,
            message: format!("unknown tool `{name}`"),
        }),
    }
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
