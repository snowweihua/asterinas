#!/bin/bash
# verify_sd_card.sh - Verify SD card has correct files for RPi3 boot
#
# Usage: sudo ./verify_sd_card.sh /dev/sdX

set -e

if [ $# -lt 1 ]; then
    echo "Usage: $0 <device>"
    echo "  device: SD card device (e.g., /dev/sdb)"
    exit 1
fi

DEVICE="$1"
BOOT_PART="${DEVICE}1"

echo "=== RPi3 SD Card Verification ==="
echo "Device: $DEVICE"
echo ""

# Check partition exists
if [ ! -b "$BOOT_PART" ]; then
    echo "ERROR: Boot partition $BOOT_PART not found!"
    echo "Run build_boot_sd.sh first to format the SD card."
    exit 1
fi

# Mount and check
MOUNT_DIR=$(mktemp -d)
mount "$BOOT_PART" "$MOUNT_DIR"

echo "[1/5] Boot partition files:"
ls -la "$MOUNT_DIR/"
echo ""

echo "[2/5] Checking required files:"
for file in bootcode.bin start.elf fixup.dat config.txt kernel8.img; do
    if [ -f "$MOUNT_DIR/$file" ]; then
        size=$(stat -c%s "$MOUNT_DIR/$file")
        echo "  ✓ $file ($size bytes)"
    else
        echo "  ✗ $file MISSING!"
    fi
done
echo ""

echo "[3/5] kernel8.img size check:"
if [ -f "$MOUNT_DIR/kernel8.img" ]; then
    size=$(stat -c%s "$MOUNT_DIR/kernel8.img")
    if [ "$size" -gt 1000000 ]; then
        echo "  ✓ kernel8.img is $size bytes (looks valid)"
    else
        echo "  ✗ kernel8.img is only $size bytes (too small!)"
    fi
else
    echo "  ✗ kernel8.img not found!"
fi
echo ""

echo "[4/5] config.txt contents:"
cat "$MOUNT_DIR/config.txt"
echo ""

echo "[5/5] cmdline.txt contents:"
cat "$MOUNT_DIR/cmdline.txt"
echo ""

# Check for old kernel backups
if [ -f "$MOUNT_DIR/kernel8.img.bak" ]; then
    echo "WARNING: kernel8.img.bak exists (old kernel backup)"
fi

umount "$MOUNT_DIR"
rmdir "$MOUNT_DIR"

echo "=== Verification Complete ==="
