//! Browser native-messaging host.
//!
//! Bridges a WebExtension to a running RDM app. It speaks the browser's native
//! messaging framing — a 4-byte little-endian length followed by UTF-8 JSON — on
//! stdin/stdout, and forwards each message to RDM's local socket
//! ([`rdm_core::ipc`]). The host is deliberately thin: all download logic lives
//! in the app.

use std::io::{self, Read, Write};

use serde_json::{json, Map, Value};

use rdm_core::ipc::{self, Request};

/// Read one length-prefixed message. `Ok(None)` means stdin closed cleanly.
fn read_message(reader: &mut impl Read) -> io::Result<Option<Value>> {
    let mut len = [0u8; 4];
    match reader.read_exact(&mut len) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err),
    }
    let len = u32::from_le_bytes(len) as usize;
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body).unwrap_or(Value::Null)))
}

/// Write one length-prefixed message.
fn write_message(writer: &mut impl Write, value: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(value).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    writer.write_all(&(body.len() as u32).to_le_bytes())?;
    writer.write_all(&body)?;
    writer.flush()
}

fn opt_string(message: &Value, key: &str) -> Option<String> {
    message
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

/// Translate a browser message into an IPC request.
fn to_request(message: &Value) -> Request {
    match message.get("action").and_then(Value::as_str) {
        Some("ping") => Request::Ping,
        Some("qualities") => Request::Qualities {
            url: message
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            cookies: opt_string(message, "cookies"),
            user_agent: opt_string(message, "user_agent"),
            referer: opt_string(message, "referer"),
        },
        _ => Request::Add {
            url: message
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            file_name: opt_string(message, "file_name"),
            referer: opt_string(message, "referer"),
            cookies: opt_string(message, "cookies"),
            user_agent: opt_string(message, "user_agent"),
            connections: message
                .get("connections")
                .and_then(Value::as_u64)
                .map(|n| n.min(32) as u8),
            queue: opt_string(message, "queue"),
            height: message
                .get("height")
                .and_then(Value::as_u64)
                .map(|n| n.min(4320) as u32),
            content_type: opt_string(message, "content_type"),
            title: opt_string(message, "title"),
        },
    }
}

fn response_value(resp: ipc::Response) -> Value {
    let mut out = Map::new();
    out.insert("ok".into(), Value::Bool(resp.ok));
    if let Some(id) = resp.id {
        out.insert("id".into(), Value::String(id));
    }
    if let Some(error) = resp.error {
        out.insert("error".into(), Value::String(error));
    }
    Value::Object(out)
}

fn error_value(message: impl Into<String>) -> Value {
    json!({ "ok": false, "error": message.into() })
}

fn handle(runtime: &tokio::runtime::Runtime, message: &Value) -> Value {
    let request = to_request(message);

    // Defence in depth: never forward a non-http(s) URL (the app checks too).
    if let Request::Add { url, .. } = &request {
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return error_value("only http(s) URLs are supported");
        }
    }
    match runtime.block_on(ipc::request(&ipc::default_socket_path(), &request)) {
        Ok(resp) => response_value(resp),
        Err(err) => error_value(format!("RDM is not running ({err})")),
    }
}

fn main() {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("rdm-native-host: could not start the runtime: {err}");
            std::process::exit(1);
        }
    };

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    loop {
        let message = match read_message(&mut reader) {
            Ok(Some(message)) => message,
            Ok(None) => break,
            Err(err) => {
                eprintln!("rdm-native-host: read error: {err}");
                break;
            }
        };

        let response = handle(&runtime, &message);
        if let Err(err) = write_message(&mut writer, &response) {
            eprintln!("rdm-native-host: write error: {err}");
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn framing_round_trips() {
        let mut buf = Vec::new();
        write_message(&mut buf, &json!({ "action": "add", "url": "https://x/y.zip" })).unwrap();

        let mut cursor = Cursor::new(buf);
        let read = read_message(&mut cursor).unwrap().unwrap();
        assert_eq!(read["url"], "https://x/y.zip");
        // Clean EOF at the message boundary.
        assert!(read_message(&mut cursor).unwrap().is_none());
    }

    #[test]
    fn maps_a_browser_message_to_an_add_request() {
        let request = to_request(&json!({
            "action": "add",
            "url": "https://example.test/big.iso",
            "referer": "https://example.test/",
            "connections": 16
        }));

        match request {
            Request::Add {
                url,
                referer,
                connections,
                file_name,
                ..
            } => {
                assert_eq!(url, "https://example.test/big.iso");
                assert_eq!(referer.as_deref(), Some("https://example.test/"));
                assert_eq!(connections, Some(16));
                assert!(file_name.is_none());
            }
            Request::Ping | Request::Qualities { .. } => panic!("expected an Add request"),
        }
    }

    #[test]
    fn rejects_non_http_urls() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let response = handle(&runtime, &json!({ "action": "add", "url": "file:///etc/passwd" }));
        assert_eq!(response["ok"], json!(false));
    }
}
