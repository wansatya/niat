# NIAT: The Intent-Driven Operating System

**NIAT** is a minimal, headless Linux distribution built using **Buildroot + Linux kernel + BusyBox**, driven by a **Rust-based AI Agent Kernel and TUI** as the primary operating interface.

Operating NIAT is centered around expressing **intent** rather than navigating desktop apps or memorizing shell commands.

---

## Key Features

- **Direct UEFI Boot:** Boots directly from USB or internal NVMe/SATA storage into the NIAT TUI.
- **Intent-Driven Workspace:** Interactive AI Agent Kernel translates human goals into controlled system primitives.
- **Security Policy Engine:** Intercepts state-modifying or dangerous operations with clear confirmation prompts.
- **Model Context Protocol (MCP):** Connect external tools, databases, and APIs dynamically.
- **Interactive Configuration Modal:** Press `F3` or type `:config` to configure API URL, Model name, and API Key on the fly.
- **System Reboot & Clean Exit:** Type `:reboot` to restart or `:quit` to exit cleanly.
- **Instant Fallback Shell:** Press `F2` at any time to switch to BusyBox `ash`.
- **Integrated System Installer:** Deploy NIAT to internal disks directly from Live media.
- **Zero Graphical Overhead:** No X11/Wayland daemons or heavy GUI dependencies.

---

## Workspace TUI Commands & Shortcuts

| Command / Key | Action |
|---|---|
| `:config` or `F3` | Open interactive API Endpoint, Model, and API Key configuration modal |
| `:config set [url\|model\|key] <val>` | Update configuration parameter directly from chat |
| `:config show` | Show current LLM API URL, active model, and key status |
| `:reboot` | Safely reboot the operating system |
| `:quit` / `:exit` / `Ctrl+C` | Exit the TUI application |
| `:clear` / `Ctrl+L` | Clear workspace message history |
| `:status` | View system manager and model provider connectivity status |
| `:help` | Show in-app command and shortcut reference |
| `F2` | Breakout directly into raw BusyBox shell (`ash`) |
| `PageUp` / `PageDown` | Scroll message history |

---

## Quick Start

### 1. Build the OS Image

```bash
./scripts/setup.sh
./scripts/build.sh
```

This generates the following artifacts under `dist/`:
- `dist/niat.img` — UEFI bootable raw disk image for USB drives
- `dist/niat.iso` — Live bootable hybrid ISO image
- `dist/bzImage` — Compressed Linux kernel
- `dist/rootfs.cpio.gz` — Compressed initramfs

### 2. Run in QEMU Emulator

```bash
./scripts/run-qemu.sh
```

### 3. Flash to USB Drive

```bash
./scripts/make-usb.sh /dev/sdX
```

### 4. Install to Device

Boot from the USB drive and choose **"2. Launch NIAT System Installer"** from the GRUB boot menu, or launch `/usr/bin/niat-installer` from inside the live session.

---

## Repository Structure

```text
niat/
├── SPEC.md             — Complete architectural specification
├── Cargo.toml          — Rust workspace manifest
├── crates/             — Rust application sources
│   ├── agent-kernel    — Intent parser, planner & policy engine
│   ├── niat-tui        — Ratatui console user interface
│   ├── tool-runtime    — System tool execution runtime
│   ├── mcp-client      — Model Context Protocol client
│   ├── niat-installer  — Disk partitioning & OS installer
│   ├── system-manager  — Hardware & network supervisor
│   └── common          — Shared types, config, error definitions
├── buildroot/          — Buildroot external tree integration
│   └── external/niat   — Packages, board overlay & defconfig
├── scripts/            — Build, test, flashing & packaging scripts
└── dist/               — Bootable ISO, disk image, kernel & initramfs
```

---

## License

MIT License. See [LICENSE](LICENSE) for details.
