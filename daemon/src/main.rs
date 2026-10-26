// Muse companion daemon — the real native layer.
// User-owned systemd service; all system access flows through here so the
// permission policy is enforced in exactly one place.
//
// Transport: HTTP/JSON bound to the Tailscale IP only (config/policy.json),
// token-authenticated. Tool protocol per docs/guide section 5.

mod pairing;
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

/// Extract ?code= from a /pair/poll?code=NNNNNN path (best-effort).
fn path_split_code(path: &str) -> Option<String> {
    path.split("code=").nth(1).map(|c| c.split('&').next().unwrap_or("").to_string())
}

pub fn run() -> Result<(), String> {
    let pol = Policy::load().map_err(|e| format!("policy: {e}"))?;
    let pairing_state = std::sync::Arc::new(pairing::PairingState::new());
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
        let pairing_state = std::sync::Arc::clone(&pairing_state);
        std::thread::spawn(move || {
            let Some((method, path, auth_header, body)) = read_request(&mut stream) else { return; };
            // auth: everything except /health and the pairing handshake needs
            // the bearer token (pairing is how you GET a token)
            // routing ignores query strings
            let route_path = path.split('?').next().unwrap_or("").to_string();
            let unauthed_route = path == "/health"
                || path.starts_with("/pair/start")
                || path.starts_with("/pair/poll")
                || path.starts_with("/pair/approve");
            if !unauthed_route {
                let header_ok = auth_header.as_deref().map(|t| t == token.as_str()).unwrap_or(false);
                let authed = header_ok || body.contains(token.as_str()); // body fallback for curl ease
                if !authed {
                    write_response(&mut stream, "401 Unauthorized", r#"{"error":"unauthorized"}"#);
                    return;
                }
            }
            match (method.as_str(), route_path.as_str()) {
                ("GET", "/health") => write_response(&mut stream, "200 OK", r#"{"ok":true}"#),
                ("GET", "/tools") => write_response(&mut stream, "200 OK", &registry.to_string()),
                ("POST", "/call") => {
                    let out = handle_call(&pol, approver.as_ref(), &body);
                    write_response(&mut stream, "200 OK", &out);
                }
                // ---- one-step pairing (see pairing.rs) ----
                // App asks for a code; the user approves on the device itself.
                ("GET", "/pair/start") => {
                    let code = pairing_state.start();
                    policy::audit(&pol, "pair.start", &serde_json::json!({}), "ok", None);
                    eprintln!("[muse-daemon] PAIRING CODE: {code} — approve with: muse pair {code}");
                    write_response(&mut stream, "200 OK", &serde_json::json!({"code": code}).to_string());
                }
                // Approval is LOCAL-ONLY: must come from the device itself.
                ("POST", "/pair/approve") => {
                    let peer = stream.peer_addr().map(|a| a.ip().to_string()).unwrap_or_default();
                    let is_local = matches!(peer.as_str(), "127.0.0.1" | "::1" | "::ffff:127.0.0.1");
                    if !is_local {
                        policy::audit(&pol, "pair.approve", &serde_json::json!({"peer": peer}), "denied", Some("remote approval not allowed"));
                        write_response(&mut stream, "403 Forbidden", r#"{"error":"approve on the device itself (localhost)"}"#);
                        return;
                    }
                    let code = serde_json::from_str::<Value>(&body)
                        .ok()
                        .and_then(|v| v.get("code").and_then(|c| c.as_str()).map(String::from))
                        .unwrap_or_default();
                    let ok = pairing_state.approve(&code);
                    policy::audit(&pol, "pair.approve", &serde_json::json!({"result": ok}), if ok {"ok"} else {"invalid"}, None);
                    if ok {
                        write_response(&mut stream, "200 OK", r#"{"approved":true}"#);
                    } else {
                        write_response(&mut stream, "400 Bad Request", r#"{"error":"invalid or expired code"}"#);
                    }
                }
                // App polls with the code it displayed; on approval gets the token ONCE.
                ("GET", "/pair/poll") => {
                    // the poller must present the code it received from /pair/start;
                    // approval handoff is one-shot
                    let qcode = path_split_code(&path).unwrap_or_default();
                    let matches = pairing_state.pending().as_deref() == Some(qcode.as_str());
                    let approved = matches && pairing_state.take_approved();
                    if approved {
                        policy::audit(&pol, "pair.complete", &serde_json::json!({}), "ok", None);
                        write_response(&mut stream, "200 OK", &serde_json::json!({"token": token.as_str()}).to_string());
                    } else {
                        write_response(&mut stream, "200 OK", r#"{"pending":true}"#);
                    }
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
