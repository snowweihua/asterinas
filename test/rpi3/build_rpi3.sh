#!/bin/bash
set -e
cd /root/asterinas

# Build with RPi3 linker
export RUSTFLAGS="-C link-arg=-Taarch64-rpi3.ld"
cargo build --release --target aarch64-unknown-none-softfloat 2>&1 | tail -5

# Create raw binary
objcopy -O binary \
    target/aarch64-unknown-none-softfloat/release/aster-nix \
    test/rpi3/kernel8.img

# Show result
echo "Built kernel8.img:"
ls -la test/rpi3/kernel8.img
xxd test/rpi3/kernel8.img | head -5
