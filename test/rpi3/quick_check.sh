#!/bin/bash
# quick_check.sh - One-command SD card diagnostic
# Usage: sudo ./quick_check.sh /dev/sdX

DEVICE="${1:-/dev/sdb}"
BOOT_PART="${DEVICE}1"

echo "=== Quick SD Card Check ==="
echo "Device: $DEVICE"

if [ ! -b "$BOOT_PART" ]; then
    echo "ERROR: $BOOT_PART not found!"
    echo "Available block devices:"
    lsblk | grep -E "^NAME|sd|mmc"
    exit 1
fi

MOUNT_DIR=$(mktemp -d)
mount "$BOOT_PART" "$MOUNT_DIR" 2>/dev/null || {
    echo "ERROR: Cannot mount $BOOT_PART"
    echo "Try: sudo mkdir -p /mnt/sdcard && sudo mount $BOOT_PART /mnt/sdcard"
    exit 1
}

echo ""
echo "Files on boot partition:"
ls -la "$MOUNT_DIR/"

echo ""
if [ -f "$MOUNT_DIR/kernel8.img" ]; then
    size=$(stat -c%s "$MOUNT_DIR/kernel8.img")
    echo "kernel8.img size: $size bytes"
    if [ "$size" -eq 3579872 ]; then
        echo "✓ Size matches expected (3,579,872 bytes)"
    else
        echo "✗ Size mismatch! Expected 3579872, got $size"
        echo "  Run: sudo cp /home/snow/asterinas/target/osdk/aster-nix/kernel8.img $MOUNT_DIR/"
    fi
else
    echo "✗ kernel8.img NOT FOUND!"
    echo "  Run: sudo cp /home/snow/asterinas/target/osdk/aster-nix/kernel8.img $MOUNT_DIR/"
fi

umount "$MOUNT_DIR"
rmdir "$MOUNT_DIR"
