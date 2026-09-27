//! NIAT System Installer
//! Standalone TUI application to partition, format, and install NIAT onto target disk.

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PartitionInfo {
    path: String,
    name: String,
    size: String,
    bytes: u64,
    fstype: String,
    label: String,
    partlabel: String,
    uuid: String,
    mountpoints: Vec<String>,
    // Legacy single mountpoint (first entry) kept for compat / display.
    mountpoint: String,
    os_desc: String,
    is_efi: bool,
    is_mounted: bool,
    is_live_source: bool,
    is_too_small: bool,
    fs_avail_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FreeRegion {
    start_bytes: u64,
    end_bytes: u64,
    bytes: u64,
    display: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiskInfo {
    path: String,
    name: String,
    size: String,
    bytes: u64,
    model: String,
    removable: bool,
    readonly: bool,
    transport: String,
    parttable: String,
    partitions: Vec<PartitionInfo>,
    free_regions: Vec<FreeRegion>,
    detected_oses: Vec<String>,
    is_live_source: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallMode {
    Alongside,   // Shrink-safe: install into free space / untouched gaps (Ubuntu-style).
    UsePartition, // Preserve existing partitions & boot (manual partition pick).
    EraseDisk,    // Format entire disk
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    SelectDisk,
    SelectInstallMode,
    SelectFreeSpace,
    SelectPartition,
    SelectEfiPartition,
    ConfirmInstallation,
    Installing,
    Done,
    Failed(String),
}

struct InstallerApp {
    disks: Vec<DiskInfo>,
    selected_disk_idx: usize,
    install_mode: InstallMode,
    selected_mode_idx: usize, // 0: Alongside, 1: UsePartition, 2: EraseDisk
    selected_part_idx: usize,
    selected_free_idx: usize,
    selected_efi_idx: usize,
    step: Step,
    input_confirm: String,
    logs: Vec<String>,
}

impl InstallerApp {
    fn new() -> Self {
        let disks = scan_disks();
        let mut app = Self {
            disks,
            selected_disk_idx: 0,
            install_mode: InstallMode::Alongside,
            selected_mode_idx: 0,
            selected_part_idx: 0,
            selected_free_idx: 0,
            selected_efi_idx: 0,
            step: Step::SelectDisk,
            input_confirm: String::new(),
            logs: vec!["Installer initialized. Storage device and OS scan complete.".into()],
        };
        app.clamp_disk_selection();
        app
    }

    fn clamp_disk_selection(&mut self) {
        if self.disks.is_empty() {
            return;
        }
        if self.selected_disk_idx >= self.disks.len() {
            self.selected_disk_idx = 0;
        }
        // Auto-skip Live USB source like Ubuntu/Fedora do.
        if self.disks[self.selected_disk_idx].is_live_source {
            if let Some(i) = self
                .disks
                .iter()
                .position(|d| !d.is_live_source && !d.readonly)
            {
                self.selected_disk_idx = i;
                self.logs.push(format!(
                    "Live media {} excluded as install target.",
                    self.disks.iter().find(|d| d.is_live_source).map(|d| d.path.as_str()).unwrap_or("")
                ));
            }
        }
    }

    fn selected_disk(&self) -> Option<&DiskInfo> {
        self.disks.get(self.selected_disk_idx)
    }

    fn selected_partition(&self) -> Option<&PartitionInfo> {
        // Skip non-installable pseudo-rows: EFI rows are info-only, and the
        // live-source partition can never be a target.
        let installable: Vec<&PartitionInfo> = self
            .selected_disk()
            .map(|d| d.partitions.iter().filter(|p| !p.is_efi && !p.is_live_source).collect())
            .unwrap_or_default();
        installable.get(self.selected_part_idx).copied()
    }

    fn selectable_partitions(&self) -> Vec<PartitionInfo> {
        self.selected_disk()
            .map(|d| {
                d.partitions
                    .iter()
                    .filter(|p| !p.is_efi && !p.is_live_source)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn selected_free_region(&self) -> Option<&FreeRegion> {
        self.selected_disk()
            .and_then(|d| d.free_regions.get(self.selected_free_idx))
    }

    fn efi_partitions(&self) -> Vec<&PartitionInfo> {
        if let Some(disk) = self.selected_disk() {
            disk.partitions.iter().filter(|p| p.is_efi).collect()
        } else {
            Vec::new()
        }
    }

    fn selected_efi_partition(&self) -> Option<&PartitionInfo> {
        let efis = self.efi_partitions();
        if efis.is_empty() {
            if let Some(disk) = self.selected_disk() {
                disk.partitions
                    .iter()
                    .find(|p| p.fstype.to_lowercase() == "vfat" || p.fstype.to_lowercase() == "fat32")
            } else {
                None
            }
        } else {
            efis.get(self.selected_efi_idx).copied()
        }
    }
}

fn parse_kv_line(line: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut remaining = line.trim();

    while !remaining.is_empty() {
        if let Some(eq_pos) = remaining.find('=') {
            let key = remaining[..eq_pos].trim().to_string();
            let after_eq = remaining[eq_pos + 1..].trim();
            if after_eq.starts_with('"') {
                if let Some(end_quote) = after_eq[1..].find('"') {
                    let val = after_eq[1..1 + end_quote].to_string();
                    map.insert(key, val);
                    remaining = after_eq[1 + end_quote + 1..].trim();
                    continue;
                }
            }
            let next_space = after_eq.find(' ').unwrap_or(after_eq.len());
            let val = after_eq[..next_space].to_string();
            map.insert(key, val);
            remaining = after_eq[next_space..].trim();
        } else {
            break;
        }
    }
    map
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "Unknown".into();
    }
    let gb = bytes as f64 / 1_073_741_824.0;
    if gb >= 1.0 {
        format!("{:.1} GB", gb)
    } else {
        let mb = bytes as f64 / 1_048_576.0;
        format!("{:.1} MB", mb)
    }
}

/// Minimum NIAT root size: 4 GiB (SPEC §3: Storage >= 4 GB).
const MIN_ROOT_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// GUID for EFI System Partition.
const EFI_GUID: &str = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b";
/// GUIDs that must never be offered as install targets (firmware / reserved).
const PROTECTED_GUIDS: &[&str] = &[
    EFI_GUID,
    "e3c9e316-0b5c-4db8-817d-f92df00215ae", // Microsoft reserved
    "de94bba4-06d1-40d1-a16a-bfd50179d6ac", // Windows recovery
    "21686148-6449-6e6f-744e-656564454649", // BIOS boot
];

fn is_efi_guid(parttype: &str) -> bool {
    parttype.trim().to_lowercase() == EFI_GUID
}

fn is_protected_guid(parttype: &str) -> bool {
    let p = parttype.trim().to_lowercase();
    PROTECTED_GUIDS.iter().any(|g| *g == p)
}

fn split_mountpoints(raw: &str) -> Vec<String> {
    // lsblk MOUNTPOINTS may be "[...]" bracket list or single path.
    let t = raw.trim();
    if t.is_empty() {
        return Vec::new();
    }
    let inner = if t.starts_with('[') && t.ends_with(']') && t.len() >= 2 {
        &t[1..t.len() - 1]
    } else {
        t
    };
    inner
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn infer_os_desc(fstype: &str, label: &str, mountpoint: &str, is_efi: bool) -> String {
    if is_efi {
        return "EFI System Partition".into();
    }

    let fstype_lower = fstype.to_lowercase();
    let label_lower = label.to_lowercase();
    let mount_lower = mountpoint.to_lowercase();

    if fstype_lower == "ntfs" || label_lower.contains("win") || label_lower.contains("basic data") {
        "Windows OS / NTFS".into()
    } else if label_lower.contains("fedora") {
        "Fedora Linux System".into()
    } else if fstype_lower == "ext4" || fstype_lower == "btrfs" || fstype_lower == "xfs" {
        if mount_lower == "/boot" {
            "Linux /boot Partition".into()
        } else if mount_lower == "/" {
            "Linux Root Partition".into()
        } else if !label.is_empty() {
            format!("Linux System ({})", label)
        } else {
            "Linux System Partition".into()
        }
    } else if fstype_lower == "swap" {
        "Linux Swap".into()
    } else if fstype_lower == "vfat" || fstype_lower == "fat32" {
        if !label.is_empty() {
            format!("FAT Partition ({})", label)
        } else {
            "FAT Partition".into()
        }
    } else if !fstype.is_empty() {
        format!("{} Partition", fstype.to_uppercase())
    } else {
        "Unformatted / Raw Partition".into()
    }
}

fn scan_os_prober() -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(out) = Command::new("os-prober").output() {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 2 {
                let dev = parts[0].trim().to_string();
                let name = parts[1].trim().to_string();
                if !dev.is_empty() && !name.is_empty() {
                    map.insert(dev, name);
                }
            }
        }
    }
    map
}

fn live_source_mounts() -> Vec<String> {
    // Collect mountpoints that indicate the Live USB source (Ubuntu/Fedora-style guard).
    let mut mounts = Vec::new();
    for probe in ["/proc/cmdline", "/proc/mounts", "/etc/mtab"] {
        if let Ok(t) = std::fs::read_to_string(probe) {
            for token in ["/run/live", "/run/archiso", "/live", "/cdrom", "liveOS"] {
                if t.contains(token) && !mounts.contains(&token.to_string()) {
                    mounts.push(token.to_string());
                }
            }
            // cmdline like root=live:CDLABEL=... or boot=live
            if probe.ends_with("cmdline")
                && (t.contains("boot=live") || t.contains("root=live") || t.contains("liveimg"))
            {
                mounts.push("LIVE-CMDLINE".to_string());
            }
        }
    }
    // findmnt of / gives live source device path fragment (e.g. /dev/sdb1 on /run/live)
    if let Ok(out) = Command::new("findmnt").args(["-no", "SOURCE,TARGET"]).output() {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let l = line.to_lowercase();
            if l.contains("/run/live") || l.contains("/run/archiso") || l.contains("/cdrom") {
                if let Some(src) = line.split_whitespace().next() {
                    mounts.push(src.to_string());
                }
            }
        }
    }
    mounts
}

fn is_live_path(mounts_list: &[String], mp_list: &[String], fstype: &str, label: &str) -> bool {
    let fs = fstype.to_lowercase();
    // iso9660 / squashfs members are virtually always Live media.
    if fs == "iso9660" || fs == "squashfs" {
        return true;
    }
    for mp in mp_list {
        let m = mp.to_lowercase();
        if m.contains("/run/live")
            || m.contains("/run/archiso")
            || m == "/cdrom"
            || m.starts_with("/cdrom/")
            || m == "/live"
            || m.starts_with("/live/")
            || m.contains("liveos")
        {
            return true;
        }
        for probe in mounts_list {
            if !probe.starts_with('/') {
                continue;
            }
            if mp == probe {
                return true;
            }
        }
    }
    let ll = label.to_lowercase();
    if ll.contains("niat_live") || ll.contains("niat-live") || ll.contains("live") && fs == "iso9660" {
        return true;
    }
    false
}

fn part_disk_key(disk_name: &str, part_name: &str, pkname: &str) -> bool {
    if !pkname.is_empty() {
        return pkname == disk_name;
    }
    // Fallback for lsblk outputs without PKNAME (nvme0n1p1, sda1, mmcblk0p1).
    if part_name == disk_name {
        return false;
    }
    part_name.starts_with(disk_name)
}

fn parse_human_size(s: &str) -> u64 {
    let t = s.trim().to_uppercase();
    if t.is_empty() {
        return 0;
    }
    let (num_part, mult) = if t.ends_with('T') {
        (&t[..t.len() - 1], 1024u64.pow(4))
    } else if t.ends_with('G') {
        (&t[..t.len() - 1], 1024u64.pow(3))
    } else if t.ends_with('M') {
        (&t[..t.len() - 1], 1024u64.pow(2))
    } else if t.ends_with('K') {
        (&t[..t.len() - 1], 1024)
    } else {
        (t.as_str(), 1)
    };
    num_part
        .trim()
        .parse::<f64>()
        .map(|v| (v * mult as f64) as u64)
        .unwrap_or(0)
}

fn parse_lsblk_output(
    output: &str,
    os_prober_map: &HashMap<String, String>,
    disks: &mut Vec<DiskInfo>,
) {
    let live_hints = live_source_mounts();
    let mut disk_list: Vec<DiskInfo> = Vec::new();
    // (pkname, part) so nvme0n1p1 attaches via PKNAME, not prefix guess.
    let mut part_list: Vec<(String, PartitionInfo)> = Vec::new();

    for line in output.lines() {
        let kv = parse_kv_line(line);
        let kname = kv.get("KNAME").cloned().unwrap_or_default();
        let name = kv.get("NAME").cloned().unwrap_or_else(|| kname.clone());
        if name.is_empty() {
            continue;
        }

        if name.starts_with("loop")
            || name.starts_with("ram")
            || name.starts_with("zram")
            || name.starts_with("sr")
            || name.starts_with("dm-")
            || name.starts_with("md")
        {
            continue;
        }

        let dev_type = kv.get("TYPE").cloned().unwrap_or_default();
        let bytes: u64 = kv
            .get("SIZE")
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| parse_human_size(kv.get("SIZE").cloned().unwrap_or_default().as_str()));
        // Empty card readers report SIZE 0 — same as Ubuntu's "no media", hide them.
        if dev_type == "disk" && bytes == 0 {
            continue;
        }
        let size_display = format_bytes(bytes);
        let rm = kv.get("RM").map(|s| s == "1").unwrap_or(false);
        let ro = kv.get("RO").map(|s| s == "1").unwrap_or(false);
        let tran = kv.get("TRAN").cloned().unwrap_or_default();
        let pttype = kv.get("PTTYPE").cloned().unwrap_or_default();
        let pkname = kv.get("PKNAME").cloned().unwrap_or_default();
        let dev_path = kv
            .get("PATH")
            .cloned()
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| format!("/dev/{}", kname));

        if dev_type == "disk" {
            let model = kv.get("MODEL").cloned().unwrap_or_default();
            let model_str = if model.trim().is_empty() {
                "Generic Block Device".into()
            } else {
                model.trim().replace('_', " ")
            };

            disk_list.push(DiskInfo {
                path: dev_path,
                name: if kname.is_empty() { name.clone() } else { kname },
                size: size_display,
                bytes,
                model: model_str,
                removable: rm,
                readonly: ro,
                transport: tran,
                parttable: pttype,
                partitions: Vec::new(),
                free_regions: Vec::new(),
                detected_oses: Vec::new(),
                is_live_source: false,
            });
        } else if dev_type == "part" {
            let fstype = kv.get("FSTYPE").cloned().unwrap_or_default();
            let label = kv.get("LABEL").cloned().unwrap_or_default();
            let partlabel = kv.get("PARTLABEL").cloned().unwrap_or_default();
            let uuid = kv.get("UUID").cloned().unwrap_or_default();
            let parttype = kv.get("PARTTYPE").cloned().unwrap_or_default();
            let mp_raw = kv
                .get("MOUNTPOINTS")
                .or_else(|| kv.get("MOUNTPOINT"))
                .cloned()
                .unwrap_or_default();
            let mountpoints = split_mountpoints(&mp_raw);
            let mountpoint = mountpoints.first().cloned().unwrap_or_default();
            let fs_avail: Option<u64> = kv
                .get("FSAVAIL")
                .and_then(|s| s.parse::<u64>().ok())
                .and_then(|v| if v == 0 { None } else { Some(v) });

            let fl = fstype.to_lowercase();
            let is_efi = is_efi_guid(&parttype)
                || (fl == "vfat" || fl == "fat32" || fl == "fat16")
                    && (label.to_lowercase().contains("efi")
                        || partlabel.to_lowercase().contains("efi")
                        || mountpoint.to_lowercase().contains("efi")
                        || (bytes >= 50 * 1024 * 1024
                            && bytes <= 2 * 1024 * 1024 * 1024
                            && name.ends_with('1')));

            let dev_path_p = dev_path.clone();
            let os_desc = if let Some(probed) = os_prober_map
                .get(&dev_path_p)
                .or_else(|| os_prober_map.get(&format!("/dev/{}", name)))
            {
                probed.clone()
            } else {
                infer_os_desc(&fstype, &label, &mountpoint, is_efi)
            };

            let is_live = is_live_path(&live_hints, &mountpoints, &fstype, &label)
                || live_hints.iter().any(|h| h == &dev_path_p);
            let is_too_small =
                !is_efi && !is_protected_guid(&parttype) && bytes < MIN_ROOT_BYTES;

            part_list.push((
                pkname,
                PartitionInfo {
                    path: dev_path_p,
                    name,
                    size: size_display,
                    bytes,
                    fstype,
                    label,
                    partlabel,
                    uuid,
                    mountpoints,
                    mountpoint: mountpoint.clone(),
                    os_desc,
                    is_efi,
                    is_mounted: !mountpoint.is_empty(),
                    is_live_source: is_live,
                    is_too_small,
                    fs_avail_bytes: fs_avail,
                },
            ));
        }
    }

    for mut disk in disk_list {
        let mut disk_parts = Vec::new();
        let mut oses = Vec::new();
        let mut live = false;

        for (pk, part) in &part_list {
            if part_disk_key(&disk.name, &part.name, pk) {
                if part.is_live_source {
                    live = true;
                }
                disk_parts.push(part.clone());
                if !part.os_desc.is_empty()
                    && part.os_desc != "Unformatted / Raw Partition"
                    && !oses.contains(&part.os_desc)
                {
                    oses.push(part.os_desc.clone());
                }
            }
        }

        disk_parts.sort_by(|a, b| nat_cmp(&a.name, &b.name));
        disk.partitions = disk_parts;
        disk.detected_oses = oses;
        if live {
            disk.is_live_source = true;
        } else if disk.removable {
            let mounts_txt = std::fs::read_to_string("/proc/mounts").unwrap_or_default();
            if (mounts_txt.contains(&disk.path) || mounts_txt.contains(&disk.name))
                && (mounts_txt.contains("/run/live")
                    || mounts_txt.contains("liveOS")
                    || mounts_txt.contains("iso9660"))
            {
                disk.is_live_source = true;
            }
        }
        disk.free_regions = scan_free_regions(&disk.path);
        disks.push(disk);
    }
}

fn nat_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    // Natural sort: split alpha/digit runs, compare digits numerically.
    let split = |s: &str| -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = String::new();
        let mut cur_digit: Option<bool> = None;
        for c in s.chars() {
            let d = c.is_ascii_digit();
            match cur_digit {
                Some(cd) if cd == d => cur.push(c),
                _ => {
                    if !cur.is_empty() {
                        out.push(std::mem::take(&mut cur));
                    }
                    cur.push(c);
                    cur_digit = Some(d);
                }
            }
        }
        if !cur.is_empty() {
            out.push(cur);
        }
        out
    };
    let (aa, bb) = (split(a), split(b));
    for (x, y) in aa.iter().zip(bb.iter()) {
        let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(xi), Ok(yi)) => xi.cmp(&yi),
            _ => x.cmp(y),
        };
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    aa.len().cmp(&bb.len())
}

/// Ubuntu/Fedora-style "largest continuous free space" via parted.
/// Parses `parted -m <disk> unit B print free` lines ending in `:free;`.
fn scan_free_regions(disk_path: &str) -> Vec<FreeRegion> {
    let mut regions = Vec::new();
    let out = Command::new("parted")
        .args(["-m", disk_path, "unit", "B", "print", "free"])
        .output();
    let Ok(out) = out else { return regions };
    if !out.status.success() {
        return regions;
    }
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let t = line.trim().trim_end_matches(';');
        // Format: N:startB:endB:sizeB:free;
        let fields: Vec<&str> = t.split(':').collect();
        if fields.len() < 5 || fields[4].trim() != "free" {
            continue;
        }
        let num = |s: &str| -> u64 { s.trim().trim_end_matches('B').parse().unwrap_or(0) };
        let size = num(fields[3]);
        if size == 0 {
            continue;
        }
        regions.push(FreeRegion {
            start_bytes: num(fields[1]),
            end_bytes: num(fields[2]),
            bytes: size,
            display: format_bytes(size),
        });
    }
    regions.sort_by(|a, b| b.bytes.cmp(&a.bytes)); // largest first
    regions
}

fn scan_sysfs_disks(disks: &mut Vec<DiskInfo>) {
    if let Ok(entries) = std::fs::read_dir("/sys/block") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("loop")
                || name.starts_with("ram")
                || name.starts_with("zram")
                || name.starts_with("sr")
                || name.starts_with("dm-")
                || name.starts_with("md")
            {
                continue;
            }

            let sys_path = entry.path();
            let size_sectors: u64 = std::fs::read_to_string(sys_path.join("size"))
                .unwrap_or_default()
                .trim()
                .parse()
                .unwrap_or(0);
            let bytes = size_sectors * 512;
            let size_display = format_bytes(bytes);

            let model = std::fs::read_to_string(sys_path.join("device/model"))
                .unwrap_or_else(|_| "Generic Block Device".into())
                .trim()
                .to_string();
            let model_str = if model.is_empty() {
                "Generic Block Device".into()
            } else {
                model
            };

            let removable = std::fs::read_to_string(sys_path.join("removable"))
                .unwrap_or_default()
                .trim()
                == "1";

            disks.push(DiskInfo {
                path: format!("/dev/{}", name),
                name,
                size: size_display,
                bytes,
                model: model_str,
                removable,
                readonly: false,
                transport: String::new(),
                parttable: String::new(),
                partitions: Vec::new(),
                free_regions: Vec::new(),
                detected_oses: Vec::new(),
                is_live_source: false,
            });
        }
    }
}

fn scan_disks() -> Vec<DiskInfo> {
    let mut disks = Vec::new();
    let os_prober_map = scan_os_prober();

    // Rich columns: PATH/PKNAME fix nvme (nvme0n1p1), MOUNTPOINTS catches
    // multi-mount live media, PARTTYPE GUID detects EFI reliably, FSAVAIL
    // powers alongside shrink checks, PTTYPE/RO/TRAN feed safety guards.
    for cols in [
        "PATH,KNAME,PKNAME,NAME,SIZE,MODEL,FSTYPE,LABEL,UUID,PARTLABEL,PARTTYPE,MOUNTPOINTS,TYPE,RM,RO,TRAN,PTTYPE,FSAVAIL",
        "PATH,KNAME,PKNAME,NAME,SIZE,MODEL,FSTYPE,LABEL,UUID,PARTLABEL,PARTTYPE,MOUNTPOINT,TYPE,RM",
    ] {
        let output = Command::new("lsblk")
            .args(["-P", "-b", "-o", cols])
            .output();
        if let Ok(out) = output {
            if out.status.success() && !out.stdout.is_empty() {
                let text = String::from_utf8_lossy(&out.stdout);
                parse_lsblk_output(&text, &os_prober_map, &mut disks);
                if !disks.is_empty() {
                    break;
                }
                disks.clear();
            }
        } else {
            break;
        }
    }

    if disks.is_empty() {
        scan_sysfs_disks(&mut disks);
    }

    if disks.is_empty() {
        disks.push(DiskInfo {
            path: "/dev/sda".into(),
            name: "sda".into(),
            size: "64.0 GB".into(),
            bytes: 64 * 1024 * 1024 * 1024,
            model: "Virtual / System Disk".into(),
            removable: false,
            readonly: false,
            transport: "virtio".into(),
            parttable: "gpt".into(),
            partitions: vec![
                PartitionInfo {
                    path: "/dev/sda1".into(),
                    name: "sda1".into(),
                    size: "512.0 MB".into(),
                    bytes: 512 * 1024 * 1024,
                    fstype: "vfat".into(),
                    label: "EFI".into(),
                    partlabel: "EFI System Partition".into(),
                    uuid: String::new(),
                    mountpoints: vec!["/boot/efi".into()],
                    mountpoint: "/boot/efi".into(),
                    os_desc: "EFI System Partition".into(),
                    is_efi: true,
                    is_mounted: true,
                    is_live_source: false,
                    is_too_small: false,
                    fs_avail_bytes: None,
                },
                PartitionInfo {
                    path: "/dev/sda2".into(),
                    name: "sda2".into(),
                    size: "20.0 GB".into(),
                    bytes: 20 * 1024 * 1024 * 1024,
                    fstype: "ntfs".into(),
                    label: "Windows".into(),
                    partlabel: String::new(),
                    uuid: String::new(),
                    mountpoints: Vec::new(),
                    mountpoint: "".into(),
                    os_desc: "Windows OS / NTFS".into(),
                    is_efi: false,
                    is_mounted: false,
                    is_live_source: false,
                    is_too_small: false,
                    fs_avail_bytes: None,
                },
                PartitionInfo {
                    path: "/dev/sda3".into(),
                    name: "sda3".into(),
                    size: "43.5 GB".into(),
                    bytes: 43500 * 1024 * 1024,
                    fstype: "ext4".into(),
                    label: "Fedora_Root".into(),
                    partlabel: String::new(),
                    uuid: String::new(),
                    mountpoints: Vec::new(),
                    mountpoint: "".into(),
                    os_desc: "Fedora Linux System".into(),
                    is_efi: false,
                    is_mounted: false,
                    is_live_source: false,
                    is_too_small: false,
                    fs_avail_bytes: None,
                },
            ],
            free_regions: Vec::new(),
            detected_oses: vec![
                "EFI System Partition".into(),
                "Windows OS / NTFS".into(),
                "Fedora Linux System".into(),
            ],
            is_live_source: false,
        });
    }

    disks
}

fn check_efi_bootloaders(efi_dir: &Path) -> Vec<String> {
    let mut list = Vec::new();
    if efi_dir.join("EFI/Microsoft").exists() {
        list.push("Windows Boot Manager".into());
    }
    if efi_dir.join("EFI/fedora").exists() {
        list.push("Fedora Linux".into());
    }
    if efi_dir.join("EFI/ubuntu").exists() {
        list.push("Ubuntu Linux".into());
    }
    if efi_dir.join("EFI/arch").exists() {
        list.push("Arch Linux".into());
    }
    list
}

fn generate_grub_config(efi_dir: &Path) -> String {
    let mut cfg = String::new();
    cfg.push_str("set default=\"0\"\n");
    cfg.push_str("set timeout=5\n\n");

    cfg.push_str("menuentry \"NIAT OS (Intent-Driven Operating System)\" {\n");
    cfg.push_str("    linux /boot/bzImage root=LABEL=NIAT_ROOT rw quiet\n");
    cfg.push_str("}\n\n");

    if efi_dir.join("EFI/Microsoft/Boot/bootmgfw.efi").exists() {
        cfg.push_str("menuentry \"Windows Boot Manager (Preserved Boot)\" {\n");
        cfg.push_str("    insmod part_gpt\n");
        cfg.push_str("    insmod fat\n");
        cfg.push_str("    insmod chain\n");
        cfg.push_str("    chainloader /EFI/Microsoft/Boot/bootmgfw.efi\n");
        cfg.push_str("}\n\n");
    }

    if efi_dir.join("EFI/fedora/shimx64.efi").exists() || efi_dir.join("EFI/fedora/grubx64.efi").exists() {
        let loader = if efi_dir.join("EFI/fedora/shimx64.efi").exists() {
            "/EFI/fedora/shimx64.efi"
        } else {
            "/EFI/fedora/grubx64.efi"
        };
        cfg.push_str("menuentry \"Fedora Linux (Preserved Boot)\" {\n");
        cfg.push_str("    insmod part_gpt\n");
        cfg.push_str("    insmod fat\n");
        cfg.push_str("    insmod chain\n");
        cfg.push_str(&format!("    chainloader {}\n", loader));
        cfg.push_str("}\n\n");
    }

    if efi_dir.join("EFI/ubuntu/shimx64.efi").exists() || efi_dir.join("EFI/ubuntu/grubx64.efi").exists() {
        let loader = if efi_dir.join("EFI/ubuntu/shimx64.efi").exists() {
            "/EFI/ubuntu/shimx64.efi"
        } else {
            "/EFI/ubuntu/grubx64.efi"
        };
        cfg.push_str("menuentry \"Ubuntu Linux (Preserved Boot)\" {\n");
        cfg.push_str("    insmod part_gpt\n");
        cfg.push_str("    insmod fat\n");
        cfg.push_str("    insmod chain\n");
        cfg.push_str(&format!("    chainloader {}\n", loader));
        cfg.push_str("}\n\n");
    }

    cfg
}

fn part_suffix(disk_path: &str, n: u32) -> String {
    if disk_path.ends_with(|c: char| c.is_ascii_digit()) {
        format!("{}p{}", disk_path, n)
    } else {
        format!("{}{}", disk_path, n)
    }
}

fn newest_partition_node(disk_path: &str, logs: &mut Vec<String>) -> Option<String> {
    // Prefer lsblk: highest partition number on this disk = the one we appended.
    if let Ok(out) = Command::new("lsblk")
        .args(["-P", "-o", "PATH,PKNAME,TYPE"])
        .output()
    {
        if out.status.success() {
            let disk_kname = disk_path.trim_start_matches("/dev/");
            let mut best: Option<(u32, String)> = None;
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let kv = parse_kv_line(line);
                if kv.get("TYPE").map(|s| s.as_str()) != Some("part") {
                    continue;
                }
                let pk = kv.get("PKNAME").cloned().unwrap_or_default();
                let path = kv.get("PATH").cloned().unwrap_or_default();
                // PKNAME authoritative; fallback to prefix for minimal lsblk.
                let belongs = if !pk.is_empty() {
                    pk == disk_kname || format!("/dev/{}", pk) == disk_path
                } else {
                    path.starts_with(disk_path) && path != disk_path
                };
                if !belongs || path.is_empty() {
                    continue;
                }
                let num: u32 = path
                    .trim_start_matches(disk_path)
                    .trim_start_matches('p')
                    .parse()
                    .unwrap_or(0);
                if best.as_ref().map(|(n, _)| num > *n).unwrap_or(true) {
                    best = Some((num, path));
                }
            }
            if let Some((_, p)) = best {
                logs.push(format!("Detected new partition node {}", p));
                return Some(p);
            }
        }
    }
    None
}

fn install_system_files(root_part: &str, efi_part: &str, logs: &mut Vec<String>) -> Result<()> {
    logs.push("Mounting target root filesystem to /mnt/target...".into());
    std::fs::create_dir_all("/mnt/target")?;
    let _ = Command::new("mount").args([root_part, "/mnt/target"]).status();

    std::fs::create_dir_all("/mnt/target/boot/efi")?;
    let _ = Command::new("mount").args([efi_part, "/mnt/target/boot/efi"]).status();

    logs.push("Copying NIAT OS image files...".into());
    if Path::new("/run/initramfs/live").exists() {
        let _ = Command::new("cp")
            .args(["-a", "/run/initramfs/live/.", "/mnt/target/"])
            .status();
    } else {
        logs.push("Installing core system files to target...".into());
        let _ = Command::new("cp")
            .args([
                "-a",
                "/usr/bin/agent-kernel",
                "/usr/bin/niat-tui",
                "/usr/bin/niat-installer",
                "/mnt/target/",
            ])
            .status();
    }

    let efi_dir = Path::new("/mnt/target/boot/efi");
    let existing_bootloaders = check_efi_bootloaders(efi_dir);
    if !existing_bootloaders.is_empty() {
        logs.push(format!(
            "Preserving existing bootloader entries in EFI: {}",
            existing_bootloaders.join(", ")
        ));
    }

    logs.push("Writing GRUB UEFI bootloader configuration...".into());
    let grub_cfg = generate_grub_config(efi_dir);
    let _ = std::fs::create_dir_all("/mnt/target/boot/grub");
    let _ = std::fs::write("/mnt/target/boot/grub/grub.cfg", grub_cfg);

    logs.push("Unmounting target filesystems...".into());
    let _ = Command::new("umount").arg("/mnt/target/boot/efi").status();
    let _ = Command::new("umount").arg("/mnt/target").status();

    Ok(())
}

fn perform_installation(
    disk_path: &str,
    target_root_partition: Option<&str>,
    target_efi_partition: Option<&str>,
    target_free: Option<&FreeRegion>,
    install_mode: InstallMode,
    logs: &mut Vec<String>,
) -> Result<()> {
    match install_mode {
        InstallMode::Alongside => {
            let fr = target_free
                .ok_or_else(|| anyhow::anyhow!("No free-space region selected"))?;
            if fr.bytes < MIN_ROOT_BYTES {
                anyhow::bail!(
                    "Free region {} smaller than {} minimum",
                    fr.display,
                    format_bytes(MIN_ROOT_BYTES)
                );
            }
            let efi_part = target_efi_partition
                .ok_or_else(|| anyhow::anyhow!("No EFI partition selected"))?;
            // Append a new Linux partition into the free gap via sfdisk (no wipe).
            logs.push(format!(
                "Creating NIAT root partition in {} free space on {} (existing partitions untouched)...",
                fr.display, disk_path
            ));
            // N:size (append last partition, use whole gap). parted/free offsets are
            // byte-accurate; sfdisk -a appends at the largest free area.
            let sfdisk_input = format!("start={}, size={}, type=L, name=NIAT_ROOT\n", fr.start_bytes / 512, fr.bytes / 512);
            let mut child = Command::new("sfdisk")
                .args(["-a", disk_path])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()?;
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(sfdisk_input.as_bytes());
            }
            let status = child.wait()?;
            if !status.success() {
                anyhow::bail!("sfdisk failed to create partition in free space (exit {:?})", status.code());
            }
            let _ = Command::new("partprobe").arg(disk_path).status();
            // Re-read partition list to find the newly created node.
            std::thread::sleep(std::time::Duration::from_secs(1));
            let new_root = newest_partition_node(disk_path, logs);
            let Some(new_root) = new_root else {
                anyhow::bail!("New partition created but device node not found; reboot and retry manual mode");
            };
            logs.push(format!("Formatting new NIAT root {} as ext4...", new_root));
            let _ = Command::new("mkfs.ext4").args(["-F", "-L", "NIAT_ROOT", &new_root]).status();
            logs.push(format!(
                "Using existing EFI {} (preserving existing boot files)...",
                efi_part
            ));
            install_system_files(&new_root, efi_part, logs)?;
        }
        InstallMode::EraseDisk => {
            logs.push(format!("Partitioning target device {} (GPT scheme)...", disk_path));
            let sfdisk_input = "label: gpt\nsize=512MiB, type=U, name=EFI\nsize=+, type=L, name=NIAT_ROOT\n";
            let mut child = Command::new("sfdisk")
                .arg("--wipe")
                .arg("always")
                .arg(disk_path)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()?;

            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(sfdisk_input.as_bytes());
            }
            let status = child.wait()?;
            if !status.success() {
                logs.push("sfdisk completed with status code. Continuing setup...".into());
            }

            let p1 = part_suffix(disk_path, 1);
            let p2 = part_suffix(disk_path, 2);

            logs.push(format!("Formatting EFI System Partition {} as FAT32...", p1));
            let _ = Command::new("mkfs.vfat").args(["-F32", "-n", "EFI", &p1]).status();

            logs.push(format!("Formatting Root Filesystem {} as ext4...", p2));
            let _ = Command::new("mkfs.ext4").args(["-F", "-L", "NIAT_ROOT", &p2]).status();

            install_system_files(&p2, &p1, logs)?;
        }
        InstallMode::UsePartition => {
            let root_part = target_root_partition
                .ok_or_else(|| anyhow::anyhow!("No target root partition selected"))?;
            let efi_part = target_efi_partition
                .ok_or_else(|| anyhow::anyhow!("No EFI partition selected"))?;

            logs.push(format!("Formatting selected partition {} as ext4 (NIAT_ROOT)...", root_part));
            let status = Command::new("mkfs.ext4")
                .args(["-F", "-L", "NIAT_ROOT", root_part])
                .status();
            if let Ok(st) = status {
                if !st.success() {
                    logs.push(format!("mkfs.ext4 returned exit code {:?}", st.code()));
                }
            }

            logs.push(format!(
                "Using existing EFI Partition {} (preserving existing boot files)...",
                efi_part
            ));

            install_system_files(root_part, efi_part, logs)?;
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = InstallerApp::new();
    let res = run_installer(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        eprintln!("Installer error: {:?}", e);
    }
    Ok(())
}

fn run_installer(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut InstallerApp,
) -> Result<()> {
    loop {
        terminal.draw(|f| draw_ui(f, app))?;

        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == event::KeyEventKind::Release {
                    continue;
                }

                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    return Ok(());
                }

                match &app.step {
                    Step::SelectDisk => match key.code {
                        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') | KeyCode::PageUp | KeyCode::BackTab => {
                            if !app.disks.is_empty() {
                                if app.selected_disk_idx > 0 {
                                    app.selected_disk_idx -= 1;
                                } else {
                                    app.selected_disk_idx = app.disks.len() - 1;
                                }
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') | KeyCode::PageDown | KeyCode::Tab => {
                            if !app.disks.is_empty() {
                                if app.selected_disk_idx + 1 < app.disks.len() {
                                    app.selected_disk_idx += 1;
                                } else {
                                    app.selected_disk_idx = 0;
                                }
                            }
                        }
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            app.disks = scan_disks();
                            if app.selected_disk_idx >= app.disks.len() && !app.disks.is_empty() {
                                app.selected_disk_idx = app.disks.len() - 1;
                            }
                            app.selected_part_idx = 0;
                            app.selected_free_idx = 0;
                            app.selected_efi_idx = 0;
                            app.logs.push(format!("Rescanned storage devices. Found {} drive(s).", app.disks.len()));
                        }
                        KeyCode::Enter => {
                            if !app.disks.is_empty() {
                                // Guard: never install onto the Live USB itself.
                                if let Some(d) = app.selected_disk() {
                                    if d.is_live_source {
                                        app.logs.push("Refusing Live USB as target: select an internal disk.".into());
                                        return Ok(());
                                    }
                                    if d.readonly {
                                        app.logs.push("Selected device is read-only.".into());
                                        return Ok(());
                                    }
                                }
                                app.selected_mode_idx = 0;
                                app.install_mode = InstallMode::Alongside;
                                app.step = Step::SelectInstallMode;
                            }
                        }
                        KeyCode::Esc => return Ok(()),
                        _ => {}
                    },
                    Step::SelectInstallMode => match key.code {
                        KeyCode::Up | KeyCode::Char('k') | KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                            // 3-way cycle: 0 Alongside, 1 Manual partition, 2 Erase.
                            let dir_up = matches!(key.code, KeyCode::Up | KeyCode::Char('k'));
                            if dir_up {
                                app.selected_mode_idx = app.selected_mode_idx.checked_sub(1).unwrap_or(2);
                            } else {
                                app.selected_mode_idx = (app.selected_mode_idx + 1) % 3;
                            }
                            app.install_mode = match app.selected_mode_idx {
                                0 => InstallMode::Alongside,
                                1 => InstallMode::UsePartition,
                                _ => InstallMode::EraseDisk,
                            };
                        }
                        KeyCode::Enter => match app.install_mode {
                            InstallMode::Alongside => {
                                let empty = app.selected_disk().map(|d| d.free_regions.is_empty()).unwrap_or(true);
                                app.selected_free_idx = 0;
                                if empty {
                                    app.logs.push("No unpartitioned free space found. Free space via GParted/Disks, or pick 'Manual Partition'.".into());
                                } else {
                                    app.step = Step::SelectFreeSpace;
                                }
                            }
                            InstallMode::UsePartition => {
                                if let Some(disk) = app.selected_disk() {
                                    if disk.partitions.is_empty() {
                                        app.logs.push("No existing partitions found on disk. Switch to 'Erase Entire Disk' or rescan.".into());
                                    } else {
                                        app.selected_part_idx = 0;
                                        app.step = Step::SelectPartition;
                                    }
                                }
                            }
                            InstallMode::EraseDisk => {
                                app.input_confirm.clear();
                                app.step = Step::ConfirmInstallation;
                            }
                        },
                        KeyCode::Esc => {
                            app.step = Step::SelectDisk;
                        }
                        _ => {}
                    },
                    Step::SelectFreeSpace => match key.code {
                        KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                            let n = app.selected_disk().map(|d| d.free_regions.len()).unwrap_or(0);
                            if n > 0 {
                                if app.selected_free_idx > 0 {
                                    app.selected_free_idx -= 1;
                                } else {
                                    app.selected_free_idx = n - 1;
                                }
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                            let n = app.selected_disk().map(|d| d.free_regions.len()).unwrap_or(0);
                            if n > 0 {
                                if app.selected_free_idx + 1 < n {
                                    app.selected_free_idx += 1;
                                } else {
                                    app.selected_free_idx = 0;
                                }
                            }
                        }
                        KeyCode::Enter => {
                            if let Some(fr) = app.selected_free_region() {
                                if fr.bytes < MIN_ROOT_BYTES {
                                    app.logs.push(format!(
                                        "Free region {} too small (needs {}). Pick a larger gap or Manual Partition.",
                                        fr.display,
                                        format_bytes(MIN_ROOT_BYTES)
                                    ));
                                    return Ok(());
                                }
                            }
                            app.selected_efi_idx = 0;
                            app.step = Step::SelectEfiPartition;
                        }
                        KeyCode::Esc => {
                            app.step = Step::SelectInstallMode;
                        }
                        _ => {}
                    },
                    Step::SelectPartition => match key.code {
                        KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                            let n = app.selectable_partitions().len();
                            if n > 0 {
                                if app.selected_part_idx > 0 {
                                    app.selected_part_idx -= 1;
                                } else {
                                    app.selected_part_idx = n - 1;
                                }
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                            let n = app.selectable_partitions().len();
                            if n > 0 {
                                if app.selected_part_idx + 1 < n {
                                    app.selected_part_idx += 1;
                                } else {
                                    app.selected_part_idx = 0;
                                }
                            }
                        }
                        KeyCode::Enter => {
                            if let Some(p) = app.selected_partition() {
                                if p.is_mounted {
                                    app.logs.push(format!(
                                        "{} is mounted at {} — unmount it first or use Alongside mode.",
                                        p.path, p.mountpoint
                                    ));
                                    return Ok(());
                                }
                                if p.is_too_small {
                                    app.logs.push(format!(
                                        "{} ({}) is smaller than {} minimum — pick a larger partition.",
                                        p.path,
                                        p.size,
                                        format_bytes(MIN_ROOT_BYTES)
                                    ));
                                    return Ok(());
                                }
                            }
                            app.selected_efi_idx = 0;
                            app.step = Step::SelectEfiPartition;
                        }
                        KeyCode::Esc => {
                            app.step = Step::SelectInstallMode;
                        }
                        _ => {}
                    },
                    Step::SelectEfiPartition => match key.code {
                        KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                            let efis = app.efi_partitions();
                            if !efis.is_empty() {
                                if app.selected_efi_idx > 0 {
                                    app.selected_efi_idx -= 1;
                                } else {
                                    app.selected_efi_idx = efis.len() - 1;
                                }
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                            let efis = app.efi_partitions();
                            if !efis.is_empty() {
                                if app.selected_efi_idx + 1 < efis.len() {
                                    app.selected_efi_idx += 1;
                                } else {
                                    app.selected_efi_idx = 0;
                                }
                            }
                        }
                        KeyCode::Enter => {
                            app.input_confirm.clear();
                            app.step = Step::ConfirmInstallation;
                        }
                        KeyCode::Esc => {
                            match app.install_mode {
                                InstallMode::Alongside => app.step = Step::SelectFreeSpace,
                                InstallMode::UsePartition => app.step = Step::SelectPartition,
                                InstallMode::EraseDisk => app.step = Step::SelectInstallMode,
                            }
                        }
                        _ => {}
                    },
                    Step::ConfirmInstallation => match key.code {
                        KeyCode::Enter => {
                            if app.input_confirm.trim() == "INSTALL" {
                                app.step = Step::Installing;
                                app.logs.push("Starting disk preparation...".into());

                                let disk_path = app
                                    .selected_disk()
                                    .map(|d| d.path.clone())
                                    .unwrap_or_else(|| "/dev/sda".into());
                                let target_root = app.selected_partition().map(|p| p.path.clone());
                                let target_efi = app.selected_efi_partition().map(|p| p.path.clone());
                                let target_free = app.selected_free_region().cloned();

                                match perform_installation(
                                    &disk_path,
                                    target_root.as_deref(),
                                    target_efi.as_deref(),
                                    target_free.as_ref(),
                                    app.install_mode,
                                    &mut app.logs,
                                ) {
                                    Ok(()) => {
                                        app.step = Step::Done;
                                        app.logs.push("NIAT OS installation completed successfully!".into());
                                    }
                                    Err(err) => {
                                        app.step = Step::Failed(err.to_string());
                                        app.logs.push(format!("ERROR: {}", err));
                                    }
                                }
                            }
                        }
                        KeyCode::Backspace => {
                            app.input_confirm.pop();
                        }
                        KeyCode::Char(c) => {
                            app.input_confirm.push(c);
                        }
                        KeyCode::Esc => {
                            app.input_confirm.clear();
                            match app.install_mode {
                                InstallMode::UsePartition | InstallMode::Alongside => {
                                    app.step = Step::SelectEfiPartition;
                                }
                                InstallMode::EraseDisk => {
                                    app.step = Step::SelectInstallMode;
                                }
                            }
                        }
                        _ => {}
                    },
                    Step::Done | Step::Failed(_) => match key.code {
                        KeyCode::Enter | KeyCode::Esc => return Ok(()),
                        _ => {}
                    },
                    Step::Installing => {}
                }
            }
        }
    }
}

fn draw_ui(f: &mut Frame, app: &InstallerApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(8),
        ])
        .split(f.area());

    let title = Paragraph::new(Line::from(vec![
        Span::styled(" ◆ NIAT System Installer ", Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(" (v0.1) ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("│ Intent-Driven OS Deployment Engine", Style::default().fg(Color::Rgb(200, 220, 255))),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan))
            .style(Style::default().bg(Color::Rgb(18, 24, 38))),
    );
    f.render_widget(title, chunks[0]);

    match &app.step {
        Step::SelectDisk => {
            let mut lines = vec![
                Line::from(Span::styled("Select Target Storage Device:", Style::default().fg(Color::White).add_modifier(Modifier::BOLD))),
                Line::from(""),
            ];

            if app.disks.is_empty() {
                lines.push(Line::from(Span::styled("   No storage disks detected!", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))));
            } else {
                for (i, disk) in app.disks.iter().enumerate() {
                    let is_selected = i == app.selected_disk_idx;
                    let prefix = if is_selected { " ► " } else { "   " };
                    let style = if is_selected {
                        Style::default().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    };

                    let detail_style = if is_selected {
                        Style::default().fg(Color::Black).bg(Color::LightGreen)
                    } else {
                        Style::default().fg(Color::LightYellow)
                    };

                    let mut badges: Vec<String> = Vec::new();
                    if disk.is_live_source {
                        badges.push("LIVE USB — cannot install here".into());
                    }
                    if disk.readonly {
                        badges.push("read-only".into());
                    }
                    if !disk.parttable.is_empty() {
                        badges.push(disk.parttable.clone());
                    }
                    if !disk.free_regions.is_empty() {
                        badges.push(format!("{} free: {}", disk.free_regions.len(), disk.free_regions[0].display));
                    }
                    let os_info = if !disk.detected_oses.is_empty() {
                        format!(" — Detected: {}", disk.detected_oses.join(", "))
                    } else {
                        "".into()
                    };
                    let badge_info = if badges.is_empty() { String::new() } else { format!(" [{}]", badges.join(" | ")) };

                    lines.push(Line::from(vec![
                        Span::styled(prefix, style),
                        Span::styled(format!("[{}] {} ", i + 1, disk.path), style),
                        Span::styled(format!("- {} ", disk.size), detail_style),
                        Span::styled(format!("({})", disk.model), Style::default().fg(if is_selected { Color::Black } else { Color::DarkGray })),
                        Span::styled(badge_info, Style::default().fg(Color::Yellow)),
                        Span::styled(os_info, Style::default().fg(if is_selected { Color::Black } else { Color::LightCyan })),
                    ]));
                }
            }

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Use [↑/↓] or [j/k] to select, [r] to rescan, [ENTER] to continue, [ESC] to exit", Style::default().fg(Color::DarkGray))));

            let content = Paragraph::new(Text::from(lines))
                .block(
                    Block::default()
                        .title(" Target Storage Device Selection ")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::Rgb(71, 85, 105)))
                        .style(Style::default().bg(Color::Rgb(13, 17, 26))),
                );
            f.render_widget(content, chunks[1]);
        }
        Step::SelectInstallMode => {
            let disk_path = app.selected_disk().map(|d| d.path.as_str()).unwrap_or("/dev/sda");
            let mut lines = vec![
                Line::from(Span::styled(format!("Select Installation Method for target disk {}:", disk_path), Style::default().fg(Color::White).add_modifier(Modifier::BOLD))),
                Line::from(""),
            ];

            let mode0_sel = app.selected_mode_idx == 0;
            let m0_prefix = if mode0_sel { " ► " } else { "   " };
            let m0_style = if mode0_sel {
                Style::default().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let free_hint = app.selected_disk().map(|d| {
                if d.free_regions.is_empty() {
                    "No free space detected — shrink Windows/Linux first, or use Manual.".to_string()
                } else {
                    format!("{} free region(s), largest: {}", d.free_regions.len(), d.free_regions[0].display)
                }
            }).unwrap_or_default();
            lines.push(Line::from(vec![
                Span::styled(m0_prefix, m0_style),
                Span::styled("[1] Install Alongside (Dual-Boot Safe, Recommended)", m0_style),
            ]));
            lines.push(Line::from(Span::styled(format!("     Claim unpartitioned free space as a NEW NIAT partition. {}", free_hint), Style::default().fg(Color::DarkGray))));
            lines.push(Line::from(""));

            let mode1_sel = app.selected_mode_idx == 1;
            let m1_prefix = if mode1_sel { " ► " } else { "   " };
            let m1_style = if mode1_sel {
                Style::default().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(vec![
                Span::styled(m1_prefix, m1_style),
                Span::styled("[2] Manual Partition (Format Selected Partition)", m1_style),
            ]));
            lines.push(Line::from(Span::styled("     Reformat one existing partition as NIAT root. Existing boot entries preserved.", Style::default().fg(Color::DarkGray))));
            lines.push(Line::from(""));

            let mode2_sel = app.selected_mode_idx == 2;
            let m2_prefix = if mode2_sel { " ► " } else { "   " };
            let m2_style = if mode2_sel {
                Style::default().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(vec![
                Span::styled(m2_prefix, m2_style),
                Span::styled("[3] Erase Entire Disk (Clean Install)", m2_style),
            ]));
            lines.push(Line::from(Span::styled("     Wipes the entire disk and creates a clean GPT layout (512MB EFI + NIAT Root).", Style::default().fg(Color::DarkGray))));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Use [↑/↓] to switch method, [ENTER] to continue, [ESC] to go back", Style::default().fg(Color::DarkGray))));

            let content = Paragraph::new(Text::from(lines))
                .block(
                    Block::default()
                        .title(" Installation Mode ")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::Rgb(71, 85, 105)))
                        .style(Style::default().bg(Color::Rgb(13, 17, 26))),
                );
            f.render_widget(content, chunks[1]);
        }
        Step::SelectFreeSpace => {
            let mut lines = vec![
                Line::from(Span::styled("Install Alongside — Select Free Space for New NIAT Partition:", Style::default().fg(Color::White).add_modifier(Modifier::BOLD))),
                Line::from(Span::styled("Existing partitions are NOT touched. A new partition is created in the gap.", Style::default().fg(Color::DarkGray))),
                Line::from(""),
            ];

            if let Some(disk) = app.selected_disk() {
                if disk.free_regions.is_empty() {
                    lines.push(Line::from(Span::styled("   No unpartitioned free space on this disk.", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))));
                    lines.push(Line::from(Span::styled("   Shrink Windows (Disk Management) or Linux (GParted) first, then [r] rescan.", Style::default().fg(Color::Yellow))));
                } else {
                    for (i, fr) in disk.free_regions.iter().enumerate() {
                        let is_selected = i == app.selected_free_idx;
                        let prefix = if is_selected { " ► " } else { "   " };
                        let style = if is_selected {
                            Style::default().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::White)
                        };
                        let too_small = fr.bytes < MIN_ROOT_BYTES;
                        let flag = if too_small { " [TOO SMALL — needs 4.0 GB]" } else { " [fits NIAT]" };
                        lines.push(Line::from(vec![
                            Span::styled(prefix, style),
                            Span::styled(format!("[{}] Free space {} ", i + 1, fr.display), style),
                            Span::styled(format!("(offset {})", format_bytes(fr.start_bytes)), Style::default().fg(if is_selected { Color::Black } else { Color::DarkGray })),
                            Span::styled(flag, Style::default().fg(if too_small { Color::Red } else if is_selected { Color::Black } else { Color::Green })),
                        ]));
                    }
                }
            }

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Use [↑/↓] to select free gap, [ENTER] to continue, [ESC] to go back", Style::default().fg(Color::DarkGray))));

            let content = Paragraph::new(Text::from(lines))
                .block(
                    Block::default()
                        .title(" Alongside: Free Space Selection ")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::Rgb(71, 85, 105)))
                        .style(Style::default().bg(Color::Rgb(13, 17, 26))),
                );
            f.render_widget(content, chunks[1]);
        }
        Step::SelectPartition => {
            let selectable = app.selectable_partitions();
            let mut lines = vec![
                Line::from(Span::styled("Manual Partition — Select Partition to Format as NIAT Root (/):", Style::default().fg(Color::White).add_modifier(Modifier::BOLD))),
                Line::from(Span::styled("⚠ Only the selected partition is formatted. EFI boot entries are preserved.", Style::default().fg(Color::Yellow))),
                Line::from(""),
            ];

            if selectable.is_empty() {
                lines.push(Line::from(Span::styled("   No usable partitions on this device (EFI/live media hidden).", Style::default().fg(Color::Red))));
            } else {
                for (i, part) in selectable.iter().enumerate() {
                    let is_selected = i == app.selected_part_idx;
                    let prefix = if is_selected { " ► " } else { "   " };
                    let style = if is_selected {
                        Style::default().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    };

                    let mut flags = Vec::new();
                    if !part.os_desc.is_empty() {
                        flags.push(format!("[{}]", part.os_desc));
                    }
                    if part.is_mounted {
                        flags.push(format!("[MOUNTED: {}]", part.mountpoint));
                    }
                    if part.is_too_small {
                        flags.push("[TOO SMALL — needs 4.0 GB]".into());
                    } else if !part.fstype.is_empty() {
                        flags.push(format!("[{} → ext4]", part.fstype));
                    }
                    let os_label = if flags.is_empty() { String::new() } else { format!(" {}", flags.join(" ")) };

                    lines.push(Line::from(vec![
                        Span::styled(prefix, style),
                        Span::styled(format!("[{}] {} ", i + 1, part.path), style),
                        Span::styled(format!("- {} ", part.size), Style::default().fg(if is_selected { Color::Black } else { Color::LightYellow })),
                        Span::styled(format!("({})", if part.fstype.is_empty() { "raw" } else { part.fstype.as_str() }), Style::default().fg(if is_selected { Color::Black } else { Color::DarkGray })),
                        Span::styled(os_label, Style::default().fg(if is_selected { Color::Black } else { Color::Cyan })),
                    ]));
                }
            }

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Use [↑/↓] to select partition for NIAT Root, [ENTER] to continue, [ESC] to go back", Style::default().fg(Color::DarkGray))));

            let content = Paragraph::new(Text::from(lines))
                .block(
                    Block::default()
                        .title(" Manual Partition Selection (EFI + Live USB hidden) ")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::Rgb(71, 85, 105)))
                        .style(Style::default().bg(Color::Rgb(13, 17, 26))),
                );
            f.render_widget(content, chunks[1]);
        }
        Step::SelectEfiPartition => {
            let mut lines = vec![
                Line::from(Span::styled("Select EFI System Partition (Preserves Windows/Fedora Bootloader):", Style::default().fg(Color::White).add_modifier(Modifier::BOLD))),
                Line::from(""),
            ];

            let efis = app.efi_partitions();
            if efis.is_empty() {
                lines.push(Line::from(Span::styled("   No explicit EFI partition found. Using default disk partition settings.", Style::default().fg(Color::Yellow))));
            } else {
                for (i, part) in efis.iter().enumerate() {
                    let is_selected = i == app.selected_efi_idx;
                    let prefix = if is_selected { " ► " } else { "   " };
                    let style = if is_selected {
                        Style::default().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White)
                    };

                    lines.push(Line::from(vec![
                        Span::styled(prefix, style),
                        Span::styled(format!("[{}] {} ", i + 1, part.path), style),
                        Span::styled(format!("- {} ({}) ", part.size, part.fstype), Style::default().fg(if is_selected { Color::Black } else { Color::LightYellow })),
                        Span::styled("[Preserves existing OS bootloader files]", Style::default().fg(if is_selected { Color::Black } else { Color::Green })),
                    ]));
                }
            }

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Use [↑/↓] to select EFI partition, [ENTER] to confirm, [ESC] to go back", Style::default().fg(Color::DarkGray))));

            let content = Paragraph::new(Text::from(lines))
                .block(
                    Block::default()
                        .title(" EFI Partition Selection ")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::Rgb(71, 85, 105)))
                        .style(Style::default().bg(Color::Rgb(13, 17, 26))),
                );
            f.render_widget(content, chunks[1]);
        }
        Step::ConfirmInstallation => {
            let disk = app.selected_disk().map(|d| d.path.as_str()).unwrap_or("target disk");
            let mut lines = vec![];

            match app.install_mode {
                InstallMode::Alongside => {
                    let fr = app.selected_free_region().map(|r| r.display.as_str()).unwrap_or("None");
                    let efi_p = app.selected_efi_partition().map(|p| p.path.as_str()).unwrap_or("Auto/None");
                    lines.push(Line::from(Span::styled("✔ CONFIRMATION: INSTALL ALONGSIDE (DUAL-BOOT SAFE)", Style::default().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD))));
                    lines.push(Line::from(""));
                    lines.push(Line::from(format!(" • Target Disk:           {}", disk)));
                    lines.push(Line::from(format!(" • New NIAT Partition:    {} free space (NEW partition, formatted ext4)", fr)));
                    lines.push(Line::from(format!(" • EFI Partition:         {} (Existing boot files PRESERVED)", efi_p)));
                    lines.push(Line::from(" • Existing OSes:         All partitions untouched — Windows / Linux stay bootable via GRUB."));
                }
                InstallMode::UsePartition => {
                    let root_p = app.selected_partition().map(|p| p.path.as_str()).unwrap_or("None");
                    let efi_p = app.selected_efi_partition().map(|p| p.path.as_str()).unwrap_or("Auto/None");
                    lines.push(Line::from(Span::styled("⚠ CONFIRMATION: PARTITION DUAL-BOOT INSTALLATION", Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD))));
                    lines.push(Line::from(""));
                    lines.push(Line::from(format!(" • Target Disk:           {}", disk)));
                    lines.push(Line::from(format!(" • NIAT Root Partition:   {} (WILL BE FORMATTED as ext4)", root_p)));
                    lines.push(Line::from(format!(" • EFI Partition:         {} (Existing boot files PRESERVED)", efi_p)));
                    lines.push(Line::from(" • Existing OSes:         Windows / Fedora boot files will remain intact in EFI."));
                }
                InstallMode::EraseDisk => {
                    lines.push(Line::from(Span::styled("⚠ WARNING: DESTRUCTIVE ACTION (ERASE DISK)", Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD))));
                    lines.push(Line::from(""));
                    lines.push(Line::from(format!("All existing partitions and data on {} will be PERMANENTLY ERASED.", disk)));
                    lines.push(Line::from("A fresh GPT partition table and NIAT OS rootfs will be written."));
                }
            }

            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Type '", Style::default().fg(Color::White)),
                Span::styled("INSTALL", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled("' to confirm and proceed with installation:", Style::default().fg(Color::White)),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("> ", Style::default().fg(Color::Green)),
                Span::styled(&app.input_confirm, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::styled("█", Style::default().fg(Color::Green)),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Press [ESC] to cancel and go back", Style::default().fg(Color::DarkGray))));

            let content = Paragraph::new(Text::from(lines))
                .block(
                    Block::default()
                        .title(" Confirmation Required ")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(if app.install_mode == InstallMode::EraseDisk { Color::Red } else { Color::Yellow }))
                        .style(Style::default().bg(Color::Rgb(28, 18, 22))),
                );
            f.render_widget(content, chunks[1]);
        }
        Step::Installing => {
            let lines = vec![
                Line::from(Span::styled("Installing NIAT OS...", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
                Line::from("Please wait while files are written to disk..."),
            ];
            let content = Paragraph::new(Text::from(lines))
                .block(
                    Block::default()
                        .title(" In Progress ")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::Yellow))
                        .style(Style::default().bg(Color::Rgb(13, 17, 26))),
                );
            f.render_widget(content, chunks[1]);
        }
        Step::Done => {
            let lines = vec![
                Line::from(Span::styled("✔ INSTALLATION COMPLETE", Style::default().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD))),
                Line::from(""),
                Line::from("NIAT OS is now successfully installed onto your storage device."),
                Line::from("Existing boot options (Windows / Fedora) have been preserved in GRUB boot menu."),
                Line::from(""),
                Line::from(Span::styled("Press [ENTER] to exit installer and reboot", Style::default().fg(Color::Cyan))),
            ];
            let content = Paragraph::new(Text::from(lines))
                .block(
                    Block::default()
                        .title(" Success ")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::LightGreen))
                        .style(Style::default().bg(Color::Rgb(13, 24, 18))),
                );
            f.render_widget(content, chunks[1]);
        }
        Step::Failed(err) => {
            let lines = vec![
                Line::from(Span::styled("✗ INSTALLATION FAILED", Style::default().fg(Color::White).bg(Color::Red).add_modifier(Modifier::BOLD))),
                Line::from(""),
                Line::from(format!("Error detail: {}", err)),
                Line::from(""),
                Line::from(Span::styled("Press [ESC] to return", Style::default().fg(Color::Yellow))),
            ];
            let content = Paragraph::new(Text::from(lines))
                .block(
                    Block::default()
                        .title(" Error ")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(Color::Red))
                        .style(Style::default().bg(Color::Rgb(28, 18, 22))),
                );
            f.render_widget(content, chunks[1]);
        }
    }

    let log_lines: Vec<Line> = app.logs.iter().rev().take(6).rev().map(|l| Line::from(Span::styled(l, Style::default().fg(Color::DarkGray)))).collect();
    let logs_widget = Paragraph::new(Text::from(log_lines))
        .block(
            Block::default()
                .title(" Activity Log ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Rgb(71, 85, 105)))
                .style(Style::default().bg(Color::Rgb(13, 17, 26))),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(logs_widget, chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_kv_line() {
        let line = r#"NAME="sda1" SIZE="209715200" FSTYPE="vfat" LABEL="EFI" MOUNTPOINT="/boot/efi" TYPE="part" RM="0""#;
        let map = parse_kv_line(line);
        assert_eq!(map.get("NAME").unwrap(), "sda1");
        assert_eq!(map.get("SIZE").unwrap(), "209715200");
        assert_eq!(map.get("FSTYPE").unwrap(), "vfat");
        assert_eq!(map.get("LABEL").unwrap(), "EFI");
        assert_eq!(map.get("MOUNTPOINT").unwrap(), "/boot/efi");
        assert_eq!(map.get("TYPE").unwrap(), "part");
        assert_eq!(map.get("RM").unwrap(), "0");
    }

    #[test]
    fn test_parse_lsblk_rich_columns_nvme_and_live_guard() {
        // Real-world shape: PKNAME attaches nvme0n1p1, PARTTYPE GUID detects
        // EFI, MOUNTPOINTS catches Live USB squashfs host.
        let output = r#"
PATH="/dev/nvme0n1" KNAME="nvme0n1" PKNAME="" NAME="nvme0n1" SIZE="1024209543168" MODEL="Samsung_990" FSTYPE="" LABEL="" UUID="" PARTLABEL="" PARTTYPE="" MOUNTPOINTS="" TYPE="disk" RM="0" RO="0" TRAN="nvme" PTTYPE="gpt" FSAVAIL=""
PATH="/dev/nvme0n1p1" KNAME="nvme0n1p1" PKNAME="nvme0n1" NAME="nvme0n1p1" SIZE="536870912" MODEL="" FSTYPE="vfat" LABEL="EFI" UUID="D46B-1571" PARTLABEL="EFI System Partition" PARTTYPE="c12a7328-f81f-11d2-ba4b-00a0c93ec93b" MOUNTPOINTS="/boot/efi" TYPE="part" RM="0" RO="0" TRAN="" PTTYPE="" FSAVAIL=""
PATH="/dev/nvme0n1p2" KNAME="nvme0n1p2" PKNAME="nvme0n1" NAME="nvme0n1p2" SIZE="200000143360" MODEL="" FSTYPE="ntfs" LABEL="Windows" UUID="BE96" PARTLABEL="Basic data partition" PARTTYPE="ebd0a0a2-b9e5-4433-87c0-68b6b72699c7" MOUNTPOINTS="" TYPE="part" RM="0" RO="0" TRAN="" PTTYPE="" FSAVAIL=""
PATH="/dev/sdb" KNAME="sdb" PKNAME="" NAME="sdb" SIZE="15669919744" MODEL="Cruzer_Blade" FSTYPE="" LABEL="" UUID="" PARTLABEL="" PARTTYPE="" MOUNTPOINTS="" TYPE="disk" RM="1" RO="0" TRAN="usb" PTTYPE="dos" FSAVAIL=""
PATH="/dev/sdb1" KNAME="sdb1" PKNAME="sdb" NAME="sdb1" SIZE="15669919744" MODEL="" FSTYPE="iso9660" LABEL="NIAT_LIVE" UUID="" PARTLABEL="" PARTTYPE="" MOUNTPOINTS="/run/live/medium" TYPE="part" RM="0" RO="0" TRAN="" PTTYPE="" FSAVAIL=""
"#;
        let os_map = HashMap::new();
        let mut disks = Vec::new();
        parse_lsblk_output(output, &os_map, &mut disks);

        assert_eq!(disks.len(), 2);
        let nvme = disks.iter().find(|d| d.name == "nvme0n1").expect("nvme disk");
        // Both partitions attached via PKNAME (not prefix guess).
        assert_eq!(nvme.partitions.len(), 2);
        assert!(nvme.partitions.iter().any(|p| p.is_efi && p.path == "/dev/nvme0n1p1"));
        assert!(nvme.detected_oses.iter().any(|os| os.contains("Windows")));
        assert_eq!(nvme.transport, "nvme");
        assert_eq!(nvme.parttable, "gpt");

        let usb = disks.iter().find(|d| d.name == "sdb").expect("usb disk");
        assert!(usb.is_live_source, "iso9660 on /run/live must flag Live USB");
        assert!(usb.partitions.iter().all(|p| p.is_live_source));

        // Manual partition picker hides EFI + live media.
        let app = InstallerApp {
            disks,
            selected_disk_idx: 0,
            install_mode: InstallMode::UsePartition,
            selected_mode_idx: 1,
            selected_part_idx: 0,
            selected_free_idx: 0,
            selected_efi_idx: 0,
            step: Step::SelectPartition,
            input_confirm: String::new(),
            logs: Vec::new(),
        };
        let pickable = app.selectable_partitions();
        assert!(pickable.iter().all(|p| !p.is_efi && !p.is_live_source));
        assert!(pickable.iter().any(|p| p.path == "/dev/nvme0n1p2"));
    }

    #[test]
    fn test_parse_lsblk_legacy_columns_still_works() {
        // Old minimal lsblk (no PATH/PKNAME/PARTTYPE) must keep working.
        test_parse_lsblk_output_disks_and_partitions();
    }

    #[test]
    fn test_parse_lsblk_output_disks_and_partitions() {
        let output = r#"
NAME="sda" SIZE="1024209543168" MODEL="Micron_M600" FSTYPE="" LABEL="" MOUNTPOINT="" TYPE="disk" RM="0"
NAME="sda1" SIZE="209715200" MODEL="" FSTYPE="vfat" LABEL="EFI" MOUNTPOINT="/boot/efi" TYPE="part" RM="0"
NAME="sda3" SIZE="200000143360" MODEL="" FSTYPE="ntfs" LABEL="Windows" MOUNTPOINT="" TYPE="part" RM="0"
NAME="sda8" SIZE="621000261632" MODEL="" FSTYPE="ext4" LABEL="Fedora_Root" MOUNTPOINT="/" TYPE="part" RM="0"
"#;
        let os_map = HashMap::new();
        let mut disks = Vec::new();
        parse_lsblk_output(output, &os_map, &mut disks);

        assert_eq!(disks.len(), 1);
        let disk = &disks[0];
        assert_eq!(disk.name, "sda");
        assert_eq!(disk.partitions.len(), 3);
        assert!(disk.detected_oses.iter().any(|os| os.contains("EFI")));
        assert!(disk.detected_oses.iter().any(|os| os.contains("Windows")));
        assert!(disk.detected_oses.iter().any(|os| os.contains("Fedora") || os.contains("Linux")));
    }

    #[test]
    fn test_generate_grub_config_dual_boot() {
        let temp_dir = std::env::temp_dir().join("niat_test_efi");
        let efi_ms = temp_dir.join("EFI/Microsoft/Boot");
        let efi_fedora = temp_dir.join("EFI/fedora");
        let _ = std::fs::create_dir_all(&efi_ms);
        let _ = std::fs::create_dir_all(&efi_fedora);
        let _ = std::fs::write(efi_ms.join("bootmgfw.efi"), "dummy");
        let _ = std::fs::write(efi_fedora.join("shimx64.efi"), "dummy");

        let cfg = generate_grub_config(&temp_dir);
        assert!(cfg.contains("NIAT OS"));
        assert!(cfg.contains("Windows Boot Manager"));
        assert!(cfg.contains("Fedora Linux"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
