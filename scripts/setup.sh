#!/usr/bin/env bash
set -euo pipefail

echo "===================================================="
echo " NIAT OS — Development Environment Setup"
echo "===================================================="

# Check for Rust & Cargo
if ! command -v cargo &>/dev/null; then
    echo "[-] Rust toolchain not found. Installing rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

echo "[+] Rust version: $(rustc --version)"

# Ensure target for static musl builds
echo "[+] Adding x86_64-unknown-linux-musl target..."
rustup target add x86_64-unknown-linux-musl 2>/dev/null || true

# Check optional tools
for tool in qemu-system-x86_64 mcopy mformat grub2-mkrescue xorriso; do
    if command -v "$tool" &>/dev/null; then
        echo "[+] Found $tool"
    else
        echo "[!] Optional/Packaging tool '$tool' not found in PATH."
    fi
done

echo ""
echo "[✓] Environment setup complete."
