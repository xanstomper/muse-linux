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

/// HTTP request helper. Empty body ⇒ GET, otherwise POST JSON.
/// (No TLS — bind is Tailscale-encrypted.)
fn http_post(url: &str, path: &str, body: &str, token: Option<&str>) -> Result<String, String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| "daemon url must be http:// (tailscale transport)".to_string())?;
    let mut hostport = rest.split('/').next().unwrap_or(rest).to_string();
    // accept bare host/IP (Tailscale MagicDNS names) — default to the daemon port
    if !hostport.contains(':') {
        hostport.push_str(":7300");
    }
    use std::net::ToSocketAddrs;
    let addr = hostport
        .to_socket_addrs()
        .map_err(|e| format!("resolve {hostport}: {e}"))?
        .next()
        .ok_or_else(|| format!("resolve {hostport}: no addresses"))?;
    let mut stream =
        std::net::TcpStream::connect(addr).map_err(|e| format!("daemon connect: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(90)))
        .map_err(|e| e.to_string())?;
    let auth = token
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let method = if body.is_empty() { "GET" } else { "POST" };
    let req = if body.is_empty() {
        format!("{method} {path} HTTP/1.1\r\nHost: {hostport}\r\n{auth}Content-Length: 0\r\nConnection: close\r\n\r\n")
    } else {
        format!("{method} {path} HTTP/1.1\r\nHost: {hostport}\r\nContent-Type: application/json\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
    };
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
    // saved connection (from the UI connect panel) wins over env/default
    let (url, token) = match load_connection() {
        Some(c) => (c.url, c.token),
        None => (
            daemon_url(),
            read_token().ok_or_else(|| {
                "no paired device — open Settings → Devices and connect one".to_string()
            })?,
        ),
    };
    let id = format!(
        "app-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    let body = serde_json::json!({"id": id, "tool": tool, "arguments": arguments}).to_string();
    let resp = http_post(&url, "/call", &body, Some(&token))?;
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
    let conn = load_connection();
    let url = conn.as_ref().map(|c| c.url.clone()).unwrap_or_else(daemon_url);
    let health = http_post(&url, "/health", "", None)
        .ok()
        .and_then(|b| serde_json::from_str::<Value>(&b).ok())
        .and_then(|v| v.get("ok").and_then(|o| o.as_bool()))
        .unwrap_or(false);
    let token_present = conn.as_ref().map(|c| !c.token.is_empty()).unwrap_or(false)
        || read_token().is_some();
    serde_json::json!({
        "url": url,
        "reachable": health,
        "token_present": token_present,
        "paired": conn.is_some(),
    })
}

// ---------- device connect (UI panel backend) ----------

/// Connection config for the paired Linux device, persisted at
/// ~/.config/muse-daemon/connection.json (0600). Used by daemon_call too
/// via daemon_url().
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Connection {
    pub url: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub label: String,
}

pub fn connection_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::Path::new(&home).join(".config/muse-daemon/connection.json")
}

fn load_connection() -> Option<Connection> {
    let raw = std::fs::read_to_string(connection_path()).ok()?;
    serde_json::from_str(&raw).ok()
}

fn save_connection(conn: &Connection) -> Result<(), String> {
    if !conn.url.starts_with("http://") && !conn.url.starts_with("https://") {
        return Err("url must start with http:// or https://".into());
    }
    if conn.token.is_empty() {
        return Err("token is required (paste it from the target device)".into());
    }
    let p = connection_path();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, serde_json::to_string_pretty(conn).unwrap()).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Probe a device without saving: GET /health, then GET /tools with the token.
#[tauri::command]
pub fn daemon_probe(url: String, token: String) -> Value {
    let ok_health = http_post(&url, "/health", "", None).is_ok();
    let tools = http_post(&url, "/tools", "", Some(&token)).ok();
    let tool_count = tools
        .as_deref()
        .and_then(|b| serde_json::from_str::<Value>(b).ok())
        .and_then(|v| v.get("tools").and_then(|t| t.as_array().map(|a| a.len())))
        .unwrap_or(0);
    serde_json::json!({
        "reachable": ok_health,
        "authorized": tool_count > 0,
        "tools": tool_count,
        "error": if ok_health && tool_count == 0 { Some("connected but token rejected".to_string()) } else { None },
    })
}

/// Save (pair) a device connection and verify it in one step.
#[tauri::command]
pub fn daemon_connect(url: String, token: String, label: String) -> Result<Value, String> {
    let conn = Connection { url: url.trim().trim_end_matches('/').to_string(), token: token.trim().to_string(), label };
    let probe = daemon_probe(conn.url.clone(), conn.token.clone());
    if !probe["authorized"].as_bool().unwrap_or(false) {
        return Err(probe["error"]
            .as_str()
            .unwrap_or("device unreachable — check the URL (e.g. http://100.x.y.z:7300) and that muse-daemon is running")
            .to_string());
    }
    save_connection(&conn)?;
    Ok(probe)
}

/// Forget the paired device.
#[tauri::command]
pub fn daemon_disconnect() -> Result<(), String> {
    let p = connection_path();
    if p.exists() {
        std::fs::remove_file(&p).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Current paired device (token redacted) for the UI.
#[tauri::command]
pub fn daemon_get_connection() -> Value {
    match load_connection() {
        Some(c) => serde_json::json!({
            "paired": true,
            "url": c.url,
            "label": c.label,
            "token_preview": format!("…{}", &c.token[c.token.len().saturating_sub(4)..]),
        }),
        None => serde_json::json!({"paired": false}),
    }
}

/// One-step pairing from the app side:
///  1. POST nothing — GET {base}/pair/start → {code}
///  2. poll GET {base}/pair/poll?code=NN until approved (daemon hands the token once)
///  3. save the connection and verify it
/// The user approves ON the device with `muse pair <code>` — physical trust.
#[tauri::command]
pub fn daemon_pair_start(base: String) -> Result<Value, String> {
    let url = base.trim().trim_end_matches('/').to_string();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("device address must start with http:// or https://".into());
    }
    // reuse http_post (it works for GET-ish empty bodies; daemon routes GET /pair/start)
    let resp = http_post(&url, "/pair/start", "", None)?;
    serde_json::from_str::<Value>(&resp).map_err(|e| format!("pair/start reply: {e}"))
}

#[tauri::command]
pub fn daemon_pair_poll(base: String, code: String) -> Result<Value, String> {
    let url = base.trim().trim_end_matches('/').to_string();
    let resp = http_post(&url, &format!("/pair/poll?code={}", code.trim()), "", None)?;
    serde_json::from_str::<Value>(&resp).map_err(|e| format!("pair/poll reply: {e}"))
}

/// Make daemon_call use the saved connection if present (fallback to env/def).
pub fn effective_url() -> String {
    load_connection().map(|c| c.url).unwrap_or_else(daemon_url)
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
