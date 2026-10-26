#!/usr/bin/env bash
# Muse Linux daemon installer — run on the device you want to control from Muse desktop.
set -euo pipefail

REPO="https://github.com/xanstomper/muse-linux"
INSTALL_DIR="$HOME/.muse/bin"
POLICY_DIR="$HOME/.muse"

echo "[muse] installing daemon to $INSTALL_DIR"

# 1. Build or download
BIN=""
if command -v cargo >/dev/null 2>&1; then
  echo "[muse] cargo found — building daemon from source"
  TMP="$(mktemp -d)"
  git clone --depth 1 "$REPO" "$TMP/muse-linux"
  (cd "$TMP/muse-linux/daemon" && cargo build --release)
  BIN="$TMP/muse-linux/target/release/muse-daemon"
  INSTALL_TMP="$(mktemp -d)"
  cp "$BIN" "$INSTALL_TMP/muse-daemon"
  BIN="$INSTALL_TMP/muse-daemon"
else
  echo "[muse] no cargo — downloading prebuilt binary"
  ARCH="$(uname -m)"; [ "$ARCH" = "x86_64" ] && ARCH="amd64"
  curl -fsSL "$REPO/releases/latest/download/muse-daemon-linux-$ARCH" -o /tmp/muse-daemon
  chmod +x /tmp/muse-daemon
  BIN=/tmp/muse-daemon
fi

mkdir -p "$INSTALL_DIR"
cp "$BIN" "$INSTALL_DIR/muse-daemon"
chmod +x "$INSTALL_DIR/muse-daemon"
# pairing helper: `muse pair <code>` approves from the device itself
mkdir -p "$HOME/.local/bin"
curl -fsSL "$REPO/raw/master/bin/muse-pair" -o "$HOME/.local/bin/muse-pair" || true
chmod +x "$HOME/.local/bin/muse-pair" 2>/dev/null || true

# 2. Default policy (localhost tools, tailscale-only transport) if none exists
POLICY_PATH="$HOME/.config/muse-daemon/policy.json"
if [ ! -f "$POLICY_PATH" ]; then
  mkdir -p "$HOME/.config/muse-daemon"
  cat > "$POLICY_PATH" <<'EOF'
{
  "capabilities": {"files":"read","terminal":"ask","windows":"read","browser":"off","screen":"ask","camera":"off","notifications":"auto","system":"read"},
  "allowed_folders": [],
  "blocked_folders": [".ssh",".gnupg",".password-store"],
  "blocked_apps": ["keepassxc","seahorse"],
  "blocked_commands": ["rm -rf /","rm -rf ~","mkfs","dd if=/dev/zero","shutdown","reboot","poweroff"],
  "require_approval_for": ["files.write","files.move","files.trash","shell.exec","windows.click","windows.type","windows.key","browser.click","browser.type","camera.snap"],
  "audit_log": ".local/share/muse-daemon/audit.log",
  "max_output_bytes": 262144,
  "default_timeout_ms": 15000,
  "transport": { "bind": "0.0.0.0:7300" }
}
EOF
  echo "[muse] wrote default policy to $POLICY_PATH"
fi

# 3. systemd user service
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/muse-daemon.service <<EOF
[Unit]
Description=Muse Linux daemon
After=network-online.target

[Service]
ExecStart=$INSTALL_DIR/muse-daemon
Restart=on-failure

[Install]
WantedBy=default.target
EOF
systemctl --user daemon-reload
systemctl --user enable --now muse-daemon

echo "[muse] done. Daemon listening on 0.0.0.0:7300."
echo "[muse] Pair from the Muse desktop app: Connect Device."
