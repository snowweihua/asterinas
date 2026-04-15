# Asterinas AArch64 Development - Docker Setup Guide

## Overview

This guide helps you set up the AArch64 development environment on a new PC using a pre-built Docker image.

## Transfer the Docker Image

**Option A: From the tar file (faster, ~3.6 GB)**
```bash
docker load -i asterinas-aarch64-dev.tar
```

**Option B: Rebuild from Dockerfile (requires internet)**
```bash
git clone https://github.com/snowweihua/asterinas.git
cd asterinas
docker build -f tools/docker/Dockerfile.aarch64_dev -t asterinas/aarch64-dev:latest .
```

## Using the Docker Image

### Run a build
```bash
docker run --rm -v $(pwd):/root/asterinas asterinas/aarch64-dev:latest \
  bash -c "cd /root/asterinas && cargo osdk build --scheme aarch64 --target-arch aarch64"
```

### Run the kernel in QEMU
```bash
docker run --rm -v $(pwd):/root/asterinas asterinas/aarch64-dev:latest \
  bash -c "cd /root/asterinas && cargo osdk run --scheme aarch64 --target-arch aarch64"
```

### Interactive shell in the container
```bash
docker run --rm -v $(pwd):/root/asterinas -it asterinas/aarch64-dev:latest bash
```

## What's Inside the Docker Image

| Component | Version |
|-----------|---------|
| Ubuntu | 22.04 |
| Rust | nightly-2025-02-01 |
| QEMU | 10.0.92 (aarch64-softmmu, riscv64-softmmu, loongarch64-softmmu, x86_64-softmmu) |
| cargo-osdk | 0.16.0 |

## Notes

- The `asterinas` source code is mounted as a volume into the container at `/root/asterinas`
- Build artifacts are stored in `target/` on the host (persists between runs)
- The `OSDK_LOCAL_DEV=1` environment variable is set by default, so local paths are used for dependencies
- QEMU serial output goes to the terminal (not a file) when using `cargo osdk run`

## Troubleshooting

**QEMU hangs with no output**
- Try with `-display none -serial stdio` for better output handling
- Check that DTB and initramfs files exist in `test/nix/` and `test/build/`

**Build fails with dependency errors**
- Make sure you have the latest code: `git pull origin aarch64_support`
- Clean build artifacts: `rm -rf target/`

## Quick Start

```bash
# 1. Load the Docker image
docker load -i asterinas-aarch64-dev.tar

# 2. Clone the repository
git clone https://github.com/snowweihua/asterinas.git
cd asterinas
git checkout aarch64_support

# 3. Build
docker run --rm -v $(pwd):/root/asterinas asterinas/aarch64-dev:latest \
  bash -c "cd /root/asterinas && cargo osdk build --scheme aarch64 --target-arch aarch64"

# 4. Run
docker run --rm -v $(pwd):/root/asterinas asterinas/aarch64-dev:latest \
  bash -c "cd /root/asterinas && cargo osdk run --scheme aarch64 --target-arch aarch64"
```
