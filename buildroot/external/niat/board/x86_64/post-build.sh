#!/bin/sh
set -e

TARGET_DIR=$1

echo "NIAT post-build: Configuring target rootfs..."

# Ensure essential directories
mkdir -p "${TARGET_DIR}/var/log/niat"
mkdir -p "${TARGET_DIR}/var/lib/niat"
mkdir -p "${TARGET_DIR}/var/run/niat"
mkdir -p "${TARGET_DIR}/etc/niat"

# Permissions
chmod 755 "${TARGET_DIR}/etc/init.d/rcS" 2>/dev/null || true
chmod 755 "${TARGET_DIR}/etc/init.d/S"* 2>/dev/null || true

echo "NIAT post-build complete."
