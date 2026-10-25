// Persistent agent terminal — a real shell session living inside the app.
use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::tools::{audit, YOLO};

pub struct Session {
    child: Child,
    out: Arc<Mutex<Vec<u8>>>, // combined stdout+stderr, fed by reader threads
}

static TERM: Mutex<Option<Session>> = Mutex::new(None);

#[derive(Serialize)]
pub struct TermOut {
    pub output: String,
    pub alive: bool,
}

fn spawn(cwd: Option<String>) -> Session {
    let mut cmd = Command::new("bash");
    cmd.args(["--norc", "--noprofile", "-i"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(d) = cwd {
        let _ = cmd.current_dir(d);
    }
    let mut child = cmd.spawn().expect("spawn bash");
    let buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    // Reader threads: pipes are read on their own threads so the caller can
    // poll with a deadline without ever blocking on an empty pipe.
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
    let mut s = Session { child, out: buf };
    let _ = s.send_raw("export PS1=''; export TERM=dumb; cd \"${HOME:-/}\"; echo __READY__");
    let _ = s.drain_until("__READY__", 3000);
    s
}

impl Session {
    fn send_raw(&mut self, line: &str) -> bool {
        let stdin = self.child.stdin.as_mut().expect("stdin");
        stdin.write_all(format!("{line}\n").as_bytes()).is_ok()
    }

    /// Wait until `marker` appears in output or deadline passes.
    fn drain_until(&mut self, marker: &str, wait_ms: u64) -> String {
        let deadline = Instant::now() + Duration::from_millis(wait_ms);
        loop {
            {
                let buf = self.out.lock().unwrap();
                if std::string::String::from_utf8_lossy(&buf).contains(marker) {
                    break;
                }
            }
            if Instant::now() >= deadline || !self.alive() {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let s = String::from_utf8_lossy(&self.out.lock().unwrap()).into_owned();
        self.out.lock().unwrap().clear();
        s
    }

    fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

fn guarded<R: tauri::Runtime>(app: &tauri::AppHandle<R>, action: &str) -> Result<(), String> {
    if !YOLO.load(std::sync::atomic::Ordering::Relaxed) {
        use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
        let ok = app
            .dialog()
            .message(format!("Allow terminal {action}?"))
            .title("Muse Linux — terminal")
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Allow".into(),
                "Deny".into(),
            ))
            .blocking_show();
        if !ok {
            audit("term-deny", action);
            return Err("denied by user".into());
        }
    }
    Ok(())
}

/// Run one command in the persistent shell. Output collected until marker.
#[tauri::command]
pub fn term_exec<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    cmd: String,
    cwd: Option<String>,
    timeout_ms: Option<u64>,
    approval: Option<bool>,
) -> Result<TermOut, String> {
    if approval.unwrap_or(true) && !YOLO.load(std::sync::atomic::Ordering::Relaxed) {
        guarded(&app, &cmd)?;
    }
    audit("term-exec", &cmd);
    let mut guard = TERM.lock().unwrap();
    if guard.is_none() || !guard.as_mut().unwrap().alive() {
        *guard = Some(spawn(cwd.clone()));
    }
    let s = guard.as_mut().unwrap();
    if !s.alive() {
        return Ok(TermOut {
            output: "session dead".into(),
            alive: false,
        });
    }
    s.send_raw(&format!("{cmd}; echo __MUSE_DONE_$?__"));
    let mut out = s.drain_until("__MUSE_DONE_", timeout_ms.unwrap_or(10000));
    let alive = s.alive();
    Ok(TermOut { output: out, alive })
}

/// Kill the persistent shell (next call respawns).
#[tauri::command]
pub fn term_reset<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), String> {
    guarded(&app, "reset session")?;
    audit("term-reset", "");
    if let Some(mut s) = TERM.lock().unwrap().take() {
        let _ = s.child.kill();
    }
    Ok(())
}

#[derive(Serialize)]
pub struct GitStatus {
    pub branch: String,
    pub dirty: Vec<String>,
}

#[tauri::command]
pub fn git_status(repo: String) -> Result<GitStatus, String> {
    let o = |a: &[&str]| -> Result<String, String> {
        let out = Command::new("git")
            .args(a)
            .current_dir(&repo)
            .output()
            .map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let branch = o(&["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();
    let dirty: Vec<String> = o(&["status", "--porcelain"])?
        .lines()
        .take(200)
        .map(|l| l.to_string())
        .collect();
    Ok(GitStatus { branch, dirty })
}

/// Apply a unified-diff patch to files on disk (agent "edit file" primitive).
#[tauri::command]
pub fn apply_patch<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    cwd: String,
    diff: String,
) -> Result<String, String> {
    if approval_enabled(&app, "apply patch") {
        return Err("denied by user".into());
    }
    audit("apply-patch", &format!("{cwd} ({} bytes)", diff.len()));
    let out = Command::new("git")
        .args(["apply", "--whitespace=nowarn", "-"])
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(diff.as_bytes())
        .and_then(|_| Ok(()))
        .map_err(|e| e.to_string());
    out.map(|_| "patched".into()).map_err(|e| e.to_string())
}

fn approval_enabled<R: tauri::Runtime>(app: &tauri::AppHandle<R>, action: &str) -> bool {
    if YOLO.load(std::sync::atomic::Ordering::Relaxed) {
        return false;
    }
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    let ok = app
        .dialog()
        .message(format!("Allow {action}?"))
        .title("Muse Linux")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Allow".into(),
            "Deny".into(),
        ))
        .blocking_show();
    !ok
}
