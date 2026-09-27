//! Built-in system tools matching SPEC §15.

use niat_common::types::{SafetyLevel, ToolResult};
use serde_json::Value;
use std::collections::HashMap;

/// Trait for all executable system tools.
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn safety_level(&self) -> SafetyLevel;
    fn execute(&self, params: &Value) -> anyhow::Result<ToolResult>;
}

// ── read_file ──────────────────────────────────────────────

pub struct ReadFile;
impl Tool for ReadFile {
    fn name(&self) -> &str { "read_file" }
    fn description(&self) -> &str { "Read file contents with line limiters" }
    fn safety_level(&self) -> SafetyLevel { SafetyLevel::Safe }

    fn execute(&self, params: &Value) -> anyhow::Result<ToolResult> {
        let path = params["path"].as_str().unwrap_or("");
        let max_lines = params["max_lines"].as_u64().unwrap_or(500) as usize;

        match std::fs::read_to_string(path) {
            Ok(content) => {
                let lines: Vec<&str> = content.lines().take(max_lines).collect();
                let truncated = content.lines().count() > max_lines;
                let output = if truncated {
                    format!("{}\n\n... truncated at {} lines", lines.join("\n"), max_lines)
                } else {
                    lines.join("\n")
                };
                Ok(ToolResult { success: true, output, error: None })
            }
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to read {}: {}", path, e)),
            }),
        }
    }
}

// ── write_file ─────────────────────────────────────────────

pub struct WriteFile;
impl Tool for WriteFile {
    fn name(&self) -> &str { "write_file" }
    fn description(&self) -> &str { "Write or update file content" }
    fn safety_level(&self) -> SafetyLevel { SafetyLevel::Confirm }

    fn execute(&self, params: &Value) -> anyhow::Result<ToolResult> {
        let path = params["path"].as_str().unwrap_or("");
        let content = params["content"].as_str().unwrap_or("");

        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        match std::fs::write(path, content) {
            Ok(()) => Ok(ToolResult {
                success: true,
                output: format!("Written {} bytes to {}", content.len(), path),
                error: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to write {}: {}", path, e)),
            }),
        }
    }
}

// ── list_directory ─────────────────────────────────────────

pub struct ListDirectory;
impl Tool for ListDirectory {
    fn name(&self) -> &str { "list_directory" }
    fn description(&self) -> &str { "List directory tree" }
    fn safety_level(&self) -> SafetyLevel { SafetyLevel::Safe }

    fn execute(&self, params: &Value) -> anyhow::Result<ToolResult> {
        let path = params["path"].as_str().unwrap_or(".");
        let mut entries = Vec::new();

        match std::fs::read_dir(path) {
            Ok(dir) => {
                for entry in dir.flatten() {
                    let ft = entry.file_type().map(|t| {
                        if t.is_dir() { "d" } else if t.is_symlink() { "l" } else { "-" }
                    }).unwrap_or("?");
                    let name = entry.file_name().to_string_lossy().to_string();
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    entries.push(format!("{} {:>8}  {}", ft, size, name));
                }
                entries.sort();
                Ok(ToolResult {
                    success: true,
                    output: entries.join("\n"),
                    error: None,
                })
            }
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to list {}: {}", path, e)),
            }),
        }
    }
}

// ── search_files ───────────────────────────────────────────

pub struct SearchFiles;
impl Tool for SearchFiles {
    fn name(&self) -> &str { "search_files" }
    fn description(&self) -> &str { "Pattern search across files" }
    fn safety_level(&self) -> SafetyLevel { SafetyLevel::Safe }

    fn execute(&self, params: &Value) -> anyhow::Result<ToolResult> {
        let path = params["path"].as_str().unwrap_or(".");
        let pattern = params["pattern"].as_str().unwrap_or("");
        let mut matches = Vec::new();

        fn search_recursive(dir: &str, pattern: &str, matches: &mut Vec<String>, depth: u32) {
            if depth > 10 { return; }
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        search_recursive(&path.to_string_lossy(), pattern, matches, depth + 1);
                    } else if let Ok(content) = std::fs::read_to_string(&path) {
                        for (i, line) in content.lines().enumerate() {
                            if line.contains(pattern) {
                                matches.push(format!("{}:{}: {}", path.display(), i + 1, line.trim()));
                                if matches.len() >= 100 { return; }
                            }
                        }
                    }
                }
            }
        }

        search_recursive(path, pattern, &mut matches, 0);
        Ok(ToolResult {
            success: true,
            output: if matches.is_empty() {
                format!("No matches for '{}' in {}", pattern, path)
            } else {
                matches.join("\n")
            },
            error: None,
        })
    }
}

// ── run_command ─────────────────────────────────────────────

pub struct RunCommand;
impl Tool for RunCommand {
    fn name(&self) -> &str { "run_command" }
    fn description(&self) -> &str { "Execute verified shell sub-process" }
    fn safety_level(&self) -> SafetyLevel { SafetyLevel::Confirm }

    fn execute(&self, params: &Value) -> anyhow::Result<ToolResult> {
        let command = params["command"].as_str().unwrap_or("");
        let _timeout_secs = params["timeout"].as_u64().unwrap_or(30);

        let output = std::process::Command::new("/bin/sh")
            .args(["-c", command])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let combined = if stderr.is_empty() {
            stdout
        } else {
            format!("{}\n--- stderr ---\n{}", stdout, stderr)
        };

        Ok(ToolResult {
            success: output.status.success(),
            output: combined,
            error: if !output.status.success() {
                Some(format!("Exit code: {:?}", output.status.code()))
            } else {
                None
            },
        })
    }
}

// ── get_system_info ────────────────────────────────────────

pub struct GetSystemInfo;
impl Tool for GetSystemInfo {
    fn name(&self) -> &str { "get_system_info" }
    fn description(&self) -> &str { "Query CPU, RAM, disk, OS info" }
    fn safety_level(&self) -> SafetyLevel { SafetyLevel::Safe }

    fn execute(&self, _params: &Value) -> anyhow::Result<ToolResult> {
        let mut info = HashMap::new();

        if let Ok(ver) = std::fs::read_to_string("/proc/version") {
            info.insert("kernel", ver.trim().to_string());
        }

        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            let mut mem_total = 0u64;
            let mut mem_avail = 0u64;
            for line in meminfo.lines() {
                if line.starts_with("MemTotal:") {
                    mem_total = line.split_whitespace().nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
                } else if line.starts_with("MemAvailable:") {
                    mem_avail = line.split_whitespace().nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
                }
            }
            info.insert("mem_total_mb", format!("{}", mem_total / 1024));
            info.insert("mem_used_mb", format!("{}", (mem_total - mem_avail) / 1024));
        }

        if let Ok(uptime) = std::fs::read_to_string("/proc/uptime") {
            if let Some(secs) = uptime.split_whitespace().next() {
                info.insert("uptime_seconds", secs.to_string());
            }
        }

        if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
            let cores = cpuinfo.lines().filter(|l| l.starts_with("processor")).count();
            let model = cpuinfo.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split(':').nth(1))
                .unwrap_or("unknown")
                .trim();
            info.insert("cpu_model", model.to_string());
            info.insert("cpu_cores", cores.to_string());
        }

        if let Ok(hostname) = std::fs::read_to_string("/etc/hostname") {
            info.insert("hostname", hostname.trim().to_string());
        }

        let output = info.iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolResult { success: true, output, error: None })
    }
}

// ── get_network_status ─────────────────────────────────────

pub struct GetNetworkStatus;
impl Tool for GetNetworkStatus {
    fn name(&self) -> &str { "get_network_status" }
    fn description(&self) -> &str { "Ping, DNS, interface status" }
    fn safety_level(&self) -> SafetyLevel { SafetyLevel::Safe }

    fn execute(&self, _params: &Value) -> anyhow::Result<ToolResult> {
        let output = std::process::Command::new("ip")
            .args(["addr", "show"])
            .output();

        let network_info = match output {
            Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
            Err(_) => "ip command not available".to_string(),
        };

        let dns_ok = std::process::Command::new("nslookup")
            .arg("dns.google")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        let result = format!(
            "Network Interfaces:\n{}\n\nDNS Resolution: {}",
            network_info,
            if dns_ok { "OK" } else { "FAILED" }
        );

        Ok(ToolResult { success: true, output: result, error: None })
    }
}

// ── manage_service ─────────────────────────────────────────

pub struct ManageService;
impl Tool for ManageService {
    fn name(&self) -> &str { "manage_service" }
    fn description(&self) -> &str { "Control system processes/init" }
    fn safety_level(&self) -> SafetyLevel { SafetyLevel::Confirm }

    fn execute(&self, params: &Value) -> anyhow::Result<ToolResult> {
        let service = params["service"].as_str().unwrap_or("");
        let action = params["action"].as_str().unwrap_or("status");

        let script = format!("/etc/init.d/{}", service);
        if !std::path::Path::new(&script).exists() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Service script not found: {}", script)),
            });
        }

        let output = std::process::Command::new(&script)
            .arg(action)
            .output()?;

        Ok(ToolResult {
            success: output.status.success(),
            output: String::from_utf8_lossy(&output.stdout).to_string(),
            error: if !output.status.success() {
                Some(String::from_utf8_lossy(&output.stderr).to_string())
            } else {
                None
            },
        })
    }
}

// ── partition_disk ─────────────────────────────────────────

pub struct PartitionDisk;
impl Tool for PartitionDisk {
    fn name(&self) -> &str { "partition_disk" }
    fn description(&self) -> &str { "Partition target storage block" }
    fn safety_level(&self) -> SafetyLevel { SafetyLevel::Dangerous }

    fn execute(&self, params: &Value) -> anyhow::Result<ToolResult> {
        let device = params["device"].as_str().unwrap_or("");

        if device.is_empty() || !device.starts_with("/dev/") {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Invalid device path. Must start with /dev/".to_string()),
            });
        }

        Ok(ToolResult {
            success: false,
            output: String::new(),
            error: Some("Use niat-installer for disk partitioning operations".to_string()),
        })
    }
}
