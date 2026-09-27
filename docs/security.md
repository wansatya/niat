# NIAT Security & Permission Policy Model

The NIAT security architecture ensures that artificial intelligence models cannot execute unauthorized or destructive system operations without explicit human consent.

## Permission Classes

| Safety Level | Policy | Description |
|---|---|---|
| **SAFE** | Auto-Execute | Read-only inspection commands (`read_file`, `list_directory`, `search_files`, `get_system_info`, `get_network_status`). |
| **CONFIRM** | Human Approval Prompt | Operations that modify system state (`write_file`, `run_command`, `manage_service`). |
| **DANGEROUS** | Strict Double Confirmation | Destructive system modifications (`partition_disk`, formatting drives, reinstallation). |

## Security Rules

1. **No Raw Shell Passthrough:** Unchecked execution of raw LLM output strings directly to `/bin/sh` is strictly forbidden.
2. **Policy Modal Interception:** Any tool flagged as `CONFIRM` or `DANGEROUS` halts execution until the user presses `[Y]` in the TUI confirmation modal.
3. **Audit Logging:** Every intent, tool request, approval, and execution output is recorded to `/var/log/niat/security.log`.
4. **Offline Resilience:** System security policies apply identically whether connected to remote LLMs or running locally.
