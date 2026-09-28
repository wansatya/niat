#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
DIST_DIR="${ROOT_DIR}/dist"
BUILD_DIR="${ROOT_DIR}/build"
OVERLAY_DIR="${ROOT_DIR}/buildroot/external/niat/board/x86_64/rootfs-overlay"

echo "===================================================="
echo " Building NIAT: The Intent-Driven Operating System"
echo "===================================================="

mkdir -p "${DIST_DIR}" "${BUILD_DIR}"

# 1. Compile Rust workspace binaries
echo "[1/6] Compiling NIAT Rust binaries in release mode..."
cargo build --release --workspace

# 2. Prepare Root Filesystem
echo "[2/6] Assembling minimal root filesystem..."
ROOTFS_DIR="${BUILD_DIR}/rootfs"
rm -rf "${ROOTFS_DIR}"
mkdir -p "${ROOTFS_DIR}"/{bin,sbin,usr/bin,usr/sbin,usr/share/udhcpc,etc,proc,sys,dev,tmp,run,var/log/niat,var/lib/niat,var/run/niat,root,mnt,boot}

# Copy rootfs overlay
if [ -d "${OVERLAY_DIR}" ]; then
    cp -r "${OVERLAY_DIR}"/* "${ROOTFS_DIR}/"
fi

# Copy compiled NIAT binaries
cp "${ROOT_DIR}/target/release/agent-kernel" "${ROOTFS_DIR}/usr/bin/"
cp "${ROOT_DIR}/target/release/niat-tui" "${ROOTFS_DIR}/usr/bin/"
cp "${ROOT_DIR}/target/release/niat-installer" "${ROOTFS_DIR}/usr/bin/"
chmod 755 "${ROOTFS_DIR}/usr/bin/"*

# Copy shared libraries and dynamic linker required by binaries
echo "[+] Bundling dynamic linker and shared libraries..."
mkdir -p "${ROOTFS_DIR}/lib64" "${ROOTFS_DIR}/lib"
for bin in "${ROOTFS_DIR}/usr/bin/"*; do
    if [ -f "$bin" ]; then
        ldd "$bin" 2>/dev/null | grep -o '/lib[^ ]*' | while read -r lib; do
            if [ -f "$lib" ]; then
                target_dir="${ROOTFS_DIR}$(dirname "$lib")"
                mkdir -p "$target_dir"
                cp -L "$lib" "$target_dir/" 2>/dev/null || true
            fi
        done
    fi
done

# Install static BusyBox & all applets
echo "[+] Installing BusyBox utilities..."
BUSYBOX_BIN="${BUILD_DIR}/busybox"
if [ ! -f "${BUSYBOX_BIN}" ]; then
    curl -sL https://busybox.net/downloads/binaries/1.35.0-x86_64-linux-musl/busybox -o "${BUSYBOX_BIN}"
    chmod +x "${BUSYBOX_BIN}"
fi
cp "${BUSYBOX_BIN}" "${ROOTFS_DIR}/bin/busybox"
chmod 755 "${ROOTFS_DIR}/bin/busybox"

# Install all busybox symlinks
(
    cd "${ROOTFS_DIR}/bin"
    for app in $("${BUSYBOX_BIN}" --list); do
        ln -sf busybox "$app" 2>/dev/null || true
    done
)
(
    cd "${ROOTFS_DIR}/sbin"
    for app in $("${BUSYBOX_BIN}" --list); do
        ln -sf ../bin/busybox "$app" 2>/dev/null || true
    done
)

# Udpc script for DHCP
cat <<'EOF' > "${ROOTFS_DIR}/usr/share/udhcpc/default.script"
#!/bin/sh
case "$1" in
    deconfig)
        ip addr flush dev $interface
        ;;
    bound|renew)
        ip addr add $ip/$mask dev $interface
        if [ -n "$router" ]; then
            ip route add default via ${router%% *} dev $interface
        fi
        if [ -n "$dns" ]; then
            echo -n > /etc/resolv.conf
            for i in $dns; do
                echo "nameserver $i" >> /etc/resolv.conf
            done
        fi
        ;;
esac
exit 0
EOF
chmod 755 "${ROOTFS_DIR}/usr/share/udhcpc/default.script"

# Main init script
cat <<'EOF' > "${ROOTFS_DIR}/init"
#!/bin/sh
export PATH=/bin:/sbin:/usr/bin:/usr/sbin
export TERM=linux
export HOME=/root

# Mount virtual filesystems
mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev 2>/dev/null || mount -t tmpfs tmpfs /dev
mkdir -p /dev/pts /dev/shm
mount -t devpts devpts /dev/pts 2>/dev/null || true
mount -t tmpfs tmpfs /dev/shm 2>/dev/null || true
mount -t tmpfs tmpfs /tmp
mount -t tmpfs tmpfs /run
mount -t tmpfs tmpfs /var/run

# Create essential device nodes if missing
[ -e /dev/console ] || mknod -m 600 /dev/console c 5 1
[ -e /dev/null ]    || mknod -m 666 /dev/null c 1 3
[ -e /dev/zero ]    || mknod -m 666 /dev/zero c 1 5
[ -e /dev/tty ]     || mknod -m 666 /dev/tty c 5 0
[ -e /dev/tty0 ]    || mknod -m 620 /dev/tty0 c 4 0
[ -e /dev/tty1 ]    || mknod -m 620 /dev/tty1 c 4 1
[ -e /dev/ttyS0 ]   || mknod -m 620 /dev/ttyS0 c 4 64

# Redirect standard I/O to console
exec < /dev/console > /dev/console 2>&1

# ANSI styling helpers
GREEN="\033[1;32m"
AMBER="\033[1;33m"
BROWN="\033[0;33m"
YELLOW="\033[1;33m"
RESET="\033[0m"

printf "${AMBER}\n"
printf "  █  █  ███   █   ███\n"
printf "  ██ █   █   █ █   █\n"
printf "  █ ██   █   ███   █\n"
printf "  █  █  ███  █ █   █\n"
printf "${BROWN}\n"
printf "  intent-driven operating system · v0.1.0\n"
printf "  Made in Indonesia @ 2026 - Niat Baik Initiative\n"
printf "${RESET}\n\n"

log_step() {
    printf " ${GREEN}[ OK ]${RESET} %s\n" "$1"
}

log_step "Mounted virtual filesystems (/proc, /sys, /dev, /tmp, /run)"

# Device manager
echo /sbin/mdev > /proc/sys/kernel/hotplug 2>/dev/null || true
mdev -s 2>/dev/null || true
log_step "Device manager (mdev) initialized"

# Hostname
if [ -f /etc/hostname ]; then
    hostname $(cat /etc/hostname)
else
    hostname niat
fi
log_step "System hostname set: $(hostname)"

# Run service scripts
for s in /etc/init.d/S[0-9]*; do
    if [ -x "$s" ]; then
        "$s" start
    fi
done

# Start Agent Kernel daemon in background if not already running
mkdir -p /var/run/niat /var/log/niat /var/lib/niat
if ! pidof agent-kernel >/dev/null 2>&1; then
    /usr/bin/agent-kernel > /var/log/niat/agent.log 2>&1 &
    log_step "Started NIAT Agent Kernel daemon"
fi

# Launch TUI or Installer with controlling TTY (cttyhack)
while true; do
    if grep -q "niat.installer=1" /proc/cmdline; then
        printf " ${YELLOW}[*]${RESET} Launching NIAT System Installer...\n"
        sleep 0.5
        clear 2>/dev/null || printf "\033c\033[2J\033[3J\033[H"
        setsid cttyhack /usr/bin/niat-installer || true
    else
        printf " ${GREEN}[*]${RESET} Launching NIAT Workspace TUI...\n"
        sleep 0.5
        clear 2>/dev/null || printf "\033c\033[2J\033[3J\033[H"
        setsid cttyhack /usr/bin/niat-tui || true
    fi
    printf " ${YELLOW}[!]${RESET} NIAT application terminated. Forcing system reboot...\n"
    sync
    /sbin/reboot -f 2>/dev/null || /sbin/reboot 2>/dev/null || reboot -f 2>/dev/null || reboot 2>/dev/null || true
    echo 1 > /proc/sys/kernel/sysrq 2>/dev/null || true
    echo b > /proc/sysrq-trigger 2>/dev/null || true
    sleep 2
done
EOF
chmod 755 "${ROOTFS_DIR}/init"

# 3. Create compressed initramfs (rootfs.cpio.gz)
echo "[3/6] Packaging initramfs (rootfs.cpio.gz)..."
(
    cd "${ROOTFS_DIR}"
    find . -print0 | cpio --null -ov --format=newc | gzip -9 > "${DIST_DIR}/rootfs.cpio.gz"
)

# 4. Kernel
echo "[4/6] Preparing Linux Kernel (bzImage)..."
KERNEL_SRC=""
for k in /boot/vmlinuz-$(uname -r) /boot/vmlinuz-*; do
    if [ -r "$k" ]; then
        KERNEL_SRC="$k"
        break
    fi
done

if [ -n "${KERNEL_SRC}" ]; then
    echo "[+] Using kernel: ${KERNEL_SRC}"
    cp "${KERNEL_SRC}" "${DIST_DIR}/bzImage"
else
    echo "[!] No host kernel found in /boot. Creating placeholder bzImage..."
    touch "${DIST_DIR}/bzImage"
fi

# 5. Create UEFI Bootable Disk Image (niat.img)
echo "[5/6] Generating UEFI bootable disk image (dist/niat.img)..."
IMG_FILE="${DIST_DIR}/niat.img"
IMG_SIZE_MB=128
rm -f "${IMG_FILE}"
dd if=/dev/zero of="${IMG_FILE}" bs=1M count="${IMG_SIZE_MB}" status=none

# Create GPT partition table: Partition 1 = EFI System (FAT32)
sfdisk "${IMG_FILE}" <<EOF >/dev/null 2>&1
label: gpt
type=U, size=+, name=EFI
EOF

# Calculate partition offset: default sfdisk starts at sector 2048 (1MB)
PART_OFFSET_SECTORS=2048

# Prepare EFI Partition image
EFI_PART_FILE="${BUILD_DIR}/efi_part.img"
rm -f "${EFI_PART_FILE}"
EFI_SIZE_MB=$((IMG_SIZE_MB - 2))
dd if=/dev/zero of="${EFI_PART_FILE}" bs=1M count="${EFI_SIZE_MB}" status=none
mkfs.vfat -F 32 -n "NIAT_BOOT" "${EFI_PART_FILE}" >/dev/null 2>&1

# Populate EFI partition with GRUB / bootloader and kernel/initramfs
mmd -i "${EFI_PART_FILE}" ::EFI ::EFI/BOOT ::boot

# Copy kernel and initrd
mcopy -i "${EFI_PART_FILE}" "${DIST_DIR}/bzImage" ::boot/bzImage
mcopy -i "${EFI_PART_FILE}" "${DIST_DIR}/rootfs.cpio.gz" ::boot/rootfs.cpio.gz

# GRUB configuration
GRUB_CFG_TMP="${BUILD_DIR}/grub.cfg"
cat <<'EOF' > "${GRUB_CFG_TMP}"
set default="0"
set timeout=3

menuentry "1. Boot NIAT Live Workspace" {
    echo "Loading Linux Kernel (bzImage)..."
    linux /boot/bzImage root=/dev/ram0 rw console=ttyS0,115200n8 console=tty0
    echo "Loading Initial Ramdisk (rootfs.cpio.gz)..."
    initrd /boot/rootfs.cpio.gz
    echo "Booting NIAT Workspace..."
}

menuentry "2. Launch NIAT System Installer" {
    echo "Loading Linux Kernel (bzImage)..."
    linux /boot/bzImage root=/dev/ram0 rw console=ttyS0,115200n8 console=tty0 niat.installer=1
    echo "Loading Initial Ramdisk (rootfs.cpio.gz)..."
    initrd /boot/rootfs.cpio.gz
    echo "Booting NIAT Installer..."
}

menuentry "3. Recovery Console (BusyBox Shell)" {
    echo "Loading Linux Kernel (bzImage)..."
    linux /boot/bzImage root=/dev/ram0 rw console=ttyS0,115200n8 console=tty0 init=/bin/sh
    echo "Loading Initial Ramdisk (rootfs.cpio.gz)..."
    initrd /boot/rootfs.cpio.gz
    echo "Starting Recovery Shell..."
}

menuentry "4. Reboot System" {
    reboot
}
EOF
mcopy -i "${EFI_PART_FILE}" "${GRUB_CFG_TMP}" ::EFI/BOOT/grub.cfg
mcopy -i "${EFI_PART_FILE}" "${GRUB_CFG_TMP}" ::boot/grub.cfg

# Build standalone GRUB EFI binary
echo "[+] Building standalone GRUB EFI loader (BOOTX64.EFI)..."
grub2-mkimage -O x86_64-efi -o "${BUILD_DIR}/BOOTX64.EFI" -p "/EFI/BOOT" fat part_gpt part_msdos normal linux echo test configfile search search_fs_file search_label serial terminal terminfo efi_gop all_video video font gfxterm
mcopy -i "${EFI_PART_FILE}" "${BUILD_DIR}/BOOTX64.EFI" ::EFI/BOOT/BOOTX64.EFI

# Write EFI partition into disk image
dd if="${EFI_PART_FILE}" of="${IMG_FILE}" bs=512 seek="${PART_OFFSET_SECTORS}" conv=notrunc status=none

# Also create ISO image
cp "${IMG_FILE}" "${DIST_DIR}/niat.iso"

# 6. Generate Checksums
echo "[6/6] Generating SHA256 checksums..."
(
    cd "${DIST_DIR}"
    sha256sum bzImage rootfs.cpio.gz niat.img niat.iso > SHA256SUMS
)

echo "===================================================="
echo "[✓] NIAT build completed successfully!"
echo "    Artifacts generated in: ${DIST_DIR}/"
ls -lh "${DIST_DIR}"
echo "===================================================="
