use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

pub static YOLO: AtomicBool = AtomicBool::new(false);

fn audit_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let d = PathBuf::from(home).join(".local/share/muse-linux");
    let _ = fs::create_dir_all(&d);
    d
}

pub fn audit(action: &str, detail: &str) {
    let line = format!(
        "{{\"ts\":\"{}\",\"action\":\"{}\",\"detail\":{}}}\n",
        chrono_now(),
        action,
        serde_json::to_string(detail).unwrap_or_else(|_| "\"?\"".into())
    );
    if let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(audit_dir().join("audit.log"))
    {
        let _ = f.write_all(line.as_bytes());
    }
}

fn chrono_now() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{t}")
}

#[derive(Serialize)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
    pub timed_out: bool,
}

#[tauri::command]
pub fn sys_exec(
    app: tauri::AppHandle,
    cmd: String,
    cwd: Option<String>,
    timeout_secs: Option<u64>,
    approval: Option<bool>,
) -> Result<ExecResult, String> {
    if approval.unwrap_or(true) && !YOLO.load(Ordering::Relaxed) {
        use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
        let ok = app
            .dialog()
            .message(format!("Run this command?\n\n{cmd}"))
            .title("Muse Linux — System Tool Approval")
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Run".into(),
                "Cancel".into(),
            ))
            .blocking_show();
        if !ok {
            audit("exec_denied", &cmd);
            return Err("denied by user".into());
        }
    }
    audit("exec", &cmd);
    let timeout = Duration::from_secs(timeout_secs.unwrap_or(30).min(600));
    let mut child = Command::new("bash")
        .arg("-lc")
        .arg(&cmd)
        .current_dir(cwd.unwrap_or_else(|| std::env::var("HOME").unwrap_or_else(|_| "/".into())))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut out = String::new();
                let mut err = String::new();
                if let Some(mut o) = child.stdout.take() {
                    let _ = o.read_to_string(&mut out);
                }
                if let Some(mut e) = child.stderr.take() {
                    let _ = e.read_to_string(&mut err);
                }
                let _ = out.pop();
                let _ = err.pop();
                return Ok(ExecResult {
                    stdout: out,
                    stderr: err,
                    code: status.code(),
                    timed_out: false,
                });
            }
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(ExecResult {
                    stdout: String::new(),
                    stderr: "timed out".into(),
                    code: None,
                    timed_out: true,
                });
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

#[derive(Serialize)]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub dir: bool,
    pub size: u64,
}

#[tauri::command]
pub fn fs_list(path: Option<String>) -> Result<Vec<Entry>, String> {
    let p =
        PathBuf::from(path.unwrap_or_else(|| std::env::var("HOME").unwrap_or_else(|_| "/".into())));
    let mut out = Vec::new();
    for e in fs::read_dir(&p).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let md = e.metadata().map_err(|e| e.to_string())?;
        out.push(Entry {
            name: e.file_name().to_string_lossy().into_owned(),
            path: e.path().to_string_lossy().into_owned(),
            dir: md.is_dir(),
            size: md.len(),
        });
    }
    out.sort_by(|a, b| b.dir.cmp(&a.dir).then(a.name.cmp(&b.name)));
    Ok(out)
}

#[tauri::command]
pub fn fs_read(path: String, max_bytes: Option<u64>) -> Result<String, String> {
    let cap = max_bytes.unwrap_or(512 * 1024);
    let mut f = fs::File::open(&path).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    f.take(cap)
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

#[tauri::command]
pub fn fs_write(path: String, content: String) -> Result<(), String> {
    audit("fs_write", &path);
    fs::write(&path, content).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct SearchHit {
    pub path: String,
    pub line_no: usize,
    pub line: String,
}

#[tauri::command]
pub fn fs_search(root: String, query: String) -> Result<Vec<SearchHit>, String> {
    use walkdir::WalkDir;
    let q = query.to_lowercase();
    let mut hits = Vec::new();
    for entry in WalkDir::new(&root)
        .max_depth(6)
        .into_iter()
        .filter_entry(|e| {
            let n = e.file_name().to_string_lossy();
            !(n.starts_with('.') || n == "node_modules" || n == "target")
        })
    {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        if entry
            .metadata()
            .map(|m| m.len() > 2 * 1024 * 1024)
            .unwrap_or(true)
        {
            continue;
        }
        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        for (i, line) in text.lines().enumerate() {
            if line.to_lowercase().contains(&q) {
                hits.push(SearchHit {
                    path: entry.path().to_string_lossy().into_owned(),
                    line_no: i + 1,
                    line: line.chars().take(200).collect(),
                });
                if hits.len() >= 50 {
                    return Ok(hits);
                }
            }
        }
    }
    Ok(hits)
}

#[derive(Serialize)]
pub struct Proc {
    pub pid: u32,
    pub name: String,
    pub rss_kb: u64,
}

#[tauri::command]
pub fn procs() -> Result<Vec<Proc>, String> {
    let mut out = Vec::new();
    for d in fs::read_dir("/proc").map_err(|e| e.to_string())? {
        let d = d.map_err(|e| e.to_string())?;
        let name = d.file_name().to_string_lossy().into_owned();
        let Ok(pid) = name.parse::<u32>() else {
            continue;
        };
        let statm = fs::read_to_string(d.path().join("statm")).unwrap_or_default();
        let rss_kb = statm
            .split_whitespace()
            .nth(1)
            .and_then(|p| p.parse::<u64>().ok())
            .unwrap_or(0)
            * 4;
        let comm = fs::read_to_string(d.path().join("comm")).unwrap_or_default();
        out.push(Proc {
            pid,
            name: comm.trim().into(),
            rss_kb,
        });
    }
    out.sort_by(|a, b| b.rss_kb.cmp(&a.rss_kb));
    out.truncate(80);
    Ok(out)
}

#[tauri::command]
pub fn sys_kill(pid: u32) -> Result<(), String> {
    audit("kill", &pid.to_string());
    Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn notify(title: String, body: String) -> Result<(), String> {
    notify_rust::Notification::new()
        .summary(&title)
        .body(&body)
        .show()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn push_to_muse(app: tauri::AppHandle, text: String) -> Result<String, String> {
    use arboard::Clipboard;
    use tauri::{Emitter, Manager};
    Clipboard::new()
        .map_err(|e| e.to_string())?
        .set_text(text.clone())
        .map_err(|e| e.to_string())?;
    if let Some(w) = app.get_webview_window("main") {
        w.emit("muse-insert", text.clone())
            .map_err(|e| e.to_string())?;
        let _ = w.show();
        let _ = w.set_focus();
        Ok("inserted+clipboard".into())
    } else {
        Ok("clipboard-only".into())
    }
}

#[tauri::command]
pub fn set_yolo(on: bool) -> Result<(), String> {
    YOLO.store(on, Ordering::Relaxed);
    audit("yolo", if on { "enabled" } else { "disabled" });
    Ok(())
}

#[tauri::command]
pub fn audit_tail() -> Result<String, String> {
    fs::read_to_string(audit_dir().join("audit.log")).or(Ok(String::new()))
}
