#!/bin/bash
# copy_kernel.sh - Copy latest kernel8.img to SD card
# Run this on the remote PC (10.142.10.201) as user 'snow'

set -e

KERNEL_SRC="/home/snow/asterinas/target/osdk/aster-nix/kernel8.img"
SD_MOUNT="/media/stbpc/BOOT"

echo "=== Copying latest kernel8.img to SD card ==="
echo "Source: $KERNEL_SRC"
echo "Destination: $SD_MOUNT/kernel8.img"

if [ ! -f "$KERNEL_SRC" ]; then
    echo "ERROR: kernel8.img not found at $KERNEL_SRC"
    echo "Build it first with:"
    echo "  docker run --rm -v /home/snow/asterinas:/root/asterinas asterinas/aarch64-dev:latest \\"
    echo "    bash -c 'cd /root/asterinas && cargo osdk build --release --target-arch aarch64 --scheme aarch64-rpi3'"
    exit 1
fi

if [ ! -d "$SD_MOUNT" ]; then
    echo "ERROR: SD card not mounted at $SD_MOUNT"
    echo "Make sure the SD card is inserted and mounted."
    exit 1
fi

# Backup old kernel
cp "$SD_MOUNT/kernel8.img" "$SD_MOUNT/kernel8.img.bak" 2>/dev/null || true

# Copy new kernel
sudo cp "$KERNEL_SRC" "$SD_MOUNT/kernel8.img"
sudo chmod 644 "$SD_MOUNT/kernel8.img"
sync

echo ""
echo "Done! New kernel8.img copied."
echo "Old kernel backed up as kernel8.img.bak"
echo ""
echo "Next steps:"
echo "1. Unmount SD card: sudo umount $SD_MOUNT"
echo "2. Insert into RPi3"
echo "3. Power on and watch serial at 921600 baud"

# Verify
ls -lh "$SD_MOUNT/kernel8.img"
