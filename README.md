<div align="center">

<img src="src-tauri/icons/muse-logo.svg" width="110" alt="Muse logo"/>

# Muse for Linux

### The Mac version of Meta Muse — but for Linux, and better.

A native Linux client for **Meta Muse** with a real system layer: a policy-gated
companion daemon, shell & window control, audit logging, and one-command launch.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20X11-0064E0?logo=linux&logoColor=white)](#requirements)
[![Built with Tauri](https://img.shields.io/badge/built%20with-Tauri%20v2-FFC131?logo=tauri&logoColor=black)](https://tauri.app)
[![Rust](https://img.shields.io/badge/rust-1.85+-DEA584?logo=rust&logoColor=white)](https://rustup.rs)
[![Release](https://img.shields.io/badge/download-deb%20%7C%20AppImage-0082FB?logo=github&logoColor=white)](#install)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](CONTRIBUTING.md)
[![Discussions](https://img.shields.io/badge/discussions-open-8B5CF6?logo=github&logoColor=white)](../../discussions)

**[Install](#install) · [Features](#features) · [How it works](#how-it-works) · [Build](#build-from-source) · [Roadmap](#roadmap)**

</div>

---

> [!TIP]
> **Tired of Muse living in a browser tab?** Muse for Linux gives the AI agent
> real hands on your machine — gated by a permission layer *you* control.

## Why

Meta ships Muse for Mac and the web. This project is what the Linux client
should be: the full Muse experience in a native window **plus** a local
companion daemon that gives your agent actual system access — safely.

| Web app | Muse for Linux |
|---|---|
| Chat only | Chat + **shell, filesystem, windows, notifications** |
| No permissions model | Per-capability policy (`off/read/read-write/ask`) + audit log |
| Sandboxed browser tab | Native window, tray, `muse` CLI, systemd service |
| Agent can't touch your system | Agent *can* — but only through the policy gate |

## Features

- 🪟 **Native WebKitGTK window** — real desktop app, tray icon, global `muse` command
- 🤖 **Companion daemon** (`muse-daemon`) — 20 tools: persistent shell sessions,
  file ops, window control (wmctrl/xdotool), screenshots, clipboard, notifications
- 🔐 **Policy-gated by design** — every tool runs through `policy.json`:
  capabilities off by default, path allowlists, blocked commands, refusal (not prompt-bombing) for ask-gated tools
- 📜 **Full audit log** — every call logged with tool, status, and redacted args
- 🛡 **Process-group hardening** — timeouts SIGKILL the whole tree, no orphans
- 🌐 **Tailscale-native** — daemon binds your tailscale IP; the laptop app can drive your desktop's tools from anywhere on your tailnet
- ⚡ **Fast** — rendering tuning measured at 365→15 scroll jiffies on Alder Lake-N
- 📦 **One package** — deb, rpm, or standalone AppImage

## Install

### Debian / Ubuntu (deb)

```bash
wget https://github.com/xanstomper/muse-linux/releases/latest/download/Muse_0.1.0_amd64.deb
sudo apt install ./Muse_0.1.0_amd64.deb
```

### Any distro (AppImage)

```bash
mkdir -p ~/Applications
wget https://github.com/xanstomper/muse-linux/releases/latest/download/Muse-0.1.0-x86_64.AppImage -O ~/Applications/Muse.AppImage
chmod +x ~/Applications/Muse.AppImage
```

### Launch it

```bash
muse
```

The `muse` command is installed with the deb, or drop
[bin/muse](bin/muse) onto your `PATH` for AppImage installs.

> [!IMPORTANT]
> You need a Meta Muse account (muse.ai). This client embeds the official
> web experience — it does **not** bypass their auth.

### First run

1. Start the companion daemon (the system layer):
   ```bash
   systemctl --user enable --now muse-daemon
   ```
2. Token lives at `~/.config/muse-daemon/token` — created on first boot.
3. Tweak permissions in `~/.config/muse-daemon/policy.json` (see
   [config/policy.json](config/policy.json) for every knob).

## How it works

```
┌────────────────────┐        ┌──────────────────────────┐
│  Muse webview       │ HTTP   │  muse-daemon              │
│  (muse.ai + bridge) ├───────►│  (systemd user service)   │
│  __museNative.*     │Bearer  │                           │
└────────────────────┘        │  policy.json  ┌─────────┐ │
                              │  audit.log    │ 20 tools │ │
                              │               └─────────┘ │
                              │  shell · files · windows  │
                              │  screen · clipboard · not │
                              └───────────┬───────────────┘
                                          │ Tailscale IP only
                                          ▼
                              your actual Linux system
```

- **The app never enforces policy.** It forwards calls to the daemon; the
  daemon refuses anything outside `policy.json` and writes an audit line.
- **The daemon never trusts the app.** Bearer token, Tailscale-only bind,
  read-only defaults for files, blocked lists for shell.
- **Ask-gated tools are refused headlessly** — no prompt-storms. Set
  `MUSE_DAEMON_YOLO=1` if you accept the risk for a session.

## Requirements

- Linux with X11 (Wayland via XWayland works)
- `webkit2gtk-4.1`, `wmctrl`, `xdotool` (deb pulls these in)
- A [Tailscale](https://tailscale.com) network for remote daemon access
  (optional but recommended — that's the "control your PC from your laptop" magic)

## Build from source

```bash
git clone https://github.com/xanstomper/muse-linux
cd muse-linux
npm install
cargo test --lib          # 6 unit tests
npm run tauri build -- --bundles deb   # or appimage/rpm
```

Workspace layout:

```
muse-linux/
├── src-tauri/     # Tauri v2 app (Rust): window, tray, native bridge, daemon proxy
├── daemon/        # muse-daemon: standalone policy-gated tool server
├── config/        # policy.json reference
├── ui/            # native-bridge.js injected into the webview
└── bin/           # `muse` launcher script
```

## Roadmap

- [ ] Wayland-native (no XWayland)
- [ ] Permission prompts in-app (interactive "ask" instead of refuse)
- [ ] Daemon on phones — run Muse tools from your tailnet
- [ ] Multi-machine tool routing
- [ ] Packaging: Flathub, AUR

See [open issues](../../issues) — good first issues tagged `good first issue`.

## Contributing

PRs welcome! Read [CONTRIBUTING.md](CONTRIBUTING.md), run `cargo test --lib`
and `cargo fmt` before submitting. Please match the existing KISS/DRY style —
no new deps for things stdlib does.

## Disclaimer

This is an independent, unofficial Linux client. Not affiliated with or
endorsed by Meta. All trademarks belong to their respective owners. Use the
daemon responsibly — it executes commands on your machine under your policy.

<div align="center">

**Built with** [Tauri](https://tauri.app) **·** [WebKitGTK](https://webkitgtk.org) **·** lots of `cargo test`

⭐ Star this repo if Muse for Linux saves you a browser tab

</div>
