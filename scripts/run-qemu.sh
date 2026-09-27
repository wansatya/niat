#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
DIST_DIR="${ROOT_DIR}/dist"

IMAGE="${DIST_DIR}/niat.img"

if [ ! -f "${IMAGE}" ]; then
    echo "Disk image not found at ${IMAGE}. Running build script first..."
    "${SCRIPT_DIR}/build.sh"
fi

# Find OVMF firmware
OVMF_BIOS="/usr/share/OVMF/OVMF_CODE.fd"
if [ ! -f "${OVMF_BIOS}" ]; then
    OVMF_BIOS=$(find /usr/share -name "OVMF*.fd" -o -name "OVMF_CODE.fd" 2>/dev/null | head -1)
fi

MODE="gui"
DIRECT_BOOT=false

for arg in "$@"; do
    case "$arg" in
        --nographic|--terminal|-t|-n)
            MODE="terminal"
            ;;
        --gui|-g)
            MODE="gui"
            ;;
        --direct|-d)
            DIRECT_BOOT=true
            ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo "Options:"
            echo "  --gui, -g        Launch with graphical QEMU window (default if display is available)"
            echo "  --terminal, -t   Launch directly in current terminal (headless/nographic)"
            echo "  --direct, -d     Fast direct kernel boot (bypasses UEFI/GRUB)"
            echo "  --help, -h       Show this help message"
            exit 0
            ;;
    esac
done

# If no display server is available, fallback to terminal mode
if [ "${MODE}" = "gui" ] && [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
    echo "[!] No DISPLAY detected. Falling back to terminal mode..."
    MODE="terminal"
fi

echo "===================================================="
echo " Booting NIAT OS in QEMU"
echo " Mode:   ${MODE}"
if [ "${DIRECT_BOOT}" = true ]; then
    echo " Boot:   Direct Kernel Boot"
else
    echo " Boot:   UEFI (OVMF: ${OVMF_BIOS:-none})"
    echo " Disk:   ${IMAGE}"
fi
echo " Exit:   Ctrl+A then X (terminal mode) or close window"
echo "===================================================="

QEMU_ARGS=(
    -m 2G
    -smp 2
    -net nic,model=virtio
    -net user
)

# Add KVM acceleration if available
if [ -w /dev/kvm ]; then
    QEMU_ARGS+=(-enable-kvm -cpu host)
fi

# Terminal or GUI mode
if [ "${MODE}" = "terminal" ]; then
    QEMU_ARGS+=(-nographic -serial mon:stdio)
fi

if [ "${DIRECT_BOOT}" = true ]; then
    if [ "${MODE}" = "terminal" ]; then
        APPEND_CMD="console=tty0 console=ttyS0,115200n8"
    else
        APPEND_CMD="console=ttyS0,115200n8 console=tty0"
    fi
    QEMU_ARGS+=(
        -kernel "${DIST_DIR}/bzImage"
        -initrd "${DIST_DIR}/rootfs.cpio.gz"
        -append "${APPEND_CMD}"
    )
else
    QEMU_ARGS+=(-drive file="${IMAGE}",format=raw,if=virtio)
    if [ -n "${OVMF_BIOS}" ] && [ -f "${OVMF_BIOS}" ]; then
        QEMU_ARGS+=(-bios "${OVMF_BIOS}")
    fi
fi

exec qemu-system-x86_64 "${QEMU_ARGS[@]}"
