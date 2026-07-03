#!/bin/bash
# update_kernel.sh - Update only kernel8.img on existing SD card (no reformat)
#
# Usage: sudo ./update_kernel.sh /dev/sdX [kernel8.img]
#   where /dev/sdX is your SD card device (e.g., /dev/sdb)

set -e

if [ $# -lt 1 ]; then
    echo "Usage: $0 <device> [kernel8.img]"
    echo "  device:   SD card device (e.g., /dev/sdb)"
    echo "  kernel8.img: Path to kernel image (default: ../../target/osdk/aster-nix/kernel8.img)"
    exit 1
fi

DEVICE="$1"
KERNEL_IMG="${2:-../../target/osdk/aster-nix/kernel8.img}"

# Verify device exists
if [ ! -b "$DEVICE" ]; then
    echo "Error: $DEVICE is not a block device"
    exit 1
fi

# Verify kernel image exists
if [ ! -f "$KERNEL_IMG" ]; then
    echo "Error: Kernel image not found: $KERNEL_IMG"
    exit 1
fi

BOOT_PART="${DEVICE}1"

# Verify boot partition exists
if [ ! -b "$BOOT_PART" ]; then
    echo "Error: Boot partition $BOOT_PART not found"
    echo "Run build_boot_sd.sh first to format the SD card"
    exit 1
fi

echo "=== Updating kernel8.img on SD Card ==="
echo "Device: $DEVICE"
echo "Kernel: $KERNEL_IMG"
echo ""

# Mount boot partition
MOUNT_DIR=$(mktemp -d)
mount "$BOOT_PART" "$MOUNT_DIR"

# Backup old kernel
cp "$MOUNT_DIR/kernel8.img" "$MOUNT_DIR/kernel8.img.bak" 2>/dev/null || true

# Copy new kernel
cp "$KERNEL_IMG" "$MOUNT_DIR/kernel8.img"
chmod 644 "$MOUNT_DIR/kernel8.img"

# Sync and unmount
sync
umount "$MOUNT_DIR"
rmdir "$MOUNT_DIR"

echo "Done! kernel8.img updated on $BOOT_PART"
echo "Old kernel backed up as kernel8.img.bak"
