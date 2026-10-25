// Muse Linux — native bridge injected into the Muse webview.
// Exposes window.__museNative so the page (or user JS) can use real system tools.
(function () {
  if (window.__museNative) return;

  const { invoke } = window.__TAURI_INTERNALS__;

  const native = {
    exec: (cmd, opts) => invoke("sys_exec", { cmd, cwd: opts?.cwd, timeoutSecs: opts?.timeoutSecs, approval: opts?.approval }),
    list: (path) => invoke("fs_list", { path }),
    read: (path, maxBytes) => invoke("fs_read", { path, maxBytes }),
    write: (path, content) => invoke("fs_write", { path, content }),
    search: (root, query) => invoke("fs_search", { root, query }),
    procs: () => invoke("procs"),
    kill: (pid) => invoke("sys_kill", { pid }),
    open: (path) => invoke("open_path", { path }),
    notify: (title, body) => invoke("notify", { title, body }),
    audit: () => invoke("audit_tail"),
    yolo: (on) => invoke("set_yolo", { on }),
  };

  window.__museNative = native;

  // Log native tool use into the audit trail automatically.
  window.addEventListener("error", (e) => {
    native.notify("Muse Linux error", String(e.message)).catch(() => {});
  });

  console.info("[MuseLinux] native bridge ready: window.__museNative");
})();
