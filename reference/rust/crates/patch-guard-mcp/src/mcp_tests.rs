use serde_json::{Value, json};

use crate::mcp::handle_line;

fn call(request: Value) -> Value {
    let line = serde_json::to_string(&request).expect("encode request");
    handle_line(&line).expect("request expects a response")
}

#[test]
fn initialize_advertises_tools_capability() {
    let response = call(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2025-06-18", "capabilities": {} }
    }));
    assert_eq!(response["id"], json!(1));
    assert_eq!(response["result"]["protocolVersion"], "2025-06-18");
    assert!(response["result"]["capabilities"]["tools"].is_object());
    assert_eq!(response["result"]["serverInfo"]["name"], "patch-guard-mcp");
}

#[test]
fn initialized_notification_has_no_response() {
    let line = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string();
    assert!(handle_line(&line).is_none());
}

#[test]
fn tools_list_returns_every_tool() {
    let response = call(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    let tools = response["result"]["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    for expected in [
        "verify_source",
        "verify_exact_roundtrip",
        "evaluate_readiness",
        "validate_product_graph",
        "require_runtime_pass",
        "apply_write_plan",
    ] {
        assert!(names.contains(&expected), "missing tool {expected}");
    }
    for tool in tools {
        assert_eq!(tool["inputSchema"]["type"], "object");
    }
}

#[test]
fn tools_call_returns_structured_decision() {
    let response = call(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "verify_exact_roundtrip",
            "arguments": { "boundary_id": "header", "original": [1, 2, 3], "rebuilt": [1, 2, 9] }
        }
    }));
    assert_eq!(response["result"]["isError"], json!(false));
    assert_eq!(
        response["result"]["structuredContent"]["decision"],
        "reject"
    );
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");
    assert!(text.contains("reject"));
}

#[test]
fn unknown_tool_reports_invalid_params() {
    let response = call(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": { "name": "no_such_tool", "arguments": {} }
    }));
    assert_eq!(response["error"]["code"], json!(-32602));
}

#[test]
fn unknown_method_reports_method_not_found() {
    let response = call(json!({ "jsonrpc": "2.0", "id": 5, "method": "frobnicate" }));
    assert_eq!(response["error"]["code"], json!(-32601));
}

#[test]
fn malformed_json_reports_parse_error() {
    let response = handle_line("{not json").expect("parse error still responds");
    assert_eq!(response["error"]["code"], json!(-32700));
}
