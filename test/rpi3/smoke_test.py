#!/usr/bin/env python3
"""
AArch64 Smoke Test for Asterinas on QEMU using tmux.

Tests:
1. Shell prompt (~ #) appears
2. echo command works
3. ls command works
"""

import subprocess
import time
import sys
import os
import tempfile

TMUX_SESSION = "smoke-test"
KERNEL = "/tmp/asterina.img"
INITRAMFS = "/home/snow/asterinas/test/build/initramfs.cpio"
DTB = "/mnt/d/pi_sd/bcm2710-rpi-3-b.dtb"
BOOT_TIMEOUT = 80
CMD_DELAY = 2


def run_cmd(cmd):
    return subprocess.run(cmd, shell=True, capture_output=True, text=True)


def cleanup():
    run_cmd(f"tmux kill-session -t {TMUX_SESSION} 2>/dev/null")
    run_cmd("killall qemu-system-aarch64 2>/dev/null")


def wait_for_boot(timeout):
    deadline = time.time() + timeout
    while time.time() < deadline:
        result = run_cmd(f"tmux capture-pane -t {TMUX_SESSION} -p")
        output = result.stdout
        if "~ #" in output:
            return True, output
        print("  waiting for shell prompt (~ #)...")
        time.sleep(2)
    return False, ""


def main():
    print("\n=== QEMU Smoke Test ===")

    # Verify files exist
    if not os.path.exists(KERNEL):
        print(f"ERROR: Kernel not found: {KERNEL}")
        print("Build with: cargo osdk build --release --target-arch aarch64 --boot-method qemu-direct --scheme aarch64-rpi3")
        sys.exit(1)
    if not os.path.exists(INITRAMFS):
        print(f"ERROR: Initramfs not found: {INITRAMFS}")
        sys.exit(1)

    cleanup()
    time.sleep(1)

    # Create QEMU launch script
    with tempfile.NamedTemporaryFile(mode='w', suffix='.sh', delete=False) as f:
        f.write(f"""#!/bin/bash
qemu-system-aarch64 \\
  -machine raspi3b \\
  -cpu cortex-a53 \\
  -smp 4 \\
  -m 1G \\
  -display none \\
  -monitor none \\
  -nographic \\
  -dtb {DTB} \\
  -kernel {KERNEL} \\
  -initrd {INITRAMFS} \\
  -append 'init=/init console=ttyAMA0'
""")
        script_path = f.name
    os.chmod(script_path, 0o755)

    try:
        # Start QEMU in tmux
        print("[1/5] Starting QEMU in tmux...")
        run_cmd(f"tmux new-session -d -s {TMUX_SESSION} {script_path}")
        time.sleep(2)

        # Check if QEMU is running
        result = run_cmd("ps aux | grep qemu-system-aarch64 | grep -v grep")
        if not result.stdout.strip():
            print("  ERROR: QEMU process not found!")
            cleanup()
            sys.exit(1)
        print("  QEMU started")

        # Wait for boot
        print("[2/5] Waiting for boot (~80s)...")
        found, output = wait_for_boot(BOOT_TIMEOUT)
        if not found:
            print("  FAIL: Shell prompt not found")
            cleanup()
            sys.exit(1)
        print("  PASS: Shell prompt found")

        # Test echo
        print("[3/5] Testing echo...")
        run_cmd(f"tmux send-keys -t {TMUX_SESSION} 'echo QEMU_SMOKE' Enter")
        time.sleep(CMD_DELAY)
        result = run_cmd(f"tmux capture-pane -t {TMUX_SESSION} -p")
        if "QEMU_SMOKE" in result.stdout:
            print("  PASS: echo works")
        else:
            print("  FAIL: echo output not found")
            cleanup()
            sys.exit(1)

        # Test ls
        print("[4/5] Testing ls...")
        run_cmd(f"tmux send-keys -t {TMUX_SESSION} 'ls /' Enter")
        time.sleep(CMD_DELAY)
        result = run_cmd(f"tmux capture-pane -t {TMUX_SESSION} -p")
        if "bin" in result.stdout and "etc" in result.stdout:
            print("  PASS: ls works")
        else:
            print("  FAIL: ls output not found")
            cleanup()
            sys.exit(1)

        # Done
        print("[5/5] Final output:")
        result = run_cmd(f"tmux capture-pane -t {TMUX_SESSION} -p")
        for line in result.stdout.split("\n")[-8:]:
            print(f"  {line}")

        cleanup()
        print("\n=== QEMU Smoke Test PASSED ===")
        sys.exit(0)

    finally:
        os.unlink(script_path)


if __name__ == "__main__":
    main()
