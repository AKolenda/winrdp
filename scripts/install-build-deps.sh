#!/usr/bin/env bash
set -euo pipefail
if [[ $(id -u) -eq 0 ]]; then
  echo "Run as your normal user; this script calls sudo only for apt." >&2; exit 1
fi
command -v apt-get >/dev/null || { echo "This helper targets Zorin/Ubuntu/Debian." >&2; exit 1; }
echo "This installs build tools and GTK/WebKit development packages from your configured repositories."
echo "It does not change graphics drivers, RDP host services, firewall rules, or your existing Win RDP runtime."
sudo apt-get update
sudo apt-get install build-essential cmake ninja-build pkg-config python3 curl ca-certificates \
  file dpkg-dev patchelf libgtk-3-dev libwebkit2gtk-4.1-dev libssl-dev libxdo-dev \
  librsvg2-dev libayatana-appindicator3-dev libasound2-dev \
  libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev fonts-dejavu-core
echo "Rust is installed separately via rustup; see BUILDING.md."
echo "The build reuses your existing Qt/FreeRDP runtime. It does not replace or rebuild it."
