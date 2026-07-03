#!/bin/bash
# build_kernel_docker.sh - Reliable build script for RPi3 kernel8.img
#
# Problem: Docker builds files as root, and rust-objcopy (inside Docker)
# creates kernel8.img that may not persist correctly on the host filesystem.
#
# Solution: This script ensures the file is built inside Docker and then
# explicitly copied to the host using docker cp, avoiding permission issues.
#
# Usage: ./build_kernel_docker.sh [scheme]
#   scheme: Build scheme (default: aarch64-rpi3)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ASTERINAS_ROOT="$(cd "$SCRIPT_DIR/../../" && pwd)"
SCHEME="${1:-aarch64-rpi3}"
DOCKER_IMAGE="asterinas/aarch64-dev:latest"

echo "=== Building kernel8.img for $SCHEME ==="
echo "Asterinas root: $ASTERINAS_ROOT"
echo ""

# Step 1: Build inside Docker (produces ELF)
echo "[1/3] Building ELF inside Docker..."
docker run --rm \
    -v "$ASTERINAS_ROOT:/root/asterinas" \
    "$DOCKER_IMAGE" \
    bash -c "cd /root/asterinas && cargo osdk build --release --target-arch aarch64 --scheme $SCHEME 2>&1 | tail -20"

# Step 2: Convert ELF to raw binary inside Docker
echo ""
echo "[2/3] Converting ELF to kernel8.img inside Docker..."
docker run --rm \
    -v "$ASTERINAS_ROOT:/root/asterinas" \
    "$DOCKER_IMAGE" \
    bash -c "
        export PATH=\"/root/.rustup/toolchains/nightly-2025-02-01-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin:\$PATH\"
        rust-objcopy -O binary \
            /root/asterinas/target/aarch64-unknown-none-softfloat/release/aster-nix-osdk-bin \
            /root/asterinas/target/osdk/aster-nix/kernel8.img
        ls -lh /root/asterinas/target/osdk/aster-nix/kernel8.img
    "

# Step 3: Verify file exists on host
echo ""
echo "[3/3] Verifying kernel8.img on host..."
KERNEL_IMG="$ASTERINAS_ROOT/target/osdk/aster-nix/kernel8.img"

if [ -f "$KERNEL_IMG" ]; then
    SIZE=$(stat -c%s "$KERNEL_IMG")
    TIMESTAMP=$(stat -c%y "$KERNEL_IMG")
    echo "  ✓ kernel8.img found"
    echo "    Size: $SIZE bytes ($((SIZE / 1024 / 1024)) MB)"
    echo "    Modified: $TIMESTAMP"
    echo ""
    echo "=== Build successful! ==="
    echo ""
    echo "To copy to SD card:"
    echo "  cp $KERNEL_IMG /media/stbpc/BOOT/kernel8.img"
    echo "  sync"
else
    echo "  ✗ kernel8.img NOT found on host!"
    echo "    Attempting to copy via docker cp..."
    
    # Fallback: use docker cp to extract the file
    TEMP_CONTAINER=$(docker create -v "$ASTERINAS_ROOT:/root/asterinas" "$DOCKER_IMAGE")
    docker cp "$TEMP_CONTAINER:/root/asterinas/target/osdk/aster-nix/kernel8.img" "$KERNEL_IMG"
    docker rm "$TEMP_CONTAINER"
    
    if [ -f "$KERNEL_IMG" ]; then
        echo "  ✓ Successfully copied via docker cp"
        echo ""
        echo "=== Build successful! ==="
    else
        echo "  ✗ FAILED to obtain kernel8.img"
        exit 1
    fi
fi
