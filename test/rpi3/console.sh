#!/bin/bash
# Asterinas RPi3 Serial Console
#
# Opens the serial console to the RPi3 at 115200 baud.
# Prerequisites:
#   1. usbipd-win installed on Windows
#   2. USB-TTL adapter attached via: usbipd attach --wsl --busid <BUSID>
#   3. Device appears as /dev/ttyUSB0 in WSL2
#
# Usage: ./test/rpi3/console.sh [device]
#   device defaults to /dev/ttyUSB0

DEVICE="${1:-/dev/ttyUSB0}"
BAUD=115200

# Check if device exists
if [ ! -e "$DEVICE" ]; then
    echo "ERROR: $DEVICE not found."
    echo ""
    echo "To attach USB serial adapter from Windows PowerShell:"
    echo "  usbipd list"
    echo "  usbipd attach --wsl --busid <BUSID>"
    echo ""
    echo "Available serial devices:"
    ls /dev/ttyUSB* /dev/ttyACM* 2>/dev/null || echo "  (none found)"
    exit 1
fi

echo "Connecting to RPi3 serial console"
echo "  Device : $DEVICE"
echo "  Baud   : $BAUD"
echo "  Exit   : Ctrl-A then K (screen) or Ctrl-A then X (minicom)"
echo ""

# Use minicom if available (cleaner), otherwise fall back to screen
if command -v minicom &>/dev/null; then
    minicom -D "$DEVICE" -b "$BAUD" -8 --noinit
else
    screen "$DEVICE" "$BAUD"
fi
