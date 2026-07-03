#!/bin/bash
# diagnose_rpi3.sh - Run this on the PC with the SD card
# Usage: ./diagnose_rpi3.sh

echo "=== RPi3 SD Card Diagnostic ==="
echo "Date: $(date)"
echo ""

BOOT_DIR="/media/stbpc/BOOT"

echo "[1/6] Boot partition files:"
ls -la "$BOOT_DIR/"
echo ""

echo "[2/6] kernel8.img details:"
if [ -f "$BOOT_DIR/kernel8.img" ]; then
    stat "$BOOT_DIR/kernel8.img"
    echo ""
    echo "File type:"
    file "$BOOT_DIR/kernel8.img"
    echo ""
    echo "MD5 checksum:"
    md5sum "$BOOT_DIR/kernel8.img"
    echo ""
    echo "First 64 bytes (header):"
    xxd "$BOOT_DIR/kernel8.img" | head -5
else
    echo "ERROR: kernel8.img NOT FOUND!"
fi
echo ""

echo "[3/6] config.txt:"
cat "$BOOT_DIR/config.txt"
echo ""

echo "[4/6] cmdline.txt:"
cat "$BOOT_DIR/cmdline.txt"
echo ""

echo "[5/6] Check for old backups:"
ls -la "$BOOT_DIR/" | grep -E "kernel8|img"
echo ""

echo "[6/6] Filesystem check:"
df -h "$BOOT_DIR"
echo ""

echo "=== Please copy ALL output above and paste it in the chat ==="
