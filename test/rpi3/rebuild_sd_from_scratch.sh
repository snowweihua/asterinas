#!/bin/bash
# rebuild_sd_from_scratch.sh - Completely rebuild SD card from scratch
# This wipes the SD card and recreates it with all necessary files
#
# Usage: sudo ./rebuild_sd_from_scratch.sh /dev/sdX [kernel8.img]

set -e

if [ $# -lt 1 ]; then
    echo "Usage: $0 <device> [kernel8.img]"
    echo "  device:   SD card device (e.g., /dev/sdb)"
    echo "  kernel8.img: Path to kernel image (default: ../../target/osdk/aster-nix/kernel8.img)"
    exit 1
fi

DEVICE="$1"
KERNEL_IMG="${2:-../../target/osdk/aster-nix/kernel8.img}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=== RPi3 SD Card Complete Rebuild ==="
echo "WARNING: This will DESTROY all data on $DEVICE!"
echo "Device: $DEVICE"
echo "Kernel: $KERNEL_IMG"
read -p "Are you sure? Type 'yes' to continue: " confirm
if [ "$confirm" != "yes" ]; then
    echo "Aborted."
    exit 1
fi

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

# Unmount any mounted partitions
for part in "${DEVICE}"*; do
    if mountpoint -q "$part" 2>/dev/null; then
        umount "$part" || true
    fi
done

echo ""
echo "[1/7] Wiping partition table..."
dd if=/dev/zero of="$DEVICE" bs=512 count=1 conv=notrunc status=progress

echo ""
echo "[2/7] Creating new partition table..."
parted -s "$DEVICE" mklabel msdos

echo ""
echo "[3/7] Creating boot partition (256MB FAT32)..."
parted -s "$DEVICE" mkpart primary fat32 1MiB 257MiB
parted -s "$DEVICE" set 1 boot on

echo ""
echo "[4/7] Creating root partition (rest of card)..."
parted -s "$DEVICE" mkpart primary ext4 257MiB 100%

echo ""
echo "[5/7] Formatting partitions..."
BOOT_PART="${DEVICE}1"
ROOT_PART="${DEVICE}2"

# Wait for partitions to appear
sleep 2
partprobe "$DEVICE" 2>/dev/null || true
sleep 2

mkfs.fat -F 32 -n BOOT "$BOOT_PART"
mkfs.ext4 -F -L ROOT "$ROOT_PART"

echo ""
echo "[6/7] Copying boot files..."
MOUNT_DIR=$(mktemp -d)
mount "$BOOT_PART" "$MOUNT_DIR"

# Copy GPU firmware files
FIRMWARE_DIR="$SCRIPT_DIR/firmware"
if [ -d "$FIRMWARE_DIR" ]; then
    cp "$FIRMWARE_DIR/bootcode.bin" "$MOUNT_DIR/"
    cp "$FIRMWARE_DIR/start.elf" "$MOUNT_DIR/"
    cp "$FIRMWARE_DIR/fixup.dat" "$MOUNT_DIR/"
    echo "  Copied GPU firmware files"
else
    echo "  WARNING: Firmware files not found!"
    echo "  Run: ./download_firmware.sh"
fi

# Copy configuration files
cp "$SCRIPT_DIR/config.txt" "$MOUNT_DIR/"
cp "$SCRIPT_DIR/cmdline.txt" "$MOUNT_DIR/"

# Copy kernel image
cp "$KERNEL_IMG" "$MOUNT_DIR/kernel8.img"

# Set permissions
chmod 444 "$MOUNT_DIR/config.txt"
chmod 600 "$MOUNT_DIR/cmdline.txt"
chmod 644 "$MOUNT_DIR/kernel8.img"

sync
umount "$MOUNT_DIR"
rmdir "$MOUNT_DIR"

echo ""
echo "[7/7] Verifying..."
MOUNT_DIR=$(mktemp -d)
mount "$BOOT_PART" "$MOUNT_DIR"
echo "Files on boot partition:"
ls -la "$MOUNT_DIR/"
echo ""
echo "kernel8.img size: $(stat -c%s "$MOUNT_DIR/kernel8.img") bytes"
if [ -f "$MOUNT_DIR/kernel8.img" ]; then
    size=$(stat -c%s "$MOUNT_DIR/kernel8.img")
    if [ "$size" -gt 1000000 ]; then
        echo "  ✓ kernel8.img looks valid"
    else
        echo "  ✗ kernel8.img is too small!"
    fi
fi
umount "$MOUNT_DIR"
rmdir "$MOUNT_DIR"

echo ""
echo "=== SD Card Rebuild Complete ==="
echo "Boot partition: $BOOT_PART (FAT32)"
echo "Root partition: $ROOT_PART (ext4)"
echo ""
echo "Insert SD card into RPi3 and power on."
echo "Watch for 5-3-5 green LED blink pattern."
