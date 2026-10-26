// Policy loading, capability gates, blocked lists, audit log.
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::Value;

const DEFAULT_POLICY: &str = r#"{
  "capabilities": {"files":"read","terminal":"ask","windows":"read","browser":"off","screen":"ask","camera":"off","notifications":"auto","system":"read"},
  "allowed_folders": [],
  "blocked_folders": [".ssh",".gnupg",".password-store"],
  "blocked_apps": ["keepassxc","seahorse"],
  "blocked_commands": ["rm -rf /","rm -rf ~","mkfs","dd if=/dev/zero","shutdown","reboot","poweroff"],
  "require_approval_for": ["files.write","files.move","files.trash","shell.exec","windows.click","windows.type","windows.key","browser.click","browser.type","camera.snap"],
  "audit_log": ".local/share/muse-daemon/audit.log",
  "max_output_bytes": 262144,
  "default_timeout_ms": 15000
}"#;

#[derive(Deserialize, Clone)]
pub struct Policy {
    pub capabilities: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub allowed_folders: Vec<String>,
    #[serde(default)]
    pub blocked_folders: Vec<String>,
    #[serde(default)]
    pub blocked_apps: Vec<String>,
    #[serde(default)]
    pub blocked_commands: Vec<String>,
    #[serde(default)]
    pub require_approval_for: Vec<String>,
    pub audit_log: String,
    #[serde(default)]
    pub max_output_bytes: usize,
    #[serde(default)]
    pub default_timeout_ms: u64,
    // transport.bind e.g. "100.x.y.z:7300"
    #[serde(default)]
    pub transport: Option<Value>,
}

/// Where the boot token persists (0600). The desktop app reads this file.
pub fn token_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config/muse-daemon/token")
}

fn policy_path() -> PathBuf {
    if let Ok(p) = std::env::var("MUSE_DAEMON_POLICY") { return PathBuf::from(p); }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config/muse-daemon/policy.json")
}

impl Policy {
    pub fn load() -> Result<Self, String> {
        let path = policy_path();
        let raw = std::fs::read_to_string(&path)
            .or_else(|_| {
                // fall back to the repo config if no installed policy yet
                std::fs::read_to_string("config/policy.json")
            })
            .map_err(|e| format!("no policy at {} (or ./config/policy.json): {e}", path.display()))?;
        let mut p: Policy = serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?;
        if p.max_output_bytes == 0 { p.max_output_bytes = 262144; }
        if p.default_timeout_ms == 0 { p.default_timeout_ms = 15000; }
        // expand ~ in paths
        let home = std::env::var("HOME").unwrap_or_default();
        for f in p.allowed_folders.iter_mut().chain(p.blocked_folders.iter_mut()) {
            if f.starts_with("~/") { *f = format!("{home}{}", &f[1..]); }
        }
        if p.audit_log.starts_with("~/") { p.audit_log = format!("{home}{}", &p.audit_log[1..]); }
        Ok(p)
    }

    pub fn capability(&self, cap: &str) -> Option<&str> {
        self.capabilities.get(cap).map(|s| s.as_str())
    }

    pub fn capabilities(&self) -> &std::collections::HashMap<String, String> {
        &self.capabilities
    }

    pub fn requires_approval(&self, tool: &str) -> bool {
        self.require_approval_for.iter().any(|t| t == tool)
    }

    /// true if the path is inside any blocked folder
    pub fn path_blocked(&self, path: &str) -> bool {
        let canon = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
        let s = canon.to_string_lossy();
        self.blocked_folders.iter().any(|b| {
            let bc = std::fs::canonicalize(b).unwrap_or_else(|_| PathBuf::from(b));
            s.starts_with(bc.to_string_lossy().as_ref())
        })
    }

    /// true if path is under one of allowed_folders (empty list = allow all non-blocked)
    pub fn path_allowed(&self, path: &str) -> bool {
        if self.path_blocked(path) { return false; }
        if self.allowed_folders.is_empty() { return true; }
        let canon = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
        self.allowed_folders.iter().any(|a| {
            let ac = std::fs::canonicalize(a).unwrap_or_else(|_| PathBuf::from(a));
            canon.starts_with(ac)
        })
    }

    pub fn command_blocked(&self, cmd: &str) -> bool {
        self.blocked_commands.iter().any(|b| cmd.contains(b))
    }

    pub fn app_blocked(&self, app: &str) -> bool {
        let l = app.to_lowercase();
        self.blocked_apps.iter().any(|b| l.contains(b))
    }

    pub fn transport_bind(&self) -> String {
        self.transport
            .as_ref()
            .and_then(|t| t.get("bind"))
            .and_then(|v| v.as_str())
            .unwrap_or("127.0.0.1:7300")
            .to_string()
    }
}

static AUDIT_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn audit(pol: &Policy, tool: &str, args: &Value, status: &str, err: Option<&str>) {
    let path = AUDIT_PATH.get_or_init(|| PathBuf::from(&pol.audit_log));
    if let Some(dir) = path.parent() { let _ = std::fs::create_dir_all(dir); }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let args_redacted = redact(args);
    // epoch \t tool \t status \t args(redacted) \t err
    let line = format!(
        "{now}\t{tool}\t{status}\t{}\t{err_out}\n",
        serde_json::to_string(&args_redacted).unwrap_or_default(),
        err_out = err.unwrap_or(""),
    );
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| f.write_all(line.as_bytes()));
}

/// Never let secrets reach the audit log or tool results.
pub fn redact(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, val)| {
                    let kl = k.to_lowercase();
                    if kl.contains("token") || kl.contains("password") || kl.contains("secret") || kl.contains("key") {
                        (k.clone(), Value::String("***REDACTED***".into()))
                    } else {
                        (k.clone(), redact(val))
                    }
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(redact).collect()),
        other => other.clone(),
    }
}
