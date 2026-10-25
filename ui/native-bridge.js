// Muse Linux — native bridge injected into the Muse webview.
// Exposes window.__museNative so the page (or user JS) can use real system tools.
(function () {
  if (window.__museNative) return;

  const { invoke } = window.__TAURI_INTERNALS__;

  const native = {
    // one-shot commands
    exec: (cmd, opts) => invoke("sys_exec", { cmd, cwd: opts?.cwd, timeoutSecs: opts?.timeoutSecs, approval: opts?.approval }),
    // persistent shell (state, env, cwd survive between calls)
    term: (cmd, opts) => invoke("term_exec", { cmd, cwd: opts?.cwd, timeoutMs: opts?.timeoutMs, approval: opts?.approval }),
    termReset: () => invoke("term_reset"),
    // filesystem
    list: (path) => invoke("fs_list", { path }),
    read: (path, maxBytes) => invoke("fs_read", { path, maxBytes }),
    write: (path, content) => invoke("fs_write", { path, content }),
    search: (root, query) => invoke("fs_search", { root, query }),
    patch: (cwd, diff) => invoke("apply_patch", { cwd, diff }),
    // system
    procs: () => invoke("procs"),
    kill: (pid) => invoke("sys_kill", { pid }),
    open: (path) => invoke("open_path", { path }),
    git: (repo) => invoke("git_status", { repo }),
    notify: (title, body) => invoke("notify", { title, body }),
    audit: () => invoke("audit_tail"),
    yolo: (on) => invoke("set_yolo", { on }),
  };

  window.__museNative = native;

  console.info("[MuseLinux] native bridge ready: window.__museNative (exec/term/fs/git/patch/procs)");
})();
