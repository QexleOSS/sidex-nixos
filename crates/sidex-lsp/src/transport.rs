//! JSON-RPC 2.0 transport over stdio for LSP communication.
//!
//! Implements the base protocol described in the LSP specification: messages
//! are framed with `Content-Length` headers and encoded as JSON.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};

/// A JSON-RPC 2.0 message exchanged between client and server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    /// A request from client to server (or vice-versa).
    Request {
        jsonrpc: String,
        id: RequestId,
        method: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        params: Option<Value>,
    },
    /// A response to a prior request.
    Response {
        jsonrpc: String,
        id: RequestId,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<JsonRpcError>,
    },
    /// A notification (no `id` field).
    Notification {
        jsonrpc: String,
        method: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        params: Option<Value>,
    },
}

/// A JSON-RPC request identifier — either a number or a string.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum RequestId {
    /// Numeric identifier.
    Number(i64),
    /// String identifier.
    String(String),
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcError {
    /// Error code.
    pub code: i64,
    /// Human-readable message.
    pub message: String,
    /// Optional structured data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcMessage {
    /// Creates a new JSON-RPC request.
    pub fn request(id: i64, method: impl Into<String>, params: Option<Value>) -> Self {
        Self::Request {
            jsonrpc: "2.0".to_owned(),
            id: RequestId::Number(id),
            method: method.into(),
            params,
        }
    }

    /// Creates a new JSON-RPC notification (no `id`).
    pub fn notification(method: impl Into<String>, params: Option<Value>) -> Self {
        Self::Notification {
            jsonrpc: "2.0".to_owned(),
            method: method.into(),
            params,
        }
    }

    /// Creates a successful JSON-RPC response.
    pub fn response_ok(id: RequestId, result: Value) -> Self {
        Self::Response {
            jsonrpc: "2.0".to_owned(),
            id,
            result: Some(result),
            error: None,
        }
    }
}

/// Encodes a [`JsonRpcMessage`] into a `Content-Length`-framed byte buffer.
pub fn encode_message(msg: &JsonRpcMessage) -> Result<Vec<u8>> {
    let body = serde_json::to_string(msg).context("failed to serialize JSON-RPC message")?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    let mut buf = Vec::with_capacity(header.len() + body.len());
    buf.extend_from_slice(header.as_bytes());
    buf.extend_from_slice(body.as_bytes());
    Ok(buf)
}

/// Decodes a `Content-Length`-framed JSON-RPC message from raw bytes.
///
/// Returns the parsed message and the number of bytes consumed.
pub fn decode_message(input: &[u8]) -> Result<(JsonRpcMessage, usize)> {
    let input_str = std::str::from_utf8(input).context("invalid UTF-8 in message")?;

    let header_end = input_str
        .find("\r\n\r\n")
        .context("missing header terminator")?;
    let headers_str = &input_str[..header_end];

    let content_length = parse_content_length(headers_str)?;
    let body_start = header_end + 4;
    let total = body_start + content_length;

    if input.len() < total {
        bail!(
            "incomplete message: expected {total} bytes, got {}",
            input.len()
        );
    }

    let body = &input_str[body_start..total];
    let msg: JsonRpcMessage =
        serde_json::from_str(body).context("failed to parse JSON-RPC message body")?;
    Ok((msg, total))
}

fn parse_content_length(headers: &str) -> Result<usize> {
    let headers_map = parse_headers(headers);
    let value = headers_map
        .get("content-length")
        .context("missing Content-Length header")?;
    value
        .trim()
        .parse::<usize>()
        .context("invalid Content-Length value")
}

fn parse_headers(raw: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in raw.split("\r\n") {
        if let Some((key, value)) = line.split_once(':') {
            map.insert(key.trim().to_lowercase(), value.trim().to_owned());
        }
    }
    map
}

/// Asynchronous LSP transport over a child process's stdio.
pub struct LspTransport {
    writer: ChildStdin,
    reader: BufReader<ChildStdout>,
}

impl LspTransport {
    /// Wraps a child process's stdin/stdout into an LSP transport.
    pub fn new(stdin: ChildStdin, stdout: ChildStdout) -> Self {
        Self {
            writer: stdin,
            reader: BufReader::new(stdout),
        }
    }

    /// Sends a JSON-RPC message with the proper `Content-Length` framing.
    pub async fn send(&mut self, message: &JsonRpcMessage) -> Result<()> {
        let encoded = encode_message(message)?;
        self.writer
            .write_all(&encoded)
            .await
            .context("failed to write to server stdin")?;
        self.writer
            .flush()
            .await
            .context("failed to flush server stdin")?;
        Ok(())
    }

    /// Reads the next JSON-RPC message from the server.
    pub async fn recv(&mut self) -> Result<JsonRpcMessage> {
        let content_length = self.read_headers().await?;

        let mut body = vec![0u8; content_length];
        self.reader
            .read_exact(&mut body)
            .await
            .context("failed to read message body")?;

        let msg: JsonRpcMessage = serde_json::from_slice(&body)
            .context("failed to parse JSON-RPC message from server")?;
        Ok(msg)
    }

    /// Reads headers until the blank line, returning the `Content-Length`.
    async fn read_headers(&mut self) -> Result<usize> {
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            let bytes_read = self
                .reader
                .read_line(&mut line)
                .await
                .context("failed to read header line")?;
            if bytes_read == 0 {
                bail!("unexpected EOF while reading headers");
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some((key, value)) = trimmed.split_once(':') {
                if key.trim().eq_ignore_ascii_case("content-length") {
                    content_length = Some(
                        value
                            .trim()
                            .parse()
                            .context("invalid Content-Length value")?,
                    );
                }
            }
        }
        content_length.context("missing Content-Length header")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_request() {
        let msg = JsonRpcMessage::request(1, "initialize", Some(serde_json::json!({})));
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"method\":\"initialize\""));
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
    }

    #[test]
    fn serialize_notification() {
        let msg = JsonRpcMessage::notification(
            "textDocument/didOpen",
            Some(serde_json::json!({"uri": "file:///test.rs"})),
        );
        let json = serde_json::to_string(&msg).unwrap();
        assert!(!json.contains("\"id\""));
        assert!(json.contains("\"method\":\"textDocument/didOpen\""));
    }

    #[test]
    fn deserialize_response() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}"#;
        let msg: JsonRpcMessage = serde_json::from_str(json).unwrap();
        match msg {
            JsonRpcMessage::Response { id, result, .. } => {
                assert_eq!(id, RequestId::Number(1));
                assert!(result.is_some());
            }
            _ => panic!("expected Response"),
        }
    }

    #[test]
    fn deserialize_error_response() {
        let json =
            r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32600,"message":"Invalid Request"}}"#;
        let msg: JsonRpcMessage = serde_json::from_str(json).unwrap();
        match msg {
            JsonRpcMessage::Response { id, error, .. } => {
                assert_eq!(id, RequestId::Number(2));
                let err = error.unwrap();
                assert_eq!(err.code, -32600);
                assert_eq!(err.message, "Invalid Request");
            }
            _ => panic!("expected Response"),
        }
    }

    #[test]
    fn deserialize_notification() {
        let json = r#"{"jsonrpc":"2.0","method":"window/logMessage","params":{"type":3,"message":"hello"}}"#;
        let msg: JsonRpcMessage = serde_json::from_str(json).unwrap();
        match msg {
            JsonRpcMessage::Notification { method, params, .. } => {
                assert_eq!(method, "window/logMessage");
                assert!(params.is_some());
            }
            _ => panic!("expected Notification"),
        }
    }

    #[test]
    fn content_length_encode_decode_roundtrip() {
        let msg =
            JsonRpcMessage::request(42, "test/method", Some(serde_json::json!({"key": "value"})));
        let encoded = encode_message(&msg).unwrap();
        let (decoded, consumed) = decode_message(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(
            serde_json::to_value(&msg).unwrap(),
            serde_json::to_value(&decoded).unwrap(),
        );
    }

    #[test]
    fn content_length_framing_format() {
        let msg = JsonRpcMessage::notification("test", None);
        let encoded = encode_message(&msg).unwrap();
        let s = String::from_utf8(encoded).unwrap();
        assert!(s.starts_with("Content-Length: "));
        assert!(s.contains("\r\n\r\n"));
    }

    #[test]
    fn decode_incomplete_message_errors() {
        let msg = JsonRpcMessage::request(1, "test", None);
        let encoded = encode_message(&msg).unwrap();
        let truncated = &encoded[..encoded.len() - 5];
        assert!(decode_message(truncated).is_err());
    }

    #[test]
    fn request_id_string_variant() {
        let json = r#"{"jsonrpc":"2.0","id":"abc","method":"test","params":null}"#;
        let msg: JsonRpcMessage = serde_json::from_str(json).unwrap();
        match msg {
            JsonRpcMessage::Request { id, .. } => {
                assert_eq!(id, RequestId::String("abc".to_owned()));
            }
            _ => panic!("expected Request"),
        }
    }
}
