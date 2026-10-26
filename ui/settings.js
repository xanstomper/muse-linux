// Muse Linux — desktop settings panel (baked-in look, matches muse.ai dark UI).
// Sections: Device (pairing/machine), Appearance (zoom), Linux (input/compositor),
// Advanced (yolo mode, audit log). Opens with Ctrl+, or the tray/menu shortcut.
(function () {
  if (window.__museSettings) return;

  const { invoke } = window.__TAURI_INTERNALS__;

  const I = {
    gear: `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/></svg>`,
    x: `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>`,
    chip: `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="4" width="16" height="16" rx="2"/><rect x="9" y="9" width="6" height="6"/><path d="M15 2v2"/><path d="M15 20v2"/><path d="M2 15h2"/><path d="M2 9h2"/><path d="M20 15h2"/><path d="M20 9h2"/><path d="M9 2v2"/><path d="M9 20v2"/></svg>`,
    type: `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 7 4 4 20 4 20 7"/><line x1="9" x2="15" y1="20" y2="20"/><line x1="12" x2="12" y1="4" y2="20"/></svg>`,
    eye: `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z"/><circle cx="12" cy="12" r="3"/></svg>`,
    check: `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>`,
  };

  const CSS = `
#mus-root { position: fixed; inset: 0; z-index: 999996; display: none;
  background: rgba(5,7,12,.66); backdrop-filter: blur(6px);
  font-family: system-ui, -apple-system, "Segoe UI", sans-serif;
  -webkit-user-select: none; user-select: none; }
#mus-root.open { display: flex; align-items: center; justify-content: center; }
#mus-card { width: min(560px, 94vw); max-height: 88vh; overflow-y: auto;
  background: linear-gradient(180deg, #181b23 0%, #141721 100%); color: #e9ecf3;
  border: 1px solid rgba(255,255,255,.08); border-radius: 16px;
  box-shadow: 0 24px 80px rgba(0,0,0,.6), 0 0 0 1px rgba(0,0,0,.4); }
.mus-head { display: flex; align-items: center; gap: 12px; padding: 18px 20px 14px;
  border-bottom: 1px solid rgba(255,255,255,.06); position: sticky; top: 0;
  background: inherit; z-index: 2; }
.mus-head h2 { margin: 0; font-size: 15.5px; font-weight: 700; letter-spacing: -.01em; }
.mus-head .sub { margin: 2px 0 0; font-size: 12px; color: #8b93a5; }
.mus-close { margin-left: auto; width: 30px; height: 30px; border-radius: 8px; border: 0;
  background: rgba(255,255,255,.06); color: #8b93a5; cursor: pointer; display: flex;
  align-items: center; justify-content: center; }
.mus-close:hover { background: rgba(255,255,255,.12); color: #e9ecf3; }
.mus-body { padding: 8px 20px 20px; }
.mus-section { padding: 14px 0; border-bottom: 1px solid rgba(255,255,255,.05); }
.mus-section:last-child { border-bottom: 0; }
.mus-sec-title { display: flex; align-items: center; gap: 8px; font-size: 11px;
  font-weight: 700; letter-spacing: .08em; text-transform: uppercase; color: #8b93a5;
  margin-bottom: 4px; }
.mus-sec-title svg { color: #4da3ff; }
.mus-row { display: flex; align-items: center; justify-content: space-between;
  gap: 14px; padding: 9px 0; }
.mus-row .l { font-size: 13px; font-weight: 600; }
.mus-row .d { font-size: 11.5px; color: #8b93a5; margin-top: 2px; line-height: 1.45; }
.mus-toggle { position: relative; width: 38px; height: 22px; flex-shrink: 0;
  border-radius: 999px; border: 1px solid rgba(255,255,255,.14); cursor: pointer;
  background: #0d0f15; transition: background .15s, border-color .15s; }
.mus-toggle::after { content: ""; position: absolute; top: 2px; left: 2px;
  width: 16px; height: 16px; border-radius: 50%; background: #8b93a5; transition: transform .15s, background .15s; }
.mus-toggle.on { background: rgba(0,130,251,.35); border-color: #0082fb; }
.mus-toggle.on::after { transform: translateX(16px); background: #4da3ff; }
.mus-btn { padding: 8px 14px; border-radius: 8px; border: 0; cursor: pointer;
  background: rgba(0,130,251,.16); color: #4da3ff; font-size: 12.5px; font-weight: 600;
  display: inline-flex; align-items: center; gap: 7px; }
.mus-btn:hover { background: rgba(0,130,251,.26); }
.mus-btn.ghost { background: rgba(255,255,255,.07); color: #c6cbd8; }
.mus-btn.ghost:hover { background: rgba(255,255,255,.13); }
.mus-zoom { display: flex; align-items: center; gap: 10px; }
.mus-zoom button { width: 28px; height: 28px; border-radius: 7px; border: 1px solid rgba(255,255,255,.13);
  background: #0d0f15; color: #e9ecf3; font-size: 15px; cursor: pointer; font-weight: 700; }
.mus-zoom button:hover { border-color: #0082fb; }
.mus-zoomval { font-size: 12.5px; color: #c6cbd8; min-width: 44px; text-align: center;
  font-variant-numeric: tabular-nums; }
.mus-devstat { display: flex; align-items: center; gap: 8px; font-size: 12.5px; }
.mus-devstat .dot { width: 8px; height: 8px; border-radius: 50%; }
.mus-audit { display: none; margin-top: 8px; padding: 10px 12px; border-radius: 9px;
  background: #0a0c11; border: 1px solid rgba(255,255,255,.07);
  font-family: ui-monospace, "SF Mono", Menlo, monospace; font-size: 11px;
  color: #9fe8b9; max-height: 160px; overflow-y: auto; white-space: pre-wrap;
  -webkit-user-select: text; user-select: text; }
.mus-audit.show { display: block; }
`;

  let els = null;

  function ensureDom() {
    if (els) return;
    const root = document.createElement("div");
    root.id = "mus-root";
    root.innerHTML = `
      <div id="mus-card">
        <div class="mus-head">
          <div>
            <h2>Settings</h2>
            <p class="sub">Muse Desktop for Linux</p>
          </div>
          <button class="mus-close" title="Close">${I.x}</button>
        </div>
        <div class="mus-body">
          <div class="mus-section">
            <div class="mus-sec-title">${I.chip} Device</div>
            <div class="mus-row">
              <div>
                <div class="l">Paired machine</div>
                <div class="d">Run commands, read files and control windows on your Linux device — straight from chat.</div>
                <div class="mus-devstat" id="mus-devstat" style="margin-top:8px"></div>
              </div>
              <button class="mus-btn" id="mus-manage">Manage</button>
            </div>
          </div>
          <div class="mus-section">
            <div class="mus-sec-title">${I.eye} Appearance</div>
            <div class="mus-row">
              <div><div class="l">Text size</div><div class="d">Ctrl + scroll anywhere also zooms.</div></div>
              <div class="mus-zoom">
                <button id="mus-zout" title="Smaller">−</button>
                <span class="mus-zoomval" id="mus-zoomval">100%</span>
                <button id="mus-zin" title="Larger">+</button>
              </div>
            </div>
          </div>
          <div class="mus-section">
            <div class="mus-sec-title">${I.type} Linux</div>
            <div class="mus-row">
              <div><div class="l">Native input method</div><div class="d">Simple IM module (ibus bypass) — typing works reliably in composer and dialogs.</div></div>
              <span class="mus-devstat" style="color:#3ecf6f">${I.check} on</span>
            </div>
            <div class="mus-row">
              <div><div class="l">Hardware acceleration</div><div class="d">WebKitGTK compositing/DMABUF disabled — prevents blank screens on NVIDIA systems.</div></div>
              <span class="mus-devstat" style="color:#8b93a5">off</span>
            </div>
            <div class="mus-row">
              <div><div class="l">System notifications</div><div class="d">Desktop notifications via the portal daemon (libnotify).</div></div>
              <div class="mus-toggle" id="mus-notify-tgl"></div>
            </div>
            <div class="mus-row">
              <div><div class="l">Keep in system tray</div><div class="d">Closing the window keeps Muse running in the tray. Disable to quit fully on close.</div></div>
              <div class="mus-toggle on" id="mus-tray-tgl"></div>
            </div>
            <div class="mus-row">
              <div><div class="l">Keyboard shortcuts</div><div class="d">Ctrl+L new chat · Ctrl+F find · Ctrl+, settings · Ctrl+scroll zoom · Ctrl+Shift+D connect device</div></div>
              <span class="mus-devstat" style="color:#8b93a5">—</span>
            </div>
          </div>
          <div class="mus-section">
            <div class="mus-sec-title">${I.gear} Advanced</div>
            <div class="mus-row">
              <div><div class="l">Autonomous mode (yolo)</div><div class="d">Skip per-command approval prompts on the paired device. Only enable if you trust every chat instruction.</div></div>
              <div class="mus-toggle" id="mus-yolo"></div>
            </div>
            <div class="mus-row">
              <div><div class="l">Audit log</div><div class="d">Last actions taken on your device.</div></div>
              <button class="mus-btn ghost" id="mus-auditbtn">View</button>
            </div>
            <div class="mus-audit" id="mus-audit"></div>
          </div>
        </div>
      </div>`;
    (document.body || document.documentElement).appendChild(root);
    const style = document.createElement("style");
    style.textContent = CSS;
    (document.head || document.documentElement).appendChild(style);


    root.addEventListener("click", (e) => { if (e.target === root) close(); });
    root.querySelector(".mus-close").addEventListener("click", close);

    els = {
      root,
      devstat: root.querySelector("#mus-devstat"),
      zoomval: root.querySelector("#mus-zoomval"),
      yolo: root.querySelector("#mus-yolo"),
      audit: root.querySelector("#mus-audit"),
    };
    root.querySelector("#mus-manage").addEventListener("click", () => {
      close();
      window.__museConnect?.open?.();
    });
    root.querySelector("#mus-zin").addEventListener("click", () => zoomStep(0.1));
    root.querySelector("#mus-zout").addEventListener("click", () => zoomStep(-0.1));
    els.yolo.addEventListener("click", toggleYolo);
    root.querySelector("#mus-auditbtn").addEventListener("click", toggleAudit);
    const nt = root.querySelector("#mus-notify-tgl");
    nt.classList.toggle("on", localStorage.getItem("mus-notify") !== "0");
    nt.addEventListener("click", () => {
      const on = !nt.classList.contains("on");
      nt.classList.toggle("on", on);
      localStorage.setItem("mus-notify", on ? "1" : "0");
      try { invoke("set_notify_enabled", { on }); } catch {}
    });
    const tt = root.querySelector("#mus-tray-tgl");
    tt.classList.toggle("on", localStorage.getItem("mus-tray") !== "0");
    tt.addEventListener("click", () => {
      const on = !tt.classList.contains("on");
      tt.classList.toggle("on", on);
      localStorage.setItem("mus-tray", on ? "1" : "0");
    });
  }

  function zoomStep(d) {
    window.__museDesktop?.zoom?.(d);
    const cur = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
    els.zoomval.textContent = Math.round((cur / 16) * 100) + "%";
  }

  async function toggleYolo() {
    const on = !els.yolo.classList.contains("on");
    els.yolo.classList.toggle("on", on);
    try { await invoke("set_yolo", { on }); } catch (e) { els.yolo.classList.remove("on"); }
  }

  async function toggleAudit() {
    if (els.audit.classList.contains("show")) { els.audit.classList.remove("show"); return; }
    els.audit.classList.add("show");
    els.audit.textContent = "loading…";
    try {
      const t = await invoke("audit_tail");
      const lines = String(t).split("\n").filter(Boolean);
      els.audit.textContent = lines.slice(-40).join("\n") || "(empty)";
    } catch (e) { els.audit.textContent = String(e); }
  }

  async function refresh() {
    ensureDom();
    // device status
    try {
      const s = await invoke("daemon_status");
      const dot = s.reachable ? "#3ecf6f" : s.paired ? "#ffb44d" : "#5b6272";
      const txt = s.paired ? (s.reachable ? `Connected — ${s.label || s.url}` : "Paired — device offline") : "No device paired";
      els.devstat.innerHTML = `<span class="dot" style="background:${dot}"></span><span>${txt}</span>`;
      els.yolo.classList.toggle("on", !!s.yolo);
    } catch {}
    // zoom value
    const cur = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
    els.zoomval.textContent = Math.round((cur / 16) * 100) + "%";
  }

  function open() { ensureDom(); els.root.classList.add("open"); refresh(); }
  function close() { els?.root.classList.remove("open"); }

  window.addEventListener("keydown", (e) => {
    if (e.ctrlKey && !e.shiftKey && e.key === ",") { e.preventDefault(); open(); }
    if (e.key === "Escape" && els?.root.classList.contains("open")) close();
  });

  // ---- Baked-in integration: inject "Muse Desktop" into muse.ai's own settings menu ----
  // When any popover/menu containing a Settings entry opens, append a row that matches its
  // style and opens our panel.
  function wireMenuIntegration() {
    if (wireMenuIntegration.done) return;
    wireMenuIntegration.done = true;
    const obs = new MutationObserver(() => {
      if (!document.body) return;
      // muse.ai uses [role=menu]/[data-slot=popover-content]/div overlays
      const menus = document.querySelectorAll(
        '[role="menu"], [role="dialog"], [data-slot="popover-content"], [data-radix-popper-content-wrapper]'
      );
      menus.forEach((menu) => {
        if (menu.querySelector("#mus-menu-item") || menu.dataset.musWired) return;
        const items = [...menu.querySelectorAll('[role^="menuitem"], button, a, [data-radix-collection-item]')];
        const anchor = items.find((el) => {
          if (el.id === "mus-menu-item") return false;
          const t = (el.textContent || el.getAttribute("aria-label") || "").trim().toLowerCase();
          if (/mobile|download|app\b|desktop settings|linux settings/.test(t)) return false;
          return t === "settings" || t === "preferences" ||
                 (t.length < 40 && /^[\s·•]*settings\b/.test(t));
        });
        if (!anchor) return;
        menu.dataset.musWired = "1";
        const row = anchor.cloneNode(true);
        row.id = "mus-menu-item";
        row.removeAttribute("href");
        row.setAttribute("aria-label", "Linux settings");
        row.setAttribute("data-pel-click", "linux_settings_click");
        // wipe every text node inside the clone, then set one clean label
        const walk = document.createTreeWalker(row, NodeFilter.SHOW_TEXT);
        while (walk.nextNode()) walk.currentNode.textContent = "";
        const first = row.querySelector("span,div,p") || row;
        first.textContent = "Linux settings";
        row.addEventListener("click", (e) => {
          e.preventDefault(); e.stopPropagation();
          open();
          // close their menu
          document.body.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
          document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
        });
        anchor.parentElement.insertBefore(row, anchor.nextSibling);
      });
    });
    obs.observe(document.documentElement, { childList: true, subtree: true });
  }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", wireMenuIntegration);
  } else {
    wireMenuIntegration();
  }

  window.__museSettings = { open, close, refresh };
  console.info("[MuseLinux] settings panel ready (Ctrl+, and baked into muse.ai settings menu)");
})();
