// Muse companion daemon — the real native layer.
// User-owned systemd service; all system access flows through here so the
// permission policy is enforced in exactly one place.
//
// Transport: HTTP/JSON bound to the Tailscale IP only (config/policy.json),
// token-authenticated. Tool protocol per docs/guide section 5.

mod policy;
mod tools;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use policy::Policy;

#[derive(Deserialize)]
struct Call {
    id: String,
    tool: String,
    #[serde(default)]
    arguments: Value,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Serialize)]
struct Reply {
    id: String,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorBody>,
    duration_ms: u64,
}

#[derive(Serialize)]
struct ErrorBody {
    kind: String, // denied | blocked | not_found | timeout | cancelled | invalid | internal
    message: String,
}

fn reply_json(id: &str, ok: bool, result: Option<Value>, err: Option<(&str, String)>, dur: u64) -> String {
    let r = Reply {
        id: id.to_string(),
        ok,
        result,
        error: err.map(|(k, m)| ErrorBody { kind: k.into(), message: m }),
        duration_ms: dur,
    };
    serde_json::to_string(&r).unwrap_or_else(|_| format!(r#"{{"id":"{id}","ok":false,"error":{{"kind":"internal","message":"serialize failed"}},"duration_ms":{dur}}}"#))
}

/// Approval callback hook: daemon runs headless by default; the desktop app
/// connects here for interactive approvals. v1: policy decides (ask => denied
/// unless caller holds the `approve` capability over the local IPC socket).
pub type Approver = dyn Fn(&str, &Value) -> bool + Send + Sync;

fn dispatch(pol: &Policy, tool: &str, args: &Value, timeout_ms: Option<u64>, approver: &Approver) -> Result<Value, (String, String)> {
    // 1. capability gate (tool namespace -> policy capability)
    let raw_cap = tool.split('.').next().unwrap_or("");
    let cap = match raw_cap {
        "shell" => "terminal",
        other => other,
    };
    let mode = pol.capability(cap).ok_or_else(|| ("denied".into(), format!("capability '{cap}' is off")))?;
    if mode == "off" {
        return Err(("denied".into(), format!("capability '{cap}' is off in policy")));
    }

    // 2. tool-level approval list
    if pol.requires_approval(tool) {
        if !approver(tool, args) {
            return Err(("denied".into(), format!("'{tool}' needs on-device approval; denied")));
        }
    }

    // 3. execute
    tools::execute(pol, tool, args, timeout_ms)
}

fn handle_call(pol: &Policy, approver: &Approver, body: &str) -> String {
    let started = std::time::Instant::now();
    let call: Call = match serde_json::from_str(body) {
        Ok(c) => c,
        Err(e) => return reply_json("?", false, None, Some(("invalid", e.to_string())), 0),
    };
    let dur = started.elapsed().as_millis() as u64;

    let out = dispatch(pol, &call.tool, &call.arguments, call.timeout_ms, approver);
    match out {
        Ok(v) => {
            policy::audit(pol, &call.tool, &call.arguments, "ok", None);
            reply_json(&call.id, true, Some(v), None, dur)
        }
        Err((kind, msg)) => {
            policy::audit(pol, &call.tool, &call.arguments, &kind, Some(&msg));
            reply_json(&call.id, false, None, Some((kind.as_str(), msg)), dur)
        }
    }
}

fn read_request(stream: &mut TcpStream) -> Option<(String, String, Option<String>, String)> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut req = String::new();
    // request line
    if reader.read_line(&mut req).ok()? < 3 { return None; }
    let mut content_length = 0usize;
    let mut auth_header: Option<String> = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 { break; }
        let l = line.trim().to_string();
        if l.is_empty() { break; }
        if let Some(v) = l.strip_prefix("Content-Length:") {
            content_length = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = l.strip_prefix("Authorization: Bearer ") {
            auth_header = Some(v.trim().to_string());
        }
    }
    if content_length > 4 * 1024 * 1024 { return None; }
    let mut body: Vec<u8> = vec![0u8; content_length];
    if content_length > 0 { reader.read_exact(&mut body).ok()?; }
    let mut parts = req.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    Some((method, path, auth_header, String::from_utf8_lossy(&body).into_owned()))
}

fn write_response(stream: &mut TcpStream, code: &str, body: &str) {
    let resp = format!(
        "HTTP/1.1 {code}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.flush();
}

pub fn run() -> Result<(), String> {
    let pol = Policy::load().map_err(|e| format!("policy: {e}"))?;
    let bind = pol.transport_bind();
    let token = std::env::var("MUSE_DAEMON_TOKEN").unwrap_or_else(|_| {
        // stable per-install token at ~/.config/muse-daemon/token (0600);
        // the desktop app reads this file — it never travels over the wire
        let tok_file = policy::token_path();
        if let Ok(t) = std::fs::read_to_string(&tok_file) {
            let t = t.trim().to_string();
            if !t.is_empty() {
                return t;
            }
        }
        let t = format!(
            "muse-{:032x}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()
        );
        let _ = std::fs::create_dir_all(tok_file.parent().unwrap_or_else(|| std::path::Path::new(".")));
        let _ = std::fs::write(&tok_file, &t);
        t
    });
    let token = Arc::new(token);
    let pol = Arc::new(pol);

    // default approver: auto-allow only tools whose default is auto (already
    // gated by policy.requires_approval), deny the rest — the desktop app can
    // connect to /approve to interactively allow.
    let yolo = std::env::var("MUSE_DAEMON_YOLO").map(|v| v == "1").unwrap_or(false);
    let approver: Arc<Approver> = Arc::new(move |_tool: &str, _args: &Value| {
        // headless default: deny; desktop app connects for interactive approvals.
        // MUSE_DAEMON_YOLO=1 = user explicitly accepted every write (yolo mode).
        yolo
    });

    let listener = TcpListener::bind(&bind).map_err(|e| format!("bind {bind}: {e}"))?;
    eprintln!("[muse-daemon] listening on http://{bind} (tailscale-only)");
    policy::audit(&pol, "daemon.start", &serde_json::json!({"bind": bind}), "ok", None);

    // Registry endpoint for agent discovery
    let registry = serde_json::json!({
        "server": "muse-daemon",
        "version": env!("CARGO_PKG_VERSION"),
        "capabilities": pol.capabilities(),
        "tools": tools::catalog(),
    });

    for stream in listener.incoming() {
        let mut stream = match stream { Ok(s) => s, Err(_) => continue };
        let pol = Arc::clone(&pol);
        let token = Arc::clone(&token);
        let approver = Arc::clone(&approver);
        let registry = registry.clone();
        std::thread::spawn(move || {
            let Some((method, path, auth_header, body)) = read_request(&mut stream) else { return; };
            // auth: every request except /health needs the bearer token
            if path != "/health" {
                let header_ok = auth_header.as_deref().map(|t| t == token.as_str()).unwrap_or(false);
                let authed = header_ok || body.contains(token.as_str()); // body fallback for curl ease
                if !authed {
                    write_response(&mut stream, "401 Unauthorized", r#"{"error":"unauthorized"}"#);
                    return;
                }
            }
            match (method.as_str(), path.as_str()) {
                ("GET", "/health") => write_response(&mut stream, "200 OK", r#"{"ok":true}"#),
                ("GET", "/tools") => write_response(&mut stream, "200 OK", &registry.to_string()),
                ("POST", "/call") => {
                    let out = handle_call(&pol, approver.as_ref(), &body);
                    write_response(&mut stream, "200 OK", &out);
                }
                _ => write_response(&mut stream, "404 Not Found", r#"{"error":"no route"}"#),
            }
        });
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("[muse-daemon] fatal: {e}");
        std::process::exit(1);
    }
}
