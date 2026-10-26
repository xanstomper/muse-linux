// Tool registry + executors. Every tool validates policy, then executes,
// returning structured JSON. Blocked = refused, never "ask then run".
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

fn err(kind: &str, msg: &str) -> (String, String) {
    (kind.to_string(), msg.to_string())
}

use crate::policy::redact;
use crate::Policy;

pub fn catalog() -> Value {
    json!([
        {"name":"system.info","risk":"R","desc":"Distro, kernel, DE, display server, uptime"},
        {"name":"system.processes","risk":"R","desc":"Live process list (pid, name, rss)"},
        {"name":"files.list","risk":"R","desc":"List a directory (allowed folders only)"},
        {"name":"files.read","risk":"R","desc":"Read a file (allowed folders only)"},
        {"name":"files.search","risk":"R","desc":"Recursive content search in allowed folders"},
        {"name":"files.write","risk":"W","desc":"Write a file (approval required)"},
        {"name":"shell.exec","risk":"X","desc":"One-shot shell command with timeout, blocked-list enforced"},
        {"name":"shell.session.create","risk":"X","desc":"Persistent interactive shell session"},
        {"name":"shell.session.send","risk":"X","desc":"Send a line to a persistent session"},
        {"name":"shell.session.read","risk":"R","desc":"Read pending output from a session"},
        {"name":"shell.session.kill","risk":"X","desc":"Kill a session's process group"},
        {"name":"windows.list","risk":"R","desc":"List windows (wmctrl)"},
        {"name":"windows.focus","risk":"C","desc":"Focus a window (xdotool)"},
        {"name":"windows.click","risk":"C","desc":"Click at x,y (approval)"},
        {"name":"windows.type","risk":"C","desc":"Type text into focused window (approval)"},
        {"name":"windows.key","risk":"C","desc":"Send key combo to focused window (approval)"},
        {"name":"screen.snap","risk":"R","desc":"Screenshot via scrot (approval per policy)"},
        {"name":"clipboard.read","risk":"R","desc":"Read clipboard (xclip)"},
        {"name":"clipboard.write","risk":"W","desc":"Write clipboard (xclip)"},
        {"name":"notifications.send","risk":"W","desc":"Desktop notification (notify-send)"},
    ])
}

pub fn execute(pol: &Policy, tool: &str, args: &Value, timeout_ms: Option<u64>) -> Result<Value, (String, String)> {
    let timeout = timeout_ms.unwrap_or(pol.default_timeout_ms);
    match tool {
        "system.info" => system_info(pol),
        "system.processes" => system_processes(pol),
        "files.list" => files_list(pol, args),
        "files.read" => files_read(pol, args),
        "files.search" => files_search(pol, args),
        "files.write" => files_write(pol, args),
        "shell.exec" => shell_exec(pol, args, timeout),
        "shell.session.create" => session_create(pol, args),
        "shell.session.send" => session_send(pol, args),
        "shell.session.read" => session_read(pol, args),
        "shell.session.kill" => session_kill(pol, args),
        "windows.list" => windows_list(pol),
        "windows.focus" => windows_focus(pol, args),
        "windows.click" => windows_click(pol, args),
        "windows.type" => windows_type(pol, args),
        "windows.key" => windows_key(pol, args),
        "screen.snap" => screen_snap(pol, args),
        "clipboard.read" => clipboard_read(pol),
        "clipboard.write" => clipboard_write(pol, args),
        "notifications.send" => notify_send(pol, args),
        _ => Err(err("not_found", &format!("unknown tool '{tool}'"))),
    }
}

fn out_path_ok(pol: &Policy, p: &str) -> Result<PathBuf, (String, String)> {
    // reject traversal explicitly
    if p.contains("..") {
        return Err(err("blocked", "path traversal rejected"));
    }
    let pb = PathBuf::from(p);
    if !pol.path_allowed(p) {
        return Err(err("denied", &format!("path '{}' outside allowed folders or blocked", p)));
    }
    Ok(pb)
}

// ---------- system ----------

fn system_info(pol: &Policy) -> Result<Value, (String, String)> {
    let osrelease = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let pretty = osrelease.lines().find(|l| l.starts_with("PRETTY_NAME")).map(|l| l.trim().to_string()).unwrap_or_default();
    let kernel = Command::new("uname").arg("-r").output().ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    let de = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "?".into());
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "?".into());
    let uptime = std::fs::read_to_string("/proc/uptime").ok()
        .and_then(|s| s.split_whitespace().next().and_then(|f| f.parse::<f64>().ok()))
        .map(|s| format!("{:.0}s", s)).unwrap_or_default();
    Ok(json!({
        "os": pretty, "kernel": kernel, "desktop": de, "display": session,
        "uptime": uptime, "user": std::env::var("USER").unwrap_or_default(),
    }))
}

fn system_processes(pol: &Policy) -> Result<Value, (String, String)> {
    let mut procs = Vec::new();
    if let Ok(dir) = std::fs::read_dir("/proc") {
        for e in dir.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.chars().all(|c| c.is_ascii_digit()) { continue; }
            let comm = std::fs::read_to_string(format!("/proc/{name}/comm")).unwrap_or_default().trim().to_string();
            let stat = std::fs::read_to_string(format!("/proc/{name}/stat")).unwrap_or_default();
            let rss_kb = stat.split_whitespace().nth(23).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            procs.push(json!({"pid": name.parse::<u64>().unwrap_or(0), "name": comm, "rss_kb": rss_kb}));
            if procs.len() > 200 { break; }
        }
    }
    procs.sort_by_key(|p| -p["rss_kb"].as_i64().unwrap_or(0));
    Ok(json!({"processes": procs}))
}

// ---------- files ----------

fn files_list(pol: &Policy, args: &Value) -> Result<Value, (String, String)> {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
    let pb = out_path_ok(pol, path)?;
    let mut items = Vec::new();
    for e in std::fs::read_dir(&pb).map_err(|e| err("not_found", &e.to_string()))? {
        let e = e.map_err(|e| err("internal", &e.to_string()))?;
        let md = e.metadata().map_err(|e| err("internal", &e.to_string()))?;
        items.push(json!({
            "name": e.file_name().to_string_lossy(),
            "dir": md.is_dir(),
            "size": md.len(),
        }));
    }
    items.sort_by(|a, b| {
        let (da, db) = (a["dir"].as_bool(), b["dir"].as_bool());
        db.cmp(&da).then(
            a["name"].as_str().unwrap_or("").cmp(b["name"].as_str().unwrap_or("")),
        )
    });
    Ok(json!({"path": path, "items": items}))
}

fn files_read(pol: &Policy, args: &Value) -> Result<Value, (String, String)> {
    let path = args.get("path").and_then(|v| v.as_str()).ok_or(err("invalid", "path required"))?;
    let pb = out_path_ok(pol, path)?;
    let data = std::fs::read(&pb).map_err(|e| err("not_found", &e.to_string()))?;
    if data.len() > pol.max_output_bytes {
        return Err(err("blocked", &format!("file exceeds max output ({} > {})", data.len(), pol.max_output_bytes)));
    }
    // redact obvious secrets in file content
    let text = String::from_utf8_lossy(&data).to_string();
    Ok(json!({"path": path, "content": redact(&json!(text))}))
}

fn files_search(pol: &Policy, args: &Value) -> Result<Value, (String, String)> {
    let root = args.get("path").and_then(|v| v.as_str()).ok_or(err("invalid", "path required"))?;
    let query = args.get("query").and_then(|v| v.as_str()).ok_or(err("invalid", "query required"))?;
    let pb = out_path_ok(pol, root)?;
    let mut hits = Vec::new();
    search_dir(&pb, query, pol, &mut hits, 0)?;
    Ok(json!({"matches": hits}))
}

fn search_dir(dir: &PathBuf, q: &str, pol: &Policy, hits: &mut Vec<Value>, depth: usize) -> Result<(), (String, String)> {
    if depth > 6 || hits.len() > 50 { return Ok(()); }
    for e in std::fs::read_dir(dir).map_err(|e| err("not_found", &e.to_string()))?.flatten() {
        let p = e.path();
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if name.starts_with('.') || name == "node_modules" || name == "target" { continue; }
        if pol.path_blocked(&p.to_string_lossy()) { continue; }
        if p.is_dir() {
            search_dir(&p, q, pol, hits, depth + 1)?;
        } else if p.is_file() {
            if let Ok(content) = std::fs::read_to_string(&p) {
                if content.contains(q) {
                    hits.push(json!({"file": p.to_string_lossy()}));
                    if hits.len() >= 50 { return Ok(()); }
                }
            }
        }
    }
    Ok(())
}

fn files_write(pol: &Policy, args: &Value) -> Result<Value, (String, String)> {
    let path = args.get("path").and_then(|v| v.as_str()).ok_or(err("invalid", "path required"))?;
    let content = args.get("content").and_then(|v| v.as_str()).ok_or(err("invalid", "content required"))?;
    let pb = out_path_ok(pol, path)?;
    std::fs::write(&pb, content).map_err(|e| err("internal", &e.to_string()))?;
    Ok(json!({"path": path, "bytes": content.len()}))
}

// ---------- shell ----------

static SESSIONS: Mutex<Option<HashMap<String, Session>>> = Mutex::new(None);

struct Session {
    child: std::process::Child,
    out: Arc<Mutex<Vec<u8>>>,
    cwd: PathBuf,
}

impl Session {
    fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

fn spawn_session(cwd: Option<PathBuf>) -> Session {
    let mut cmd = Command::new("bash");
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0); // own pgroup: session kill hits the whole tree
    if let Some(ref d) = cwd { cmd.current_dir(d); }
    let mut child = cmd.spawn().expect("spawn bash");
    let buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let stdout = child.stdout.take().expect("stdout");
    let stderr = child.stderr.take().expect("stderr");
    for fd in [
        Box::new(stdout) as Box<dyn Read + Send>,
        Box::new(stderr) as Box<dyn Read + Send>,
    ] {
        let sink = Arc::clone(&buf);
        std::thread::spawn(move || {
            let mut src = fd;
            let mut chunk = [0u8; 8192];
            loop {
                match src.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => sink.lock().unwrap().extend_from_slice(&chunk[..n]),
                }
            }
        });
    }
    let mut s = Session { child, out: buf, cwd: cwd.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default()) };
    let _ = s.send("export PS1=''; export TERM=dumb; echo __READY__");
    std::thread::sleep(std::time::Duration::from_millis(150));
    let _ = s.read_out();
    s
}

impl Session {
    fn send(&mut self, line: &str) -> bool {
        let stdin = self.child.stdin.as_mut().expect("stdin");
        stdin.write_all(format!("{line}\n").as_bytes()).is_ok()
    }
    fn read_out(&self) -> String {
        let mut buf = self.out.lock().unwrap();
        let s = String::from_utf8_lossy(&buf).into_owned();
        buf.clear();
        s
    }
}

/// Kill the whole process group (Stop means STOP).
fn kill_group(pid: u32) {
    unsafe {
        unsafe { libc_kill(-(pid as i32), 15) };
    }
}

#[link(name = "c")]
extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}
#[allow(non_snake_case)]
unsafe fn libc_kill(pid: i32, sig: i32) -> i32 { kill(pid, sig) }

fn shell_exec(pol: &Policy, args: &Value, timeout_ms: u64) -> Result<Value, (String, String)> {
    let cmd = args.get("cmd").and_then(|v| v.as_str()).ok_or(err("invalid", "cmd required"))?;
    if pol.command_blocked(cmd) {
        return Err(err("blocked", &format!("command matches blocked list")));
    }
    // sandbox the cwd to an allowed folder if given
    let cwd = args.get("cwd").and_then(|v| v.as_str());
    let mut command = Command::new("bash");
    command.arg("-c").arg(cmd);
    if let Some(c) = cwd {
        if !pol.path_allowed(c) {
            return Err(err("denied", &format!("cwd '{c}' outside allowed folders")));
        }
        command.current_dir(c);
    }
    let started = std::time::Instant::now();
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0) // own pgroup: timeout kill can't touch the daemon
        .spawn()
        .map_err(|e| err("internal", &e.to_string()))?;

    let pid = child.id();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = Vec::new();
                let mut stderr = Vec::new();
                let _ = child.stdout.take().unwrap().read_to_end(&mut stdout);
                let _ = child.stderr.take().unwrap().read_to_end(&mut stderr);
                let out_s = String::from_utf8_lossy(&stdout);
                let err_s = String::from_utf8_lossy(&stderr);
                let mut out_json = json!({
                    "exit_code": status.code().unwrap_or(-1),
                    "stdout": redact(&json!(truncate(&out_s, pol.max_output_bytes))),
                    "stderr": redact(&json!(truncate(&err_s, pol.max_output_bytes))),
                });
                out_json["duration_ms"] = json!(started.elapsed().as_millis() as u64);
                return Ok(out_json);
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = unsafe { libc_kill(-(pid as i32), 9) }; // SIGKILL whole group
                    let _ = child.wait(); // reap so no zombie
                    return Err(("timeout".into(), format!("killed after {timeout_ms}ms (pid {pid})")));
                }
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
            Err(e) => return Err(err("internal", &e.to_string())),
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() } else { format!("{}…[truncated {}B]", &s[..max], s.len() - max) }
}

fn session_create(pol: &Policy, args: &Value) -> Result<Value, (String, String)> {
    let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("default").to_string();
    let cwd = args.get("cwd").and_then(|v| v.as_str());
    if let Some(c) = cwd {
        if !pol.path_allowed(c) {
            return Err(err("denied", &format!("cwd '{c}' outside allowed folders")));
        }
    }
    let s = spawn_session(cwd.map(PathBuf::from));
    let mut map = SESSIONS.lock().unwrap();
    let map = map.get_or_insert_with(HashMap::new);
    map.insert(name.clone(), s);
    Ok(json!({"session": name}))
}

fn session_send(pol: &Policy, args: &Value) -> Result<Value, (String, String)> {
    let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("default");
    let line = args.get("line").and_then(|v| v.as_str()).ok_or(err("invalid", "line required"))?;
    if pol.command_blocked(line) {
        return Err(err("blocked", &format!("command matches blocked list")));
    }
    let mut map = SESSIONS.lock().unwrap();
    let map = map.get_or_insert_with(HashMap::new);
    let s = map.get_mut(name).ok_or(err("not_found", &format!("no session '{name}'")))?;
    if !s.alive() {
        return Err(err("cancelled", "session dead"));
    }
    if !s.send(line) {
        return Err(err("internal", "stdin write failed"));
    }
    Ok(json!({"sent": true}))
}

fn session_read(pol: &Policy, _args: &Value) -> Result<Value, (String, String)> {
    let name = _args.get("name").and_then(|v| v.as_str()).unwrap_or("default");
    let mut map = SESSIONS.lock().unwrap();
    let map = map.get_or_insert_with(HashMap::new);
    let s = map.get(name).ok_or(err("not_found", &format!("no session '{name}'")))?;
    Ok(json!({"output": s.read_out()}))
}

fn session_kill(pol: &Policy, args: &Value) -> Result<Value, (String, String)> {
    let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("default");
    let mut map = SESSIONS.lock().unwrap();
    let map = map.get_or_insert_with(HashMap::new);
    let s = map.get_mut(name).ok_or(err("not_found", &format!("no session '{name}'")))?;
    let pid = s.child.id();
    let _ = unsafe { libc_kill(-(pid as i32), 15) };
    let _ = s.child.wait(); // reap
    map.remove(name);
    Ok(json!({"killed": name, "pid": pid}))
}

// ---------- windows (X11) ----------

fn windows_list(pol: &Policy) -> Result<Value, (String, String)> {
    let o = Command::new("wmctrl").arg("-l").output().map_err(|e| err("internal", &e.to_string()))?;
    let text = String::from_utf8_lossy(&o.stdout);
    let mut wins = Vec::new();
    for line in text.lines() {
        let mut it = line.splitn(4, char::is_whitespace);
        let wid = it.next().unwrap_or("").to_string();
        let _desk = it.next(); let _host = it.next();
        let title = it.next().unwrap_or("").to_string();
        wins.push(json!({"id": wid, "title": title}));
    }
    let _ = pol;
    Ok(json!({"windows": wins}))
}

fn windows_focus(pol: &Policy, args: &Value) -> Result<Value, (String, String)> {
    let id = args.get("id").and_then(|v| v.as_str()).ok_or(err("invalid", "id required"))?;
    let o = Command::new("wmctrl").args(["-i", "-a", id]).output().map_err(|e| err("internal", &e.to_string()))?;
    Ok(json!({"focused": id, "ok": o.status.success()}))
}

fn windows_click(pol: &Policy, args: &Value) -> Result<Value, (String, String)> {
    let x = args.get("x").and_then(|v| v.as_i64()).ok_or(err("invalid", "x required"))?;
    let y = args.get("y").and_then(|v| v.as_i64()).ok_or(err("invalid", "y required"))?;
    let o = Command::new("xdotool").args(["mousemove", &x.to_string(), &y.to_string(), "click", "1"])
        .output().map_err(|e| err("internal", &e.to_string()))?;
    Ok(json!({"clicked": [x, y], "ok": o.status.success()}))
}

fn windows_type(pol: &Policy, args: &Value) -> Result<Value, (String, String)> {
    let text = args.get("text").and_then(|v| v.as_str()).ok_or(err("invalid", "text required"))?;
    let o = Command::new("xdotool").args(["type", "--", text])
        .output().map_err(|e| err("internal", &e.to_string()))?;
    Ok(json!({"typed": text.len(), "ok": o.status.success()}))
}

fn windows_key(pol: &Policy, args: &Value) -> Result<Value, (String, String)> {
    let key = args.get("key").and_then(|v| v.as_str()).ok_or(err("invalid", "key required"))?;
    let o = Command::new("xdotool").args(["key", key])
        .output().map_err(|e| err("internal", &e.to_string()))?;
    Ok(json!({"key": key, "ok": o.status.success()}))
}

// ---------- screen / clipboard / notifications ----------

fn screen_snap(pol: &Policy, args: &Value) -> Result<Value, (String, String)> {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("/tmp/muse-screen.png").to_string();
    let pb = PathBuf::from(&path);
    if let Some(dir) = pb.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let o = Command::new("scrot").arg("-z").arg(&path)
        .output().map_err(|e| err("internal", &e.to_string()))?;
    if !o.status.success() {
        return Err(err("internal", "scrot failed"));
    }
    Ok(json!({"path": path, "bytes": std::fs::metadata(&pb).map(|m| m.len()).unwrap_or(0)}))
}

fn clipboard_read(pol: &Policy) -> Result<Value, (String, String)> {
    let mut child = Command::new("xclip").args(["-selection", "clipboard", "-o"])
        .stdout(Stdio::piped()).spawn().map_err(|e| err("internal", &e.to_string()))?;
    let mut out = String::new();
    let _ = child.stdout.take().unwrap().read_to_string(&mut out);
    Ok(json!({"content": redact(&json!(out))}))
}

fn clipboard_write(pol: &Policy, args: &Value) -> Result<Value, (String, String)> {
    let text = args.get("text").and_then(|v| v.as_str()).ok_or(err("invalid", "text required"))?;
    let mut child = Command::new("xclip").args(["-selection", "clipboard"])
        .stdin(Stdio::piped()).spawn().map_err(|e| err("internal", &e.to_string()))?;
    let _ = child.stdin.take().unwrap().write_all(text.as_bytes());
    let _ = child.wait();
    Ok(json!({"written": text.len()}))
}

fn notify_send(pol: &Policy, args: &Value) -> Result<Value, (String, String)> {
    let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("Muse");
    let body = args.get("body").and_then(|v| v.as_str()).unwrap_or("");
    let o = Command::new("notify-send").args([title, body])
        .output().map_err(|e| err("internal", &e.to_string()))?;
    Ok(json!({"ok": o.status.success()}))
}
