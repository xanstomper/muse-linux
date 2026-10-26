// Muse Linux — device connect panel.
// Injected with the native bridge. Renders a "Connect your Linux device" card
// into the muse.ai sidebar; talks to the Tauri daemon_* commands to pair a
// muse-daemon endpoint (e.g. http://100.x.y.z:7300 over Tailscale) with a
// token. Once paired, window.__museNative.daemon.call(...) reaches that
// device's shell/files/windows tools.
(function () {
  if (window.__museConnect) return;

  const { invoke } = window.__TAURI_INTERNALS__;

  const CSS = `
#muse-connect-root { position: fixed; inset: 0; z-index: 999998; display: none;
  background: rgba(6,8,12,.62); backdrop-filter: blur(3px);
  font-family: system-ui, -apple-system, sans-serif; }
#muse-connect-root.open { display: flex; align-items: center; justify-content: center; }
#muse-connect-card { width: min(460px, 92vw); background: #16181f; color: #e8eaf0;
  border: 1px solid rgba(255,255,255,.09); border-radius: 14px; padding: 22px;
  box-shadow: 0 18px 60px rgba(0,0,0,.55); }
#muse-connect-card h2 { margin: 0 0 4px; font-size: 16px; font-weight: 650; }
#muse-connect-card .sub { margin: 0 0 14px; font-size: 12.5px; color: #9aa2b1; line-height: 1.45; }
#muse-connect-card label { display: block; font-size: 11.5px; font-weight: 600;
  color: #9aa2b1; margin: 10px 0 4px; text-transform: uppercase; letter-spacing: .04em; }
#muse-connect-card input { width: 100%; box-sizing: border-box; padding: 9px 11px;
  border-radius: 8px; border: 1px solid rgba(255,255,255,.14); background: #0e1015;
  color: #e8eaf0; font-size: 13px; outline: none; }
#muse-connect-card input:focus { border-color: #0082fb; }
#muse-connect-status { font-size: 12.5px; margin-top: 12px; min-height: 18px; line-height: 1.4; }
#muse-connect-status.ok { color: #3ecf6f; }
#muse-connect-status.err { color: #ff6b6b; }
#muse-connect-status.busy { color: #9aa2b1; }
#muse-connect-card .row { display: flex; gap: 8px; margin-top: 16px; }
#muse-connect-card button { flex: 1; padding: 9px 12px; border-radius: 8px; border: 0;
  font-size: 13px; font-weight: 600; cursor: pointer; }
#muse-connect-save { background: #0082fb; color: #fff; }
#muse-connect-save:disabled { opacity: .55; cursor: default; }
#muse-connect-forget { background: rgba(255,255,255,.08); color: #e8eaf0; }
.muse-dev-badge { display: inline-flex; align-items: center; gap: 6px; font-size: 12px;
  padding: 5px 10px; border-radius: 999px; background: rgba(62,207,111,.12);
  color: #3ecf6f; border: 1px solid rgba(62,207,111,.3); margin-top: 10px; }
.muse-dev-badge.disconnected { background: rgba(255,107,107,.1); color: #ff6b6b;
  border-color: rgba(255,107,107,.3); }
`;

  let els = null;

  function ensureDom() {
    if (els) return;
    const root = document.createElement("div");
    root.id = "muse-connect-root";
    root.innerHTML = `
      <div id="muse-connect-card">
        <h2>Connect a Linux device</h2>
        <p class="sub">Give Muse system access — terminal, files, windows — through the
        muse-daemon running on your machine. Transport is your private network
        (e.g. Tailscale <code>100.x.y.z:7300</code>); the token stays on this device.</p>
        <label>Device URL</label>
        <input id="muse-connect-url" placeholder="http://100.93.60.2:7300" spellcheck="false" />
        <label>Daemon token</label>
        <input id="muse-connect-token" placeholder="muse-…" spellcheck="false" />
        <div id="muse-connect-status"></div>
        <div class="row">
          <button id="muse-connect-save">Connect</button>
          <button id="muse-connect-forget">Forget</button>
          <button id="muse-connect-close">Close</button>
        </div>
      </div>`;
    document.body.appendChild(root);
    const style = document.createElement("style");
    style.textContent = CSS;
    document.head.appendChild(style);
    root.addEventListener("click", (e) => {
      if (e.target === root) close();
    });
    els = {
      root,
      url: root.querySelector("#muse-connect-url"),
      token: root.querySelector("#muse-connect-token"),
      status: root.querySelector("#muse-connect-status"),
      save: root.querySelector("#muse-connect-save"),
      forget: root.querySelector("#muse-connect-forget"),
    };
    els.save.addEventListener("click", connect);
    els.forget.addEventListener("click", forget);
    root.querySelector("#muse-connect-close").addEventListener("click", close);
  }

  function setStatus(msg, cls) {
    if (!els) return;
    els.status.textContent = msg;
    els.status.className = cls || "";
  }

  async function refresh() {
    const conn = await invoke("daemon_get_connection");
    if (conn.paired) {
      els.url.value = conn.url;
      els.token.placeholder = "saved (…" + (conn.token_preview || "••••") + ")";
      setStatus(`Paired: ${conn.label || conn.url}`, "ok");
    } else {
      setStatus("No device paired yet.", "");
    }
  }

  async function connect() {
    const url = els.url.value.trim();
    const token = els.token.value.trim();
    if (!url || !token) {
      setStatus("URL and token are both required.", "err");
      return;
    }
    els.save.disabled = true;
    setStatus("Probing device…", "busy");
    try {
      const probe = await invoke("daemon_probe", { url, token });
      if (probe.authorized) {
        await invoke("daemon_connect", { url, token, label: url.replace(/^https?:\/\//, "") });
        await refresh();
        setStatus(`Connected — ${probe.tools} tools available.`, "ok");
        updateBadge();
      } else {
        setStatus(probe.error || "Device reachable but token rejected.", "err");
      }
    } catch (e) {
      setStatus(String(e), "err");
    } finally {
      els.save.disabled = false;
    }
  }

  async function forget() {
    await invoke("daemon_disconnect");
    els.token.value = "";
    els.token.placeholder = "muse-…";
    await refresh();
    updateBadge();
  }

  function close() {
    els.root.classList.remove("open");
  }

  async function open() {
    ensureDom();
    els.root.classList.add("open");
    await refresh();
  }

  // Sidebar status chip so the state is visible without opening the panel.
  async function updateBadge() {
    ensureDom();
    let badge = document.getElementById("muse-dev-badge");
    if (!badge) {
      badge = document.createElement("button");
      badge.id = "muse-dev-badge";
      badge.style.cssText =
        "position:fixed;left:14px;bottom:14px;z-index:999997;display:inline-flex;" +
        "align-items:center;gap:7px;padding:7px 12px;border-radius:999px;border:0;" +
        "background:#16181f;color:#e8eaf0;font-size:12px;font-weight:600;cursor:pointer;" +
        "box-shadow:0 6px 20px rgba(0,0,0,.4);font-family:system-ui,sans-serif;";
      badge.addEventListener("click", open);
      document.body.appendChild(badge);
    }
    let s = {};
    try {
      s = await invoke("daemon_status");
    } catch {}
    const dot = document.createElement("span");
    dot.style.cssText = `width:8px;height:8px;border-radius:50%;background:${
      s.reachable ? "#3ecf6f" : "#ff6b6b"
    };display:inline-block;`;
    badge.textContent = "";
    badge.appendChild(dot);
    badge.appendChild(document.createTextNode(s.paired ? "Device connected" : "Connect device"));
  }

  // Ctrl+Shift+D opens the panel; Esc closes it.
  window.addEventListener("keydown", (e) => {
    if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === "d") {
      e.preventDefault();
      open();
    }
    if (e.key === "Escape" && els?.root.classList.contains("open")) close();
  });

  window.__museConnect = { open, close, refresh: updateBadge };

  // initial paint (page load) + periodic refresh of the status chip
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", updateBadge);
  } else {
    updateBadge();
  }
  setInterval(updateBadge, 15000);
  console.info("[MuseLinux] device connect panel ready (Ctrl+Shift+D or bottom-left chip)");
})();
