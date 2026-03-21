//! Reusable LSP test harness for integration testing acdc-lsp.
//!
//! Spawns the `acdc-lsp` binary and communicates over stdin/stdout
//! using JSON-RPC messages per the LSP specification.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};

/// Errors from the LSP test harness.
#[derive(Debug)]
pub enum HarnessError {
    Spawn(std::io::Error),
    Io(std::io::Error),
    Json(serde_json::Error),
    Lsp { method: String, error: Value },
    Protocol(String),
}

impl std::fmt::Display for HarnessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "failed to spawn acdc-lsp: {e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Json(e) => write!(f, "JSON error: {e}"),
            Self::Lsp { method, error } => write!(f, "LSP error for {method}: {error}"),
            Self::Protocol(msg) => write!(f, "protocol error: {msg}"),
        }
    }
}

impl std::error::Error for HarnessError {}

impl From<std::io::Error> for HarnessError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for HarnessError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// An LSP client that communicates with a spawned `acdc-lsp` process.
pub struct LspTestClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl LspTestClient {
    /// Spawn a new `acdc-lsp` process and return a client handle.
    ///
    /// # Errors
    ///
    /// Returns an error if the binary cannot be spawned or stdio pipes
    /// cannot be acquired.
    pub fn new() -> Result<Self, HarnessError> {
        let bin = env!("CARGO_BIN_EXE_acdc-lsp");
        let mut child = Command::new(bin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(HarnessError::Spawn)?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| HarnessError::Protocol("stdin not piped".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| HarnessError::Protocol("stdout not piped".into()))?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
        })
    }

    /// Send an LSP `initialize` request followed by `initialized` notification.
    ///
    /// # Errors
    ///
    /// Returns an error if the initialize handshake fails.
    pub fn initialize(&mut self) -> Result<Value, HarnessError> {
        let result = self.send_request(
            "initialize",
            json!({
                "processId": null,
                "capabilities": {},
                "rootUri": null
            }),
        )?;
        self.send_notification("initialized", json!({}))?;
        Ok(result)
    }

    /// Send a `textDocument/didOpen` notification.
    ///
    /// # Errors
    ///
    /// Returns an error if the notification cannot be sent.
    pub fn open_document(&mut self, uri: &str, content: &str) -> Result<(), HarnessError> {
        self.send_notification(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "asciidoc",
                    "version": 1,
                    "text": content
                }
            }),
        )?;
        // Give the server a moment to process the document and send diagnostics
        std::thread::sleep(Duration::from_millis(100));
        Ok(())
    }

    /// Send a `shutdown` request followed by `exit` notification.
    pub fn shutdown(&mut self) {
        let _ = self.send_request("shutdown", Value::Null);
        let _ = self.send_notification("exit", Value::Null);
        // Wait briefly for clean exit, then force kill
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            std::thread::sleep(Duration::from_millis(500));
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
        }
    }

    /// Send a JSON-RPC request and return the `result` field from the response.
    ///
    /// Skips any server-initiated notifications (e.g. `textDocument/publishDiagnostics`)
    /// while waiting for the matching response.
    ///
    /// # Errors
    ///
    /// Returns an error on I/O failure, JSON parse failure, or if the server
    /// responds with an LSP error.
    pub fn send_request(&mut self, method: &str, params: Value) -> Result<Value, HarnessError> {
        let id = self.next_id;
        self.next_id += 1;

        let mut message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
        });
        if !params.is_null()
            && let Some(obj) = message.as_object_mut()
        {
            obj.insert("params".into(), params);
        }

        self.write_message(&message)?;

        // Read messages until we get a response matching our request ID
        loop {
            let msg = self.read_message()?;

            // Check if this is the response to our request
            if msg.get("id").and_then(Value::as_i64) == Some(id) {
                if let Some(error) = msg.get("error") {
                    return Err(HarnessError::Lsp {
                        method: method.into(),
                        error: error.clone(),
                    });
                }
                return Ok(msg.get("result").cloned().unwrap_or(Value::Null));
            }

            // If it's a server request (has "method" and "id"), send an empty response
            if msg.get("method").is_some()
                && let Some(server_id) = msg.get("id").cloned()
            {
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": server_id,
                    "result": null
                });
                self.write_message(&response)?;
            }

            // Otherwise it's a notification — skip it and keep reading
        }
    }

    /// Send a JSON-RPC notification (no response expected).
    ///
    /// # Errors
    ///
    /// Returns an error if the message cannot be written.
    pub fn send_notification(&mut self, method: &str, params: Value) -> Result<(), HarnessError> {
        let mut message = json!({
            "jsonrpc": "2.0",
            "method": method,
        });
        if !params.is_null()
            && let Some(obj) = message.as_object_mut()
        {
            obj.insert("params".into(), params);
        }
        self.write_message(&message)
    }

    /// Write a JSON-RPC message with `Content-Length` header to stdin.
    fn write_message(&mut self, message: &Value) -> Result<(), HarnessError> {
        let body = serde_json::to_string(message)?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        self.stdin.write_all(header.as_bytes())?;
        self.stdin.write_all(body.as_bytes())?;
        self.stdin.flush()?;
        Ok(())
    }

    /// Read a single JSON-RPC message from stdout.
    ///
    /// Parses the `Content-Length` header and reads exactly that many bytes.
    fn read_message(&mut self) -> Result<Value, HarnessError> {
        // Read headers until blank line
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            let n = self.stdout.read_line(&mut line)?;

            if n == 0 {
                return Err(HarnessError::Protocol(
                    "EOF while reading LSP message headers".into(),
                ));
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }

            if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
                content_length =
                    Some(len_str.parse().map_err(|e| {
                        HarnessError::Protocol(format!("invalid Content-Length: {e}"))
                    })?);
            }
        }

        let len = content_length
            .ok_or_else(|| HarnessError::Protocol("missing Content-Length header".into()))?;
        let mut body = vec![0u8; len];
        self.stdout.read_exact(&mut body)?;

        Ok(serde_json::from_slice(&body)?)
    }
}

impl Drop for LspTestClient {
    fn drop(&mut self) {
        // Best-effort cleanup
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
