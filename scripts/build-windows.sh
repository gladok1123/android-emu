#!/usr/bin/env bash
# Cross-compile android-emu for Windows from Linux/macOS.
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "${1:-}" == "--msvc" ]]; then
  cargo build --release
else
  rustup target add x86_64-pc-windows-gnu
  if ! command -v x86_64-w64-mingw32-gcc >/dev/null; then
    echo "install mingw-w64 first:"
    echo "  Debian/Ubuntu: sudo apt install gcc-mingw-w64-x86-64"
    echo "  Fedora:        sudo dnf install mingw64-gcc"
    exit 1
  fi
  cargo build --release --target x86_64-pc-windows-gnu
  cp -f target/x86_64-pc-windows-gnu/release/android-emu.exe .
fi
echo "done: $(pwd)/android-emu.exe"
