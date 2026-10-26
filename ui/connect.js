// Muse Linux — device connect panel (v2 redesign).
// Professional dark sheet with two flows:
//   • Express (default): type the device's Tailscale IP/hostname → app shows a
//     6-digit code → user runs `muse pair <code>` on the device → paired.
//   • Manual (collapsed under "Advanced"): paste URL + token directly.
// Real inline SVG icons; no external assets.
(function () {
  if (window.__museConnect) return;

  const { invoke } = window.__TAURI_INTERNALS__;

  const ICONS = {
    plug: `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 22v-5"/><path d="M9 8V2"/><path d="M15 8V2"/><path d="M18 8v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4V8Z"/></svg>`,
    check: `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>`,
    link: `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>`,
    chevron: `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>`,
    copy: `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>`,
    shield: `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/></svg>`,
    gear: `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/></svg>`,
    x: `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>`,
  };

  const CSS = `
#muse-connect-root { position: fixed; inset: 0; z-index: 999998; display: none;
  background: rgba(5,7,12,.66); backdrop-filter: blur(6px);
  font-family: system-ui, -apple-system, "Segoe UI", sans-serif;
  -webkit-user-select: none; user-select: none; }
#muse-connect-root.open { display: flex; align-items: center; justify-content: center; }
#muse-connect-card { width: min(480px, 94vw); max-height: 88vh; overflow-y: auto;
  background: linear-gradient(180deg, #181b23 0%, #141721 100%); color: #e9ecf3;
  border: 1px solid rgba(255,255,255,.08); border-radius: 16px;
  box-shadow: 0 24px 80px rgba(0,0,0,.6), 0 0 0 1px rgba(0,0,0,.4);
  padding: 0; }
.muc-head { display: flex; align-items: center; gap: 12px; padding: 18px 20px 14px;
  border-bottom: 1px solid rgba(255,255,255,.06); }
.muc-headicon { width: 38px; height: 38px; border-radius: 11px; display: flex;
  align-items: center; justify-content: center; flex-shrink: 0;
  background: linear-gradient(135deg, #0082fb, #0059c9); color: #fff;
  box-shadow: 0 4px 14px rgba(0,130,251,.35); }
.muc-head h2 { margin: 0; font-size: 15.5px; font-weight: 700; letter-spacing: -.01em; }
.muc-head .sub { margin: 2px 0 0; font-size: 12px; color: #8b93a5; }
.muc-close { margin-left: auto; width: 30px; height: 30px; border-radius: 8px; border: 0;
  background: rgba(255,255,255,.06); color: #8b93a5; cursor: pointer; display: flex;
  align-items: center; justify-content: center; }
.muc-close:hover { background: rgba(255,255,255,.12); color: #e9ecf3; }
.muc-body { padding: 16px 20px 20px; }
.muc-step { display: flex; gap: 12px; margin-bottom: 14px; }
.muc-stepnum { width: 22px; height: 22px; border-radius: 50%; flex-shrink: 0;
  background: rgba(0,130,251,.16); color: #4da3ff; font-size: 11.5px; font-weight: 700;
  display: flex; align-items: center; justify-content: center; margin-top: 1px; }
.muc-stepnum.done { background: rgba(62,207,111,.16); color: #3ecf6f; }
.muc-step .t { font-size: 13px; font-weight: 600; margin: 2px 0 6px; }
.muc-step .d { font-size: 12px; color: #8b93a5; line-height: 1.5; }
.muc-input { width: 100%; box-sizing: border-box; padding: 10px 12px; border-radius: 9px;
  border: 1px solid rgba(255,255,255,.13); background: #0d0f15; color: #e9ecf3;
  font-size: 13px; outline: none; -webkit-user-select: text; user-select: text; }
.muc-input:focus { border-color: #0082fb; box-shadow: 0 0 0 3px rgba(0,130,251,.15); }
.muc-codebox { display: none; align-items: center; gap: 10px; margin-top: 10px;
  padding: 12px 14px; border-radius: 10px; background: rgba(0,130,251,.08);
  border: 1px solid rgba(0,130,251,.25); }
.muc-codebox.show { display: flex; }
.muc-code { font-size: 26px; font-weight: 800; letter-spacing: .35em; color: #4da3ff;
  font-variant-numeric: tabular-nums; }
.muc-copybtn { margin-left: auto; display: flex; align-items: center; gap: 6px;
  padding: 6px 11px; border-radius: 7px; border: 0; background: rgba(255,255,255,.08);
  color: #c6cbd8; font-size: 11.5px; font-weight: 600; cursor: pointer; }
.muc-copybtn:hover { background: rgba(255,255,255,.14); }
.muc-term { margin-top: 8px; padding: 10px 12px; border-radius: 9px; background: #0a0c11;
  border: 1px solid rgba(255,255,255,.07); font-family: ui-monospace, "SF Mono", Menlo, monospace;
  font-size: 12px; color: #9fe8b9; white-space: nowrap; overflow-x: auto;
  -webkit-user-select: text; user-select: text; }
.muc-term .p { color: #5b6272; }
.muc-pairbtn { margin-top: 10px; width: 100%; padding: 10px; border-radius: 9px; border: 0;
  background: linear-gradient(180deg, #0a8cfb, #0067d6); color: #fff; font-size: 13.5px;
  font-weight: 650; cursor: pointer; display: flex; align-items: center;
  justify-content: center; gap: 8px; box-shadow: 0 4px 14px rgba(0,130,251,.3); }
.muc-pairbtn:hover { filter: brightness(1.08); }
.muc-pairbtn:disabled { opacity: .55; cursor: default; filter: none; }
.muc-status { font-size: 12.5px; margin-top: 12px; min-height: 18px; line-height: 1.45;
  display: flex; align-items: center; gap: 7px; }
.muc-status.ok { color: #3ecf6f; } .muc-status.err { color: #ff7373; }
.muc-status.busy { color: #8b93a5; }
.muc-spin { width: 13px; height: 13px; border-radius: 50%; flex-shrink: 0;
  border: 2px solid rgba(255,255,255,.15); border-top-color: #4da3ff;
  animation: mucspin .8s linear infinite; display: none; }
.muc-status.busy .muc-spin { display: inline-block; }
@keyframes mucspin { to { transform: rotate(360deg); } }
.muc-adv { margin-top: 14px; border-top: 1px solid rgba(255,255,255,.06); padding-top: 12px; }
.muc-advtoggle { display: flex; align-items: center; gap: 7px; background: none; border: 0;
  color: #8b93a5; font-size: 12px; font-weight: 600; cursor: pointer; padding: 0; }
.muc-advtoggle:hover { color: #c6cbd8; }
.muc-advbody { display: none; margin-top: 12px; }
.muc-advbody.show { display: block; }
.muc-advbody label { display: block; font-size: 11px; font-weight: 650; color: #8b93a5;
  margin: 10px 0 4px; text-transform: uppercase; letter-spacing: .05em; }
.muc-advrow { display: flex; gap: 8px; margin-top: 12px; }
.muc-advrow button { flex: 1; padding: 8px 12px; border-radius: 8px; border: 0;
  font-size: 12.5px; font-weight: 600; cursor: pointer; }
#muc-manual-save { background: rgba(0,130,251,.16); color: #4da3ff;
  border: 1px solid rgba(0,130,251,.3); }
#muc-manual-save:hover { background: rgba(0,130,251,.26); }
#muc-forget { background: rgba(255,115,115,.1); color: #ff7373;
  border: 1px solid rgba(255,115,115,.25); }
.muc-foot { display: flex; align-items: center; gap: 7px; padding: 10px 20px;
  border-top: 1px solid rgba(255,255,255,.06); font-size: 11px; color: #5b6272; }
.muc-foot svg { color: #3ecf6f; }
.muc-badge { position: fixed; left: 14px; bottom: 14px; z-index: 999997;
  display: inline-flex; align-items: center; gap: 7px; padding: 7px 13px;
  border-radius: 999px; border: 1px solid rgba(255,255,255,.09);
  background: rgba(22,25,33,.92); backdrop-filter: blur(8px); color: #e9ecf3;
  font-size: 12px; font-weight: 600; cursor: pointer; font-family: system-ui, sans-serif;
  box-shadow: 0 6px 24px rgba(0,0,0,.45); }
.muc-badge:hover { border-color: rgba(0,130,251,.45); }
.muc-badge .dot { width: 8px; height: 8px; border-radius: 50%; display: inline-block; }
`;

  let els = null;
  let pollTimer = null;

  function ensureDom() {
    if (els) return;
    const root = document.createElement("div");
    root.id = "muse-connect-root";
    root.innerHTML = `
      <div id="muse-connect-card">
        <div class="muc-head">
          <div class="muc-headicon">${ICONS.plug}</div>
          <div>
            <h2>Connect your Linux device</h2>
            <p class="sub">Terminal, files &amp; windows — straight from this chat.</p>
          </div>
          <button class="muc-close" title="Close">${ICONS.x}</button>
        </div>
        <div class="muc-body">
          <div class="muc-step">
            <div class="muc-stepnum">1</div>
            <div style="flex:1">
              <div class="t">Your device's Tailscale address</div>
              <div class="d">Just the name or IP — e.g. <b>mybox</b> or <b>100.93.60.2</b>. Muse finds the daemon automatically.</div>
              <input class="muc-input" id="muc-host" placeholder="mybox.tailnet-name.ts.net or 100.x.y.z" spellcheck="false" style="margin-top:8px" />
            </div>
          </div>
          <div class="muc-step">
            <div class="muc-stepnum" id="muc-step2num">2</div>
            <div style="flex:1">
              <div class="t">Approve on the device</div>
              <div class="d">Muse shows a 6-digit code. On your device, run:</div>
              <div class="muc-term" id="muc-term"><span class="p">$</span> muse pair <b id="muc-term-code">······</b></div>
              <div class="muc-codebox" id="muc-codebox">
                <span class="muc-code" id="muc-code">······</span>
                <button class="muc-copybtn" id="muc-copy">${ICONS.copy} Copy command</button>
              </div>
              <button class="muc-pairbtn" id="muc-go">${ICONS.link} Connect</button>
            </div>
          </div>
          <div class="muc-status" id="muc-status"><span class="muc-spin"></span><span id="muc-status-text"></span></div>

          <div class="muc-adv">
            <button class="muc-advtoggle" id="muc-adv-toggle">${ICONS.gear} Advanced — manual URL &amp; token ${ICONS.chevron}</button>
            <div class="muc-advbody" id="muc-advbody">
              <label>Device URL</label>
              <input class="muc-input" id="muc-url" placeholder="http://100.93.60.2:7300" spellcheck="false" />
              <label>Daemon token</label>
              <input class="muc-input" id="muc-token" placeholder="muse-…" spellcheck="false" />
              <div class="muc-advrow">
                <button id="muc-manual-save">${ICONS.check} Save &amp; verify</button>
                <button id="muc-forget">Forget device</button>
              </div>
            </div>
          </div>
        </div>
        <div class="muc-foot">${ICONS.shield} Approval happens on your device — Muse never sees the token until you approve.</div>
      </div>`;
    (document.body || document.documentElement).appendChild(root);
    const style = document.createElement("style");
    style.textContent = CSS;
    (document.head || document.documentElement).appendChild(style);

    root.addEventListener("click", (e) => { if (e.target === root) close(); });
    els = {
      root,
      host: root.querySelector("#muc-host"),
      go: root.querySelector("#muc-go"),
      codebox: root.querySelector("#muc-codebox"),
      code: root.querySelector("#muc-code"),
      termCode: root.querySelector("#muc-term-code"),
      copy: root.querySelector("#muc-copy"),
      status: root.querySelector("#muc-status"),
      statusText: root.querySelector("#muc-status-text"),
      step2num: root.querySelector("#muc-step2num"),
      advToggle: root.querySelector("#muc-adv-toggle"),
      advBody: root.querySelector("#muc-advbody"),
      url: root.querySelector("#muc-url"),
      token: root.querySelector("#muc-token"),
    };
    root.querySelector(".muc-close").addEventListener("click", close);
    els.go.addEventListener("click", expressConnect);
    els.copy.addEventListener("click", copyCmd);
    els.advToggle.addEventListener("click", () => {
      els.advBody.classList.toggle("show");
    });
    root.querySelector("#muc-manual-save").addEventListener("click", manualSave);
    root.querySelector("#muc-forget").addEventListener("click", forget);
    root.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && document.activeElement === els.host) expressConnect();
    });
  }

  function setStatus(msg, cls) {
    els.status.className = "muc-status" + (cls ? " " + cls : "");
    els.statusText.textContent = msg;
  }

  // Turn "mybox" / "100.x.y.z" into a daemon base URL.
  function baseFromHost() {
    let h = els.host.value.trim();
    if (!h) return null;
    if (!/^https?:\/\//.test(h)) h = "http://" + h;
    if (!/:\d+(\/|$)/.test(h) && !/\.[a-z]{2,}(\/|$)/i.test(h.replace(/^https?:\/\//, "").split("/")[0])) {
      h = h.replace(/\/$/, "") + ":7300";
    }
    return h.replace(/\/$/, "");
  }

  async function expressConnect() {
    const base = baseFromHost();
    if (!base) { setStatus("Enter your device's Tailscale name or IP first.", "err"); return; }
    clearInterval(pollTimer);
    els.go.disabled = true;
    els.codebox.classList.remove("show");
    setStatus("Looking for muse-daemon…", "busy");
    try {
      const start = await invoke("daemon_pair_start", { base });
      const code = start.code || "";
      if (!code) throw new Error("device did not return a pairing code");
      els.code.textContent = code;
      els.termCode.textContent = code;
      els.codebox.classList.add("show");
      els.step2num.classList.add("done");
      setStatus(`Code sent — approve on your device: muse pair ${code}`, "busy");
      // poll every 2s, up to 3 minutes
      let tries = 0;
      pollTimer = setInterval(async () => {
        tries++;
        try {
          const poll = await invoke("daemon_pair_poll", { base, code });
          if (poll.token) {
            clearInterval(pollTimer);
            const label = base.replace(/^https?:\/\//, "");
            await invoke("daemon_connect", { url: base, token: poll.token, label });
            await refresh();
            updateBadge();
            setStatus(`Connected — ${label}`, "ok");
            els.go.disabled = false;
            setTimeout(close, 1400);
          } else if (tries > 90) {
            clearInterval(pollTimer);
            setStatus("Timed out — no approval within 3 minutes.", "err");
            els.go.disabled = false;
          }
        } catch (e) {
          clearInterval(pollTimer);
          setStatus(String(e), "err");
          els.go.disabled = false;
        }
      }, 2000);
    } catch (e) {
      const msg = String(e);
      setStatus(
        msg.includes("refused") || msg.includes("timed out")
          ? `No daemon at ${base}. On the device run: curl -fsSL https://raw.githubusercontent.com/xanstomper/muse-linux/main/install.sh | bash`
          : msg,
        "err"
      );
      els.go.disabled = false;
    }
  }

  function copyCmd() {
    const cmd = "muse pair " + els.code.textContent.trim();
    navigator.clipboard?.writeText(cmd).catch(() => {});
    els.copy.innerHTML = ICONS.check + " Copied";
    setTimeout(() => { els.copy.innerHTML = ICONS.copy + " Copy command"; }, 1600);
  }

  async function manualSave() {
    const url = els.url.value.trim();
    const token = els.token.value.trim();
    if (!url || !token) { setStatus("URL and token are both required.", "err"); return; }
    setStatus("Verifying…", "busy");
    try {
      const probe = await invoke("daemon_probe", { url, token });
      if (probe.authorized) {
        await invoke("daemon_connect", { url, token, label: url.replace(/^https?:\/\//, "") });
        await refresh();
        updateBadge();
        setStatus(`Connected — ${probe.tools} tools available.`, "ok");
        setTimeout(close, 1400);
      } else {
        setStatus(probe.error || "Device reachable but token rejected.", "err");
      }
    } catch (e) {
      setStatus(String(e), "err");
    }
  }

  async function forget() {
    await invoke("daemon_disconnect");
    els.token.value = "";
    els.url.value = "";
    await refresh();
    updateBadge();
    setStatus("Device forgotten.", "");
  }

  function close() {
    clearInterval(pollTimer);
    els?.root.classList.remove("open");
  }

  async function refresh() {
    const conn = await invoke("daemon_get_connection");
    if (conn.paired) {
      els.url.value = conn.url;
      els.token.placeholder = "saved (…" + (conn.token_preview || "••••") + ")";
      els.step2num.classList.add("done");
      setStatus(`Paired: ${conn.label || conn.url}`, "ok");
    } else {
      setStatus("");
    }
  }

  async function open() {
    ensureDom();
    els.root.classList.add("open");
    await refresh();
    els.host.focus();
  }

  async function updateBadge() {
    try {
      await updateBadgeInner();
    } catch (e) {
      /* never let the connect UI break the host page */
    }
  }
  async function updateBadgeInner() {
    ensureDom();
    let badge = document.getElementById("muse-dev-badge");
    if (!badge) {
      badge = document.createElement("button");
      badge.id = "muse-dev-badge";
      badge.className = "muc-badge";
      badge.addEventListener("click", open);
      // attach OUTSIDE the app's React root (body) so SPA reconciliation never
      // sees our nodes — this was crashing muse.ai to a white screen
      (document.documentElement || document.body).appendChild(badge);
    }
    let s = {};
    try { s = await invoke("daemon_status"); } catch {}
    badge.innerHTML = `<span class="dot" style="background:${s.reachable ? "#3ecf6f" : s.paired ? "#ffb44d" : "#5b6272"}"></span>` +
      (s.paired ? (s.reachable ? "Device connected" : "Device offline") : "Connect device");
  }

  window.addEventListener("keydown", (e) => {
    if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === "d") {
      e.preventDefault();
      open();
    }
    if (e.key === "Escape" && els?.root.classList.contains("open")) close();
  });

  window.__museConnect = { open, close, refresh: updateBadge };

  function kick() { try { updateBadge(); } catch (e) {} }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", kick);
  } else {
    kick();
  }
  setInterval(kick, 15000);
  console.info("[MuseLinux] device connect panel v2 ready (Ctrl+Shift+D or bottom-left chip)");
})();
