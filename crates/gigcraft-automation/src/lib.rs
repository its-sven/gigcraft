//! Automation for GigCraft: command lines (`id` or `id={json}`) and an MCP
//! server over stdio, both running the commands of `gigcraft-commands`.
//!
//! The MCP layer uses `craft-jsonrpc` and `craft-mcp` from the Craft libraries.
//! Tool names are the command ids with `.` replaced by `_` (`song.create` →
//! `song_create`), because MCP clients restrict tool names to `[A-Za-z0-9_-]`.
//! The server only touches the one library it was started with.

use craft_jsonrpc::{Limits, Request, RpcError};
use craft_mcp::{Policy, Session};
use gigcraft_commands::{CommandError, Engine, Outcome, specs};
use serde_json::{Map, Value, json};
use std::cell::RefCell;
use std::io::{BufRead, Write};

/// Largest JSON message accepted on stdin (8 MiB).
pub const MAX_MESSAGE_BYTES: usize = 8 << 20;

/// Parse `song.list` or `song.create={"title":"x"}`.
pub fn parse_cmd(arg: &str) -> Result<(String, Value), String> {
    let (id, json) = match arg.split_once('=') {
        Some((id, json)) => (id.trim(), Some(json)),
        None => (arg.trim(), None),
    };
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
    {
        return Err(format!("`{id}` is not a command id"));
    }
    let params = match json {
        None => Value::Null,
        Some(text) => {
            serde_json::from_str(text).map_err(|e| format!("parameters of `{id}`: {e}"))?
        }
    };
    Ok((id.to_string(), params))
}

/// Run one command, answering `commands.list` here.
pub fn run(engine: &mut Engine, id: &str, params: &Value) -> Result<Outcome, CommandError> {
    if id == "commands.list" {
        return Ok(Outcome {
            result: specs_json(),
            changes: Vec::new(),
        });
    }
    engine.run(id, params)
}

/// All command specs as JSON.
#[must_use]
pub fn specs_json() -> Value {
    serde_json::to_value(specs()).unwrap_or(Value::Null)
}

/// Command id → MCP tool name.
#[must_use]
pub fn tool_name(command: &str) -> String {
    command.replace('.', "_")
}

/// MCP tool name → command id.
#[must_use]
pub fn command_id(tool: &str) -> Option<&'static str> {
    specs()
        .iter()
        .map(|s| s.id)
        .find(|id| tool_name(id) == tool)
}

/// The MCP tool list with JSON Schemas built from the command specs.
#[must_use]
pub fn mcp_tools() -> Value {
    let tools: Vec<Value> = specs()
        .iter()
        .map(|spec| {
            let mut props = Map::new();
            let mut required = Vec::new();
            for p in spec.params {
                props.insert(
                    p.name.into(),
                    json!({ "type": p.kind, "description": p.doc }),
                );
                if p.required {
                    required.push(Value::from(p.name));
                }
            }
            json!({
                "name": tool_name(spec.id),
                "description": spec.summary,
                "inputSchema": { "type": "object", "properties": props, "required": required },
                "annotations": { "readOnlyHint": !spec.mutates },
            })
        })
        .collect();
    craft_mcp::tool_list(Value::Array(tools))
}

const INSTRUCTIONS: &str = "GigCraft library for gigging musicians. Songs have ChordPro charts, \
parts with MIDI actions and setlists with sets and breaks. Call library_info first, then \
song_list / setlist_list. Changes can be reverted with edit_undo. Deleting moves records to the \
library's trash.";

/// One MCP connection: protocol state plus the engine it drives.
#[derive(Debug)]
pub struct McpServer<'a> {
    session: RefCell<SessionBox>,
    engine: RefCell<&'a mut Engine>,
    limits: Limits,
}

/// `craft_mcp::Session` has no `Debug`; wrap it.
struct SessionBox(Session);

impl std::fmt::Debug for SessionBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("state", &self.0.state())
            .finish()
    }
}

impl<'a> McpServer<'a> {
    #[must_use]
    pub fn new(engine: &'a mut Engine) -> Self {
        let mut session = Session::new("gigcraft", env!("CARGO_PKG_VERSION"), Policy::Compatible);
        session.set_instructions(INSTRUCTIONS.to_string());
        let limits = Limits {
            max_message_bytes: MAX_MESSAGE_BYTES,
            max_batch_items: 256,
            integer_ids_only: true,
            allow_null_ids: false,
        };
        Self {
            session: RefCell::new(SessionBox(session)),
            engine: RefCell::new(engine),
            limits,
        }
    }

    /// Handle one raw message; returns the response to write, if any.
    pub fn handle(&self, bytes: &[u8]) -> Option<Value> {
        let value = match craft_jsonrpc::parse_json(bytes, self.limits) {
            Ok(v) => v,
            Err(_) => return Some(craft_jsonrpc::parse_error_response()),
        };
        let in_batch = value.is_array();
        let result = craft_jsonrpc::process(
            &value,
            self.limits,
            |req| self.request(req, in_batch),
            |req| self.session.borrow_mut().0.notification(req),
        );
        match result {
            Ok(resp) => resp,
            Err(_) => Some(craft_jsonrpc::error_response(
                Value::Null,
                RpcError::new(-32600, "Invalid Request: message or batch too large"),
            )),
        }
    }

    fn request(&self, req: Request<'_>, in_batch: bool) -> Result<Value, RpcError> {
        if let Some(answer) = self.session.borrow_mut().0.request(req, in_batch) {
            return answer;
        }
        match req.method {
            "tools/list" => Ok(mcp_tools()),
            "tools/call" => {
                let params = req.params.cloned().unwrap_or(Value::Null);
                let (tool, args) = craft_mcp::tool_call(&params)?;
                let Some(id) = command_id(tool) else {
                    return Err(RpcError::new(-32602, format!("Unknown tool: {tool}")));
                };
                let mut engine = self.engine.borrow_mut();
                Ok(match run(&mut engine, id, &args) {
                    Ok(out) => {
                        let text = serde_json::to_string_pretty(&out).unwrap_or_default();
                        craft_mcp::text_tool_result(&text, false)
                    }
                    Err(e) => {
                        let text =
                            json!({ "error": e.code(), "message": e.to_string() }).to_string();
                        craft_mcp::text_tool_result(&text, true)
                    }
                })
            }
            other => Err(RpcError::new(-32601, format!("Method not found: {other}"))),
        }
    }

    /// Serve newline-delimited JSON-RPC until the input ends.
    pub fn serve<R: BufRead, W: Write>(&self, mut input: R, mut output: W) -> std::io::Result<()> {
        loop {
            let line = match craft_jsonrpc::read_line(&mut input, MAX_MESSAGE_BYTES) {
                Ok(Some(line)) => line,
                Ok(None) => return Ok(()),
                Err(craft_jsonrpc::LineError::Io(e)) => return Err(e),
                Err(_) => {
                    // Over-limit line: answer once and stop, as the stream position is unknown.
                    let resp = craft_jsonrpc::error_response(
                        Value::Null,
                        RpcError::new(-32600, "Invalid Request: message too large"),
                    );
                    writeln!(output, "{resp}")?;
                    output.flush()?;
                    return Ok(());
                }
            };
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            if let Some(resp) = self.handle(&line) {
                writeln!(output, "{resp}")?;
                output.flush()?;
            }
        }
    }
}

#[cfg(test)]
mod tests;
