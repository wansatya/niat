#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
DIST_DIR="${ROOT_DIR}/dist"
IMAGE="${DIST_DIR}/niat.img"

if [ ! -f "${IMAGE}" ]; then
    echo "Image not found at ${IMAGE}. Building first..."
    "${SCRIPT_DIR}/build.sh"
fi

if [ $# -eq 0 ] || [ "$1" = "--help" ] || [ "$1" = "-h" ]; then
    echo "NIAT USB Flash Utility"
    echo ""
    echo "Usage:"
    echo "  $0 --image-only       Ensure disk image is built without flashing"
    echo "  $0 /dev/sdX           Flash NIAT image directly to target USB device"
    echo ""
    exit 0
fi

if [ "$1" = "--image-only" ]; then
    echo "[✓] Disk image ready at: ${IMAGE}"
    ls -lh "${IMAGE}"
    exit 0
fi

TARGET_DEV="$1"

if [ ! -b "${TARGET_DEV}" ]; then
    echo "[-] Error: '${TARGET_DEV}' is not a valid block device."
    exit 1
fi

echo "===================================================="
echo " ⚠ WARNING: ALL DATA ON ${TARGET_DEV} WILL BE DESTROYED!"
echo " Target Device: ${TARGET_DEV}"
lsblk "${TARGET_DEV}" || true
echo "===================================================="
read -p "Are you absolutely sure you want to write NIAT OS to ${TARGET_DEV}? [y/N]: " CONFIRM

if [[ "$CONFIRM" =~ ^[Yy]$ ]]; then
    echo "[+] Writing ${IMAGE} to ${TARGET_DEV}..."
    sudo dd if="${IMAGE}" of="${TARGET_DEV}" bs=4M status=progress oflag=sync
    echo "[+] Syncing disks..."
    sync
    echo "[✓] Successfully created bootable NIAT USB drive on ${TARGET_DEV}!"
else
    echo "[-] Operation cancelled."
fi
