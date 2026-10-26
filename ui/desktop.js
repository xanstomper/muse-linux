// Muse Linux — desktop behavior fixes injected on every page load.
(function () {
  if (window.__museDesktop) return;

  const desktop = {
    // n5: zoom via text-size (WebKitGTK webview zoom API not exposed to JS)
    zoom(delta) {
      if (delta === null) {
        document.documentElement.style.fontSize = "";
        return;
      }
      const cur = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
      const next = Math.min(28, Math.max(10, cur + delta * 16));
      document.documentElement.style.fontSize = next + "px";
    },

    // n5: Ctrl+L new chat — muse.ai keeps conversation in DOM; reload is the
    // reliable reset. If the page exposes its own hook, prefer it.
    newChat() {
      const btn = document.querySelector('[aria-label*="new"], [data-testid*="new-chat"]');
      if (btn) { btn.click(); } else { location.href = "https://muse.ai/"; }
    },

    // n4: find-in-page. WebKitGTK has no JS find API; provide a lightweight
    // highlight-free fallback via window.find (works in WebKit).
    find() {
      const q = prompt("Find in page:");
      if (q) window.find(q);
    },
  };
  window.__museDesktop = desktop;

  // n5: Ctrl+wheel zoom (native desktop expectation)
  window.addEventListener(
    "wheel",
    (e) => {
      if (e.ctrlKey) {
        e.preventDefault();
        desktop.zoom(e.deltaY < 0 ? 0.1 : -0.1);
      }
    },
    { passive: false }
  );

  // n1: external <a target="_blank"> links must leave the app. The Rust
  // on_navigation handler covers top-level nav; catch new-tab/window links.
  document.addEventListener(
    "click",
    (e) => {
      const a = e.target.closest?.("a[href]");
      if (!a) return;
      const href = a.href || "";
      const external =
        a.target === "_blank" ||
        (!href.startsWith("https://muse.ai") &&
          !href.startsWith("https://accounts.muse.ai") &&
          href.startsWith("http"));
      if (external) {
        e.preventDefault();
        e.stopPropagation();
        // ask the shell to open it (Tauri shell plugin is available via invoke)
        try {
          const { invoke } = window.__TAURI_INTERNALS__;
          invoke("plugin:shell|open", { path: href });
        } catch {
          window.open(href, "_blank");
        }
      }
    },
    true
  );

  // n2: make prompt()/confirm() usable — WebKitGTK dialogs are ugly but work.
  // n7: offline detection — show native-feeling error screen, not white void.
  function offlineScreen() {
    if (document.getElementById("muse-offline")) return;
    const d = document.createElement("div");
    d.id = "muse-offline";
    d.innerHTML =
      '<h1>Muse can\'t connect</h1>' +
      '<p>Check your internet connection, then try again.</p>' +
      '<button onclick="location.reload()">Retry</button>';
    (document.body || document.documentElement).appendChild(d);
  }
  function onlineAgain() {
    document.getElementById("muse-offline")?.remove();
  }
  window.addEventListener("offline", offlineScreen);
  window.addEventListener("online", onlineAgain);
  if (navigator.onLine === false) offlineScreen();

  console.info("[MuseLinux] desktop UX layer ready (zoom/find/links/offline)");
})();
