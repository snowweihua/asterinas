#!/bin/bash
# download_firmware.sh - Download Raspberry Pi 3 GPU firmware files
#
# The RPi3 GPU (VideoCore IV) boots first and needs these proprietary
# firmware blobs to load kernel8.img:
#   - bootcode.bin: GPU bootloader
#   - start.elf: GPU firmware that reads config.txt
#   - fixup.dat: Memory split configuration
#
# Usage: ./download_firmware.sh [output_dir]
#   Default output: test/rpi3/firmware/

set -euo pipefail

OUTPUT_DIR="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/firmware}"
FIRMWARE_REPO="https://github.com/raspberrypi/firmware/raw/master/boot"

mkdir -p "$OUTPUT_DIR"

echo "=== Downloading RPi3 GPU Firmware ==="
echo "Output: $OUTPUT_DIR"
echo ""

for file in bootcode.bin start.elf fixup.dat; do
    if [ -f "$OUTPUT_DIR/$file" ]; then
        echo "[SKIP] $file already exists"
    else
        echo "[DOWNLOAD] $file ..."
        curl -L -o "$OUTPUT_DIR/$file" "$FIRMWARE_REPO/$file" || {
            echo "Error: Failed to download $file"
            echo "You can manually download it from:"
            echo "  $FIRMWARE_REPO/$file"
            exit 1
        }
    fi
done

echo ""
echo "=== Firmware Download Complete ==="
echo "Files in $OUTPUT_DIR:"
ls -la "$OUTPUT_DIR/"
echo ""
echo "Next steps:"
echo "  1. Copy these files to your SD card boot partition"
echo "  2. Also copy config.txt, cmdline.txt, and kernel8.img"
echo "  3. Insert SD card into RPi3 and power on"
