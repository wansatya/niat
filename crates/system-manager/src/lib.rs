//! NIAT System Manager
//! Hardware, network, and service status supervisor.

use anyhow::Result;
use niat_common::types::SystemStatus;
use tracing::{info, warn};

/// System manager collects and monitors hardware/network status.
pub struct SystemManager;

impl SystemManager {
    pub fn new() -> Self {
        Self
    }

    /// Collect current system status snapshot.
    pub fn status(&self) -> SystemStatus {
        SystemStatus {
            kernel_version: self.kernel_version(),
            network_interface: self.primary_interface(),
            network_ip: self.primary_ip(),
            agent_ready: false, // Set by agent-kernel
            model_online: false, // Set after model health check
            uptime_secs: self.uptime(),
            mem_used_mb: self.mem_used(),
            mem_total_mb: self.mem_total(),
            disk_used_gb: self.disk_used(),
            disk_total_gb: self.disk_total(),
        }
    }

    fn kernel_version(&self) -> String {
        std::fs::read_to_string("/proc/version")
            .ok()
            .and_then(|v| v.split_whitespace().nth(2).map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown".into())
    }

    fn primary_interface(&self) -> Option<String> {
        // Check /sys/class/net for non-lo interfaces
        if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name != "lo" {
                    // Check if interface is UP
                    let operstate = format!("/sys/class/net/{}/operstate", name);
                    if let Ok(state) = std::fs::read_to_string(&operstate) {
                        if state.trim() == "up" {
                            return Some(name);
                        }
                    }
                }
            }
            // Return first non-lo interface even if not up
            if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name != "lo" {
                        return Some(name);
                    }
                }
            }
        }
        None
    }

    fn primary_ip(&self) -> Option<String> {
        // Parse ip addr output to find primary IP
        let output = std::process::Command::new("ip")
            .args(["-4", "addr", "show"])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("inet ") && !trimmed.contains("127.0.0.1") {
                return trimmed.split_whitespace()
                    .nth(1)
                    .and_then(|s| s.split('/').next())
                    .map(|s| s.to_string());
            }
        }
        None
    }

    fn uptime(&self) -> u64 {
        std::fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|s| s.split_whitespace().next()?.parse::<f64>().ok())
            .map(|f| f as u64)
            .unwrap_or(0)
    }

    fn mem_total(&self) -> u64 {
        self.parse_meminfo("MemTotal:").unwrap_or(0) / 1024
    }

    fn mem_used(&self) -> u64 {
        let total = self.parse_meminfo("MemTotal:").unwrap_or(0);
        let avail = self.parse_meminfo("MemAvailable:").unwrap_or(0);
        (total.saturating_sub(avail)) / 1024
    }

    fn parse_meminfo(&self, key: &str) -> Option<u64> {
        let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
        for line in meminfo.lines() {
            if line.starts_with(key) {
                return line.split_whitespace().nth(1)?.parse().ok();
            }
        }
        None
    }

    fn disk_used(&self) -> u64 {
        self.disk_info().map(|(used, _)| used).unwrap_or(0)
    }

    fn disk_total(&self) -> u64 {
        self.disk_info().map(|(_, total)| total).unwrap_or(0)
    }

    fn disk_info(&self) -> Option<(u64, u64)> {
        let output = std::process::Command::new("df")
            .args(["-BG", "/"])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        let line = text.lines().nth(1)?;
        let parts: Vec<&str> = line.split_whitespace().collect();
        let total = parts.get(1)?.trim_end_matches('G').parse().ok()?;
        let used = parts.get(2)?.trim_end_matches('G').parse().ok()?;
        Some((used, total))
    }

    /// Check network connectivity.
    pub fn check_connectivity(&self) -> bool {
        std::process::Command::new("ping")
            .args(["-c", "1", "-W", "3", "8.8.8.8"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Initialize network via DHCP.
    pub fn init_network(&self) -> Result<()> {
        if let Some(iface) = self.primary_interface() {
            info!(interface = %iface, "Bringing up network interface");
            let _ = std::process::Command::new("ip")
                .args(["link", "set", &iface, "up"])
                .output();

            info!(interface = %iface, "Starting DHCP");
            let _ = std::process::Command::new("udhcpc")
                .args(["-i", &iface, "-n", "-q"])
                .output();
        } else {
            warn!("No network interfaces found");
        }
        Ok(())
    }
}

impl Default for SystemManager {
    fn default() -> Self {
        Self::new()
    }
}
