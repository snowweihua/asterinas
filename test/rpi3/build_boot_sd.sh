#!/bin/bash
# build_boot_sd.sh - Format SD card and copy RPi3 boot files
#
# Usage: sudo ./build_boot_sd.sh /dev/sdX
#   where /dev/sdX is your SD card device (e.g., /dev/sdb)
#
# This script:
# 1. Creates a boot partition (FAT32, 256MB) for firmware + kernel
# 2. Creates a root partition (ext4, rest of card) for rootfs
# 3. Copies config.txt, cmdline.txt, and kernel8.img to boot partition
#
# WARNING: This will DESTROY all data on the target device!

set -e

if [ $# -lt 1 ]; then
    echo "Usage: $0 <device> [kernel8.img]"
    echo "  device:   SD card device (e.g., /dev/sdb)"
    echo "  kernel8.img: Path to kernel image (default: target/osdk/aster-nix/kernel8.img)"
    exit 1
fi

DEVICE="$1"
KERNEL_IMG="${2:-target/osdk/aster-nix/kernel8.img}"

# Verify device exists
if [ ! -b "$DEVICE" ]; then
    echo "Error: $DEVICE is not a block device"
    exit 1
fi

# Verify kernel image exists
if [ ! -f "$KERNEL_IMG" ]; then
    echo "Error: Kernel image not found: $KERNEL_IMG"
    echo "Build with: cargo osdk build --release --target-arch aarch64 --boot-method RawBinary --scheme aarch64-rpi3"
    exit 1
fi

# Get the script directory for config files
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=== RPi3 SD Card Boot Builder ==="
echo "Device: $DEVICE"
echo "Kernel: $KERNEL_IMG"
echo ""

# Unmount any mounted partitions
echo "[1/6] Unmounting mounted partitions..."
for part in "${DEVICE}"*; do
    if mountpoint -q "$part" 2>/dev/null; then
        umount "$part" || true
    fi
done

# Partition the SD card
echo "[2/6] Partitioning SD card..."
# Create fresh partition table
cat << EOF | fdisk "$DEVICE" || true
o
n
p
1
2048
+256M
t
c
n
p
2


w
EOF

# Force kernel to re-read partition table
partprobe "$DEVICE" 2>/dev/null || true
sleep 1

# Format boot partition as FAT32
echo "[3/6] Formatting boot partition..."
BOOT_PART="${DEVICE}1"
ROOT_PART="${DEVICE}2"

mkfs.fat -F 32 -n BOOT "$BOOT_PART"

# Format root partition as ext4
echo "[4/6] Formatting root partition..."
mkfs.ext4 -F -L ROOT "$ROOT_PART"

# Mount boot partition
echo "[5/6] Copying boot files to FAT32 partition..."
MOUNT_DIR=$(mktemp -d)
mount "$BOOT_PART" "$MOUNT_DIR"

# Copy GPU firmware files (required for RPi3 boot)
FIRMWARE_DIR="$SCRIPT_DIR/firmware"
if [ -d "$FIRMWARE_DIR" ]; then
    cp "$FIRMWARE_DIR/bootcode.bin" "$MOUNT_DIR/"
    cp "$FIRMWARE_DIR/start.elf" "$MOUNT_DIR/"
    cp "$FIRMWARE_DIR/fixup.dat" "$MOUNT_DIR/"
    echo "  Copied GPU firmware files"
else
    echo "  WARNING: Firmware files not found in $FIRMWARE_DIR"
    echo "  Run: ./download_firmware.sh"
fi

# Copy configuration files
cp "$SCRIPT_DIR/config.txt" "$MOUNT_DIR/"
cp "$SCRIPT_DIR/cmdline.txt" "$MOUNT_DIR/"

# Copy kernel image
cp "$KERNEL_IMG" "$MOUNT_DIR/kernel8.img"

# Set proper permissions
chmod 444 "$MOUNT_DIR/config.txt"
chmod 600 "$MOUNT_DIR/cmdline.txt"
chmod 644 "$MOUNT_DIR/kernel8.img"

# Sync and unmount
sync
umount "$MOUNT_DIR"
rmdir "$MOUNT_DIR"

echo "[6/6] Done!"
echo ""
echo "=== SD Card Ready ==="
echo "Boot partition (FAT32): ${DEVICE}1"
echo "  - config.txt"
echo "  - cmdline.txt"
echo "  - kernel8.img"
echo "Root partition (ext4):  ${DEVICE}2"
echo ""
echo "For rootfs, create an ext4 image with initramfs and mount at ${DEVICE}2"
echo "or use network boot/NFS for testing."