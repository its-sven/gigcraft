use super::*;
use gigcraft_store::Library;

fn engine() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().expect("tempdir");
    let lib = Library::create(dir.path().join("lib"), "Test").expect("create");
    (dir, Engine::new(lib))
}

#[test]
fn parse_cmd_forms() {
    assert_eq!(
        parse_cmd("song.list"),
        Ok(("song.list".into(), Value::Null))
    );
    assert_eq!(
        parse_cmd(r#"song.create={"title":"a=b"}"#),
        Ok(("song.create".into(), json!({"title": "a=b"})))
    );
    assert!(parse_cmd("").is_err());
    assert!(parse_cmd("rm -rf").is_err());
    assert!(parse_cmd("song.create={").is_err());
}

#[test]
fn every_command_maps_to_a_tool_and_back() {
    let tools = mcp_tools();
    let list = tools["tools"].as_array().expect("tools");
    assert_eq!(list.len(), specs().len());
    for t in list {
        let name = t["name"].as_str().expect("name");
        assert!(
            name.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
            "{name}"
        );
        assert!(command_id(name).is_some(), "{name}");
    }
}

fn send(server: &McpServer<'_>, msg: Value) -> Value {
    server
        .handle(msg.to_string().as_bytes())
        .unwrap_or(Value::Null)
}

#[test]
fn mcp_session_lists_and_calls_tools() {
    let (_d, mut e) = engine();
    let server = McpServer::new(&mut e);
    let init = send(
        &server,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "gigcraft");
    assert!(
        server
            .handle(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .is_none()
    );

    let tools = send(
        &server,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    );
    assert!(
        tools["result"]["tools"]
            .as_array()
            .is_some_and(|t| !t.is_empty())
    );

    let created = send(
        &server,
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call",
        "params":{"name":"song_create","arguments":{"title":"From MCP","chart":"[C]Hi"}}}),
    );
    assert_eq!(created["result"]["isError"], false);
    let text = created["result"]["content"][0]["text"]
        .as_str()
        .expect("text");
    let out: Value = serde_json::from_str(text).expect("json");
    assert_eq!(out["changes"][0]["action"], "created");

    let bad = send(
        &server,
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call",
        "params":{"name":"song_get","arguments":{"id":"not-a-uuid"}}}),
    );
    assert_eq!(bad["result"]["isError"], true);

    let unknown = send(
        &server,
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call",
        "params":{"name":"rm_rf","arguments":{}}}),
    );
    assert_eq!(unknown["error"]["code"], -32602);

    assert_eq!(
        server
            .handle(b"not json")
            .map(|v| v["error"]["code"].clone()),
        Some(json!(-32700))
    );
}

#[test]
fn serve_reads_lines_until_eof() {
    let (_d, mut e) = engine();
    let server = McpServer::new(&mut e);
    let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"library_info\"}}\n";
    let mut out = Vec::new();
    server.serve(&input[..], &mut out).expect("serve");
    let lines: Vec<Value> = String::from_utf8_lossy(&out)
        .lines()
        .map(|l| serde_json::from_str(l).expect("json line"))
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1]["result"]["isError"], false);
}
