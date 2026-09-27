# NIAT Architecture

NIAT is an **intent-driven headless Linux operating system** that replaces traditional desktop environments with a native Rust AI Agent Kernel and Ratatui TUI.

```text
                         HUMAN INTENT
                               │
                               ▼
                      ┌──────────────────┐
                      │     NIAT TUI     │
                      │  (Ratatui/Crossterm)
                      │                  │
                      │ prompt input     │
                      │ streaming output │
                      │ action approval  │
                      │ system status    │
                      └────────┬─────────┘
                               │ Unix Domain Socket
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

## Subsystems

1. **`agent-kernel` (`/usr/bin/agent-kernel`)**
   - Core background daemon supervising model communication, intent translation, and tool routing.
   - Listens on Unix socket `/var/run/niat/agent.sock`.

2. **`niat-tui` (`/usr/bin/niat-tui`)**
   - Renders directly on `/dev/tty1`.
   - Supports non-blocking async input, streaming token display, and interactive modal dialogs for security confirmations.
   - Hotkeys: `F2` for fallback shell, `Ctrl+L` to clear, `Ctrl+C` to quit.

3. **`tool-runtime`**
   - Built-in type-safe execution engine for filesystem, network, process, and system querying primitives.
   - Strict classification into `SAFE`, `CONFIRM`, and `DANGEROUS`.

4. **`mcp-client`**
   - Model Context Protocol integration over stdio JSON-RPC for external extensibility.

5. **`niat-installer` (`/usr/bin/niat-installer`)**
   - Standalone installer for deploying NIAT to internal NVMe/SATA storage drives from live boot media.

6. **`system-manager`**
   - Probes hardware, kernel, network interfaces, and monitors connectivity.
