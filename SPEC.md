# NIAT: The Intent-Driven Operating System

## 1. Project

**NIAT** is a minimal, headless Linux distribution built using **Buildroot + Linux kernel + BusyBox**, driven by a **Rust-based AI Agent Kernel and TUI** as the primary operating interface.

The name **NIAT** reflects its fundamental paradigm: operating a system by expressing *intent* rather than navigating graphical applications or remembering shell commands.

The system must:

1. Boot from USB on standard x86_64 hardware.
2. Start directly into the NIAT Terminal UI (TUI).
3. Operate without any desktop environment or graphical display server stack (no X11/Wayland).
4. Provide immediate network initialization (Ethernet / DHCP).
5. Connect to an LLM provider (remote API or local engine).
6. Translate human intent into controlled system tool executions.
7. Support the Model Context Protocol (MCP) for extensible system capabilities.
8. Provide a conventional fallback shell (BusyBox `ash`) for emergency maintenance and debugging.
9. Support installation from Live USB onto an internal drive via `niat-installer`.
10. Produce a reproducible, bootable ISO and raw disk image (`niat.iso` / `niat.img`).

The project remains strictly focused: lightweight, modular, secure, deterministic, and understandable.

---

# 2. Core Philosophy

NIAT is not a lightweight Linux desktop distro with an AI sidebar.

It is an **intent-driven operating system**.

### Traditional OS Architecture:

```text
User
 ↓
GUI / Shell Apps
 ↓
Monolithic Applications
 ↓
OS Kernel & Userspace
```

### NIAT Architecture:

```text
Human Intent
 ↓
NIAT TUI
 ↓
Agent Kernel (Planner + Policy + Tool Router)
 ↓
Controlled Tools / MCP / System Primitives
 ↓
Linux Kernel
```

In NIAT:
* **Intent is the primary interface.** You state what you want to achieve; the Agent Kernel formulates and executes a safe plan.
* **The terminal shell is the fallback.** Available at any time for manual intervention.
* **The GUI does not exist.** Eliminating graphical dependencies maximizes security, speed, and minimalism.

---

# 3. Target Hardware

### Initial Target (v0.1):

```text
Architecture: x86_64
Boot: UEFI (GPT)
CPU: Modern 64-bit x86
RAM: >= 2 GB
Storage: >= 4 GB
Network: Ethernet (DHCP)
```

The core software stack must avoid unnecessary hardware-specific bindings.

### Future Target Architectures:

```text
ARM64 (Raspberry Pi, Single-Board Computers)
RISC-V
Edge & Mini PCs
Headless Servers
```

---

# 4. Technology Stack

## Base OS

* **Buildroot** (External tree architecture)
* **Linux Kernel** (Long-Term Support kernel)
* **BusyBox** (Minimal userspace utilities)
* **musl libc** (Lightweight, statically-linkable C standard library)
* Minimal userspace without systemd, X11, Wayland, or heavy graphical libraries.

## Init System

* **BusyBox init**
* Simple init scripts under `/etc/init.d/` supervising system services and the NIAT stack.
* systemd is explicitly excluded unless a non-resolvable dependency arises.

## Application Layer (Rust)

All core NIAT OS applications are implemented in Rust:

```text
agent-kernel      — Intent parser, planner, tool router, and permission manager
niat-tui          — Interactive console TUI interface
tool-runtime       — Execution engine for built-in system tools
mcp-client        — Model Context Protocol client engine
niat-installer    — Disk partitioning and OS installation engine
system-manager    — Hardware, network, and service status supervisor
```

Components compile into statically linked or self-contained musl binaries.

## TUI Stack

* **Ratatui** + **Crossterm**
* Direct rendering on Linux virtual console (`/dev/tty1`).

---

# 5. High-Level Architecture

```text
                         HUMAN INTENT
                              │
                              ▼
                     ┌──────────────────┐
                     │     NIAT TUI     │
                     │                  │
                     │ prompt input     │
                     │ streaming output │
                     │ action approval  │
                     │ system status    │
                     └────────┬─────────┘
                              │
                              ▼
                     ┌──────────────────┐
                     │   AGENT KERNEL   │
                     │                  │
                     │ intent planner   │
                     │ model client     │
                     │ policy engine    │
                     │ tool router      │
                     │ state engine     │
                     └────────┬─────────┘
                              │
         ┌────────────────────┼────────────────────┐
         │                    │                    │
         ▼                    ▼                    ▼
   System Tools             Shell              MCP Server
 (fs, net, sys)         (BusyBox ash)         (stdio/remote)
         │                    │                    │
         └────────────────────┼────────────────────┘
                              │
                              ▼
                     ┌──────────────────┐
                     │   Linux Kernel   │
                     └──────────────────┘
```

---

# 6. Boot Flow

```text
UEFI Firmware
  ↓
GRUB / systemd-boot
  ↓
Linux Kernel (bzImage)
  ↓
initramfs / Root Filesystem
  ↓
BusyBox init (/etc/init.d/rcS)
  ↓
Network & Device Initialization
  ↓
NIAT Agent Kernel (/usr/bin/agent-kernel)
  ↓
NIAT TUI (/usr/bin/niat-tui on /dev/tty1)
```

### Post-Boot Interface

Upon successful boot, the console presents the NIAT workspace:

```text
NIAT: The Intent-Driven Operating System (v0.1)

System Status:
  [✓] Linux Kernel 6.x
  [✓] Network (eth0: 192.168.1.150)
  [✓] Agent Kernel (Ready)
  [✓] Model Provider (Online)

────────────────────────────────────────────────────────────

What would you like to accomplish?

> _
```

---

# 7. Bootloader & Images

* **Target:** UEFI x86_64.
* Generated via Buildroot standard toolchain.
* **Output Artifacts** (`dist/`):
  * `bzImage` — Compressed Linux kernel
  * `rootfs.ext4` — Root filesystem image
  * `niat.iso` — Live bootable ISO image
  * `niat.img` — Raw flashable USB disk image

---

# 8. Project Directory Structure

The repository maintains strict separation between Buildroot configuration, Rust source crates, and automation scripts:

```text
niat/
├── README.md
├── SPEC.md
├── LICENSE
│
├── buildroot/
│   └── external/
│       └── niat/
│           ├── Config.in
│           ├── external.desc
│           ├── external.mk
│           │
│           ├── board/
│           │   └── x86_64/
│           │       ├── linux.config
│           │       ├── busybox.config
│           │       ├── rootfs-overlay/
│           │       └── post-build.sh
│           │
│           ├── configs/
│           │   └── niat_x86_64_defconfig
│           │
│           └── package/
│               ├── agent-kernel/
│               ├── niat-tui/
│               ├── mcp-client/
│               └── niat-installer/
│
├── crates/
│   ├── agent-kernel/
│   ├── niat-tui/
│   ├── mcp-client/
│   ├── tool-runtime/
│   ├── niat-installer/
│   └── common/
│
├── scripts/
│   ├── setup.sh
│   ├── build.sh
│   ├── run-qemu.sh
│   ├── make-usb.sh
│   └── release.sh
│
└── docs/
    ├── architecture.md
    ├── security.md
    └── development.md
```

---

# 9. Buildroot Integration

NIAT uses a **Buildroot External Tree** (`BR2_EXTERNAL`) to avoid upstream modifications.

Building the OS image:

```bash
./scripts/setup.sh
./scripts/build.sh
```

The build process will:
1. Fetch and unpack supported Buildroot source release.
2. Register the `buildroot/external/niat` tree.
3. Apply `niat_x86_64_defconfig`.
4. Cross-compile target Rust binaries using `x86_64-unknown-linux-musl`.
5. Assemble the root filesystem overlay.
6. Generate bootable ISO and raw disk images under `dist/`.

---

# 10. Rust Cross-Compilation

Target triple: `x86_64-unknown-linux-musl`

* Musl static linking ensures zero reliance on host glibc libraries.
* Cargo builds are executed via containerized cross-toolchain or Buildroot package hooks.
* Binaries are stripped and optimized for minimum size.

---

# 11. Agent Kernel

The `agent-kernel` is the core system daemon operating behind the TUI.

* **Binary:** `/usr/bin/agent-kernel`
* **Responsibilities:**
  * Intent parsing & task breakdown
  * LLM provider communication (API streaming)
  * System tool invocation and schema validation
  * Policy checking & confirmation prompts
  * MCP server management and protocol handling
  * Audit logging & session state tracking

### Execution Pipeline:

```text
User Intent
   │
   ▼
Planner / Model Client
   │
   ▼
Tool Request Schema
   │
   ▼
Security Policy Engine ──(Requires Confirmation)──► Human Approval via TUI
   │
   ▼ (Approved / Auto-Permitted)
Tool Executor
   │
   ▼
System Primitives / Subprocess / MCP
```

Unchecked execution of raw LLM-generated shell strings directly to `/bin/sh` is strictly forbidden. All operations must route through typed tools or policy-checked command wrappers.

---

# 12. LLM / Intent Engine Provider

The Agent Kernel communicates with LLM providers through a unified trait abstraction:

```rust
#[async_trait]
pub trait ModelProvider {
    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<ChatChunk>>>;
}
```

### Supported Backends:
* **OpenAI-compatible APIs** (OpenAI, Groq, OpenRouter, vLLM, Ollama, LocalAI)

### Configuration parameters (`/etc/niat/config.toml`):

```toml
[model]
base_url = "https://api.openai.com/v1"
model = "gpt-4o"
api_key_env = "NIAT_API_KEY"
```

The system does not hard-code any single vendor. Local inference endpoints (e.g. Ollama/vLLM on local network) are fully supported.

---

# 13. Network Initialization

Boot network sequence:
1. Detect network interfaces (`eth0`, `wlan0`).
2. Bring up interface via `udhcpc` / BusyBox network scripts.
3. Verify DNS resolution and internet reachability.
4. Signal `agent-kernel` network availability status.

If network connectivity is unavailable:
* NIAT boots cleanly into offline mode.
* The TUI displays network status warnings and provides standard local tools and direct shell access.
* System operations never block indefinitely on missing network.

---

# 14. NIAT Terminal Interface (TUI)

The `niat-tui` provides an intuitive, high-performance interface for intent-driven computing.

```text
┌───────────────────────────────────────────────────────────────┐
│ NIAT OS v0.1                                     NETWORK: ON ●│
├───────────────────────────────────────────────────────────────┤
│                                                               │
│ You:                                                          │
│ Inspect disk space and remove temporary files under /tmp.     │
│                                                               │
│ Agent Kernel:                                                 │
│ I will check disk utilization and safely clean /tmp.          │
│                                                               │
│ [Tool] get_system_info                                        │
│   └─ Storage: 45GB / 50GB used (90%)                          │
│                                                               │
│ ⚠ ACTION REQUIRES CONFIRMATION                                │
│ Tool: delete_files                                            │
│ Target: /tmp/* (1.2 GB, 142 files)                            │
│                                                               │
│ [Y] Approve   [N] Deny   [E] Edit Plan                        │
│                                                               │
├───────────────────────────────────────────────────────────────┤
│ > _                                                           │
└───────────────────────────────────────────────────────────────┘
```

### Key Capabilities:
* Async non-blocking rendering (Ratatui + Crossterm)
* Real-time streaming response token rendering
* Command & intent history buffer
* Dynamic modal dialogs for security policy approvals
* Hotkey shortcuts (`Ctrl+C` interrupt, `Ctrl+L` clear, `F2` shell switch)

---

# 15. Tool System

Tools present clean, JSON-Schema validated interfaces to the Agent Kernel.

### Built-in System Tools (v0.1):

| Tool Name | Category | Description | Safety Level |
|---|---|---|---|
| `read_file` | Filesystem | Read file contents with limiters | SAFE |
| `write_file` | Filesystem | Write or update file content | CONFIRM |
| `list_directory` | Filesystem | List directory tree | SAFE |
| `search_files` | Filesystem | Pattern search across files | SAFE |
| `run_command` | System | Execute verified shell sub-process | CONFIRM |
| `get_system_info` | System | Query CPU, RAM, disk, OS info | SAFE |
| `get_network_status` | Network | Ping, DNS, interface status | SAFE |
| `manage_service` | System | Control system processes/init | CONFIRM |
| `partition_disk` | Disk | Partition target storage block | DANGEROUS |

---

# 16. Shell Access & Breakout

NIAT preserves standard Linux maintenance capabilities:

1. **Interactive Shell Switch:** Pressing `F2` or issuing intent command `open shell` suspends TUI and launches BusyBox `ash`.
2. **TTY Console Switching:** Virtual console `tty2` runs a standard getty login prompt.
3. Shell access operates independently of network status or Agent Kernel state.

```text
root@niat:~#
```

---

# 17. Model Context Protocol (MCP) Support

NIAT integrates MCP to allow modular capabilities (e.g. Git, databases, external APIs) without rebuilding the core OS.

### Configuration (`/etc/niat/mcp.json`):

```json
{
  "mcpServers": {
    "git": {
      "command": "/usr/bin/mcp-git",
      "args": ["--repository", "/home/user/repo"]
    },
    "sqlite": {
      "command": "/usr/bin/mcp-sqlite",
      "args": ["--db", "/var/lib/niat/data.db"]
    }
  }
}
```

The Agent Kernel dynamically discovers and exposes tools registered by active MCP servers.

---

# 18. Security & Permission Policy Model

The Security Engine prevents unauthorized or dangerous LLM operations.

### Permission Classes:

1. **SAFE (Auto-Execute):** Read-only inspection commands (`read_file`, `list_directory`, `get_system_info`).
2. **CONFIRM (User Confirmation):** State-modifying operations (`write_file`, `install_package`, non-destructive `run_command`).
3. **DANGEROUS (Strict Double Confirmation):** Destructive system actions (`partition_disk`, `format_disk`, `reinstall_os`, deleting `/etc` or `/var`).

### Policy Rule Enforcements:
* No LLM output can bypass the approval prompt modal for CONFIRM or DANGEROUS tools.
* User can set `require_confirmation = true` in `/etc/niat/config.toml` to enforce confirmation on all write tools.
* All tool calls are recorded in the security audit log (`/var/log/niat/security.log`).

---

# 19. Directory Layout & User Data

```text
/etc/niat/
├── config.toml       — Main NIAT system configuration
├── mcp.json          — Model Context Protocol server configuration
└── policy.toml       — Security approval rules and tool policies

/var/log/niat/
├── agent.log         — Agent Kernel operational logs
├── system.log        — Boot and network logs
└── security.log      — Immutable tool execution & approval audit trail

/var/lib/niat/
├── history.sqlite    — Local chat and intent history
└── state/            — Agent session state files
```

API keys and credentials are saved with restricted permissions (`0600`) and never output in log files.

---

# 20. Storage & Persistence Model

### 1. Live USB Mode (Ephemeral RAM rootfs)
Boots entirely in memory using `initramfs` (`cpio.gz` / `ext4`). Internal drives remain untouched.

### 2. Live USB with Persistent Overlay (Optional)
Secondary FAT32/ext4 partition on USB labeled `NIAT_DATA` mounted at `/var/lib/niat` for persistent settings and history.

### 3. Installed Mode (Internal NVMe/SATA Disk)
Dedicated OS installation created by `niat-installer`.

---

# 21. Installer (`niat-installer`)

`niat-installer` is a standalone Rust TUI application that installs NIAT from Live USB onto target internal storage.

* **Binary:** `/usr/bin/niat-installer`

```text
────────────────────────────────────────────────────────────
NIAT System Installer
────────────────────────────────────────────────────────────

Select Target Storage Disk:
  ► [1] /dev/sda - 1024 GB (SATA SSD) — Detected: Windows, Fedora Linux, EFI System
    [2] /dev/nvme0n1 - 512 GB (NVMe SSD)

Installation Mode:
  ► [1] Use Existing Partition (Preserve Existing Boot & OS - Dual Boot)
    [2] Erase Entire Disk (Clean Install)

Target Partition Selection (Mode 1):
  • /dev/sda1 - 200 MB (vfat) [EFI System Partition]
  • /dev/sda3 - 200 GB (ntfs) [Windows OS / NTFS]
  ► /dev/sda6 - 128 GB (ext4) [Linux System Partition — Target Root]
  • /dev/sda8 - 621 GB (ext4) [Fedora Linux System]

Existing OS bootloaders (Windows / Fedora) in EFI will be preserved in GRUB boot menu.
Type 'INSTALL' to confirm and proceed:
> _
```


---

# 22. Live USB Experience

Booting the Live USB image presents a choice menu:

```text
1. Boot NIAT Live Workspace
2. Launch NIAT System Installer
3. Recovery Console (BusyBox Shell)
4. Reboot System
```

---

# 23. QEMU Development Workflow

Developers can build and test NIAT entirely inside standard Linux environments using QEMU:

```bash
./scripts/run-qemu.sh
```

Which executes QEMU with UEFI firmware (`OVMF`):

```bash
qemu-system-x86_64 \
  -enable-kvm \
  -m 2G \
  -bios /usr/share/ovmf/OVMF.fd \
  -drive file=dist/niat.img,format=raw \
  -net nic,model=virtio \
  -net user \
  -nographic
```

---

# 24. Image Flashing (`make-usb.sh`)

Utility to safely create bootable USB media:

```bash
# Build raw image only
./scripts/make-usb.sh --image-only

# Write directly to USB drive (with safety check & confirmation)
./scripts/make-usb.sh /dev/sdX
```

---

# 25. Configuration Reference (`/etc/niat/config.toml`)

```toml
[system]
hostname = "niat"
log_level = "info"

[model]
base_url = "https://api.openai.com/v1"
model = "gpt-4o"
timeout_seconds = 30
max_tokens = 4096

[agent]
max_tool_iterations = 15
system_prompt = "You are NIAT, an intent-driven operating system assistant..."

[security]
require_confirmation = true
allow_raw_shell = false

[network]
dhcp = true
check_connectivity = true
```

> Location: the TUI and agent kernel resolve the live file as `~/.niat/config.toml` first, then `/etc/niat/config.toml`, `/var/lib/niat/config.toml`, and the legacy `/tmp/niat_config.toml`. When the user file is missing it is seeded from the next available file (or defaults) and used going forward; the shipped `/etc/niat/config.toml` acts as a seed, never as the sticky live file.

---

# 26. Resource & Performance Targets

| Metric | Target |
|---|---|
| Cold Boot Time (UEFI to TUI) | < 8 seconds (modern NVMe) |
| Idle RAM Usage | < 120 MB |
| ISO Image Size | < 250 MB |
| Base Dependencies | 0 GUI libraries, 0 desktop daemons |

---

# 27. Definition of Done — v0.1 Milestone

- [x] Distro official name established: **NIAT (The Intent-Driven Operating System)**.
- [ ] Buildroot external tree compiles cleanly for `x86_64`.
- [ ] UEFI boot succeeds in QEMU and physical PC.
- [ ] BusyBox init handles system startup cleanly without systemd.
- [ ] Network initializes automatically via DHCP.
- [ ] `agent-kernel` daemon launches successfully on boot.
- [ ] `niat-tui` renders on `/dev/tty1`.
- [ ] OpenAI-compatible model endpoint communicates with `agent-kernel`.
- [ ] Built-in system tools (`read_file`, `write_file`, `list_directory`, `run_command`, `get_system_info`) execute successfully.
- [ ] Confirmation policy engine intercepts CONFIRM/DANGEROUS operations.
- [ ] Basic stdio MCP server support functions properly.
- [ ] Live USB image (`niat.iso`) boots cleanly into RAM without modifying disk.
- [ ] `niat-installer` partitions, formats, and installs NIAT to internal target disk.
- [ ] `F2` hotkey switches to fallback shell (`ash`).
- [ ] Checksums generated for all dist build artifacts.

---

# 28. Guiding Principles for Implementation

When writing code or configuring NIAT:

1. **Prefer the smallest reliable implementation.**
2. **Intent before command:** Design interfaces around *what the user wants done*, not low-level binary flags.
3. **No hidden actions:** Every state change performed by the AI must be visible or confirmed.
4. **Resilient fallback:** The Linux shell must always be available if network or LLM connectivity fails.
5. **No unnecessary daemons:** Keep userspace clean, lightweight, and deterministic.
