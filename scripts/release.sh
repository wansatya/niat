#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
DIST_DIR="${ROOT_DIR}/dist"

VERSION="0.1.0"
RELEASE_TAR="${DIST_DIR}/niat-v${VERSION}-x86_64.tar.gz"

echo "===================================================="
echo " Packaging NIAT OS Release v${VERSION}"
echo "===================================================="

# Ensure build artifacts exist
"${SCRIPT_DIR}/build.sh"

echo "[+] Creating compressed release bundle: ${RELEASE_TAR}..."
tar -czf "${RELEASE_TAR}" -C "${DIST_DIR}" bzImage rootfs.cpio.gz niat.img niat.iso SHA256SUMS

(
    cd "${DIST_DIR}"
    sha256sum "niat-v${VERSION}-x86_64.tar.gz" >> SHA256SUMS
)

echo "[✓] Release package ready:"
ls -lh "${RELEASE_TAR}"
echo ""
echo "Checksums:"
cat "${DIST_DIR}/SHA256SUMS"
