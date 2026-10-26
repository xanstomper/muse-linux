//! Thin client: the desktop app proxies tool calls to the companion daemon
//! (muse-daemon, systemd user service) over Tailscale. All policy lives in
//! the daemon — the app never enforces, it just forwards and displays.
//! Hand-rolled HTTP to keep deps at zero (transport is Tailscale-encrypted).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{Read, Write};
use std::time::Duration;

pub fn daemon_url() -> String {
    std::env::var("MUSE_DAEMON_URL").unwrap_or_else(|_| "http://100.93.60.2:7300".into())
}

pub fn token_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::Path::new(&home).join(".config/muse-daemon/token")
}

fn read_token() -> Option<String> {
    std::fs::read_to_string(token_path())
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DaemonReply {
    pub id: String,
    pub ok: bool,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<Value>,
}

/// POST JSON, return response body (no TLS — bind is Tailscale-only).
fn http_post(url: &str, path: &str, body: &str, token: Option<&str>) -> Result<String, String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| "daemon url must be http:// (tailscale transport)".to_string())?;
    let hostport = rest.split('/').next().unwrap_or(rest);
    let mut stream =
        std::net::TcpStream::connect(hostport).map_err(|e| format!("daemon connect: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(90)))
        .map_err(|e| e.to_string())?;
    let auth = token
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {hostport}\r\nContent-Type: application/json\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("daemon write: {e}"))?;
    let mut resp = Vec::new();
    stream
        .read_to_end(&mut resp)
        .map_err(|e| format!("daemon read: {e}"))?;
    let text = String::from_utf8_lossy(&resp);
    // split headers from body on the first \r\n\r\n
    text.split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .ok_or_else(|| "daemon: malformed response".into())
}

/// Forward one tool call to the daemon. Returns the tool result, or the
/// daemon's error message (denied/blocked/timeout) as Err.
pub fn call_tool(tool: &str, arguments: &Value) -> Result<Value, String> {
    let token = read_token()
        .ok_or_else(|| "no daemon token — is muse-daemon installed? (systemctl --user start muse-daemon)".to_string())?;
    let id = format!(
        "app-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let body = serde_json::json!({"id": id, "tool": tool, "arguments": arguments}).to_string();
    let resp = http_post(&daemon_url(), "/call", &body, Some(&token))?;
    let reply: DaemonReply =
        serde_json::from_str(&resp).map_err(|e| format!("daemon reply: {e} ({resp})"))?;
    if reply.ok {
        Ok(reply.result.unwrap_or(Value::Null))
    } else {
        let msg = reply
            .error
            .and_then(|e| e.get("message").and_then(|m| m.as_str()).map(String::from))
            .unwrap_or_else(|| "daemon denied".into());
        Err(msg)
    }
}

#[tauri::command]
pub fn daemon_call(tool: String, arguments: Value) -> Result<Value, String> {
    call_tool(&tool, &arguments)
}

/// Health probe for the UI status chip.
#[tauri::command]
pub fn daemon_status() -> Value {
    let health = http_post(&daemon_url(), "/health", "", None)
        .ok()
        .and_then(|b| serde_json::from_str::<Value>(&b).ok())
        .and_then(|v| v.get("ok").and_then(|o| o.as_bool()))
        .unwrap_or(false);
    serde_json::json!({
        "url": daemon_url(),
        "reachable": health,
        "token_present": read_token().is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Canned one-shot HTTP server: proves client framing + reply parsing
    /// without touching the real daemon.
    #[test]
    fn client_parses_daemon_reply() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 2048];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let body = r#"{"id":"t1","ok":true,"result":{"stdout":"ok42"},"duration_ms":3}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            s.write_all(resp.as_bytes()).unwrap();
            req
        });
        let url = format!("http://127.0.0.1:{port}");
        // client connects FIRST — join before connect deadlocks both sides
        let resp = http_post(&url, "/call", r#"{"id":"t1","tool":"shell.exec","arguments":{"cmd":"x"}}"#, Some("testtoken"))
            .expect("post");
        // then assert the captured request carried the bearer token + JSON body
        let seen = server.join().unwrap();
        assert!(seen.contains("Authorization: Bearer testtoken"));
        assert!(seen.contains("\"tool\":\"shell.exec\""));
        let reply: DaemonReply = serde_json::from_str(&resp).expect("parse");
        assert!(reply.ok);
        assert_eq!(reply.result.unwrap()["stdout"], "ok42");
    }

    #[test]
    fn error_reply_maps_to_err() {
        let reply_json = r#"{"id":"t2","ok":false,"error":{"kind":"denied","message":"capability 'shell' is off"}}"#;
        let reply: DaemonReply = serde_json::from_str(reply_json).unwrap();
        assert!(!reply.ok);
        assert_eq!(
            reply.error.unwrap()["message"],
            "capability 'shell' is off"
        );
    }

    #[test]
    fn token_path_under_config() {
        std::env::set_var("HOME", "/tmp/fakehome-muse");
        assert!(token_path().ends_with(".config/muse-daemon/token"));
    }
}
