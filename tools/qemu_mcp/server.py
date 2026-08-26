#!/usr/bin/env python3
"""MCP server for interactive AArch64 QEMU tests using tmux."""

from __future__ import annotations

import subprocess
import time
from pathlib import Path

from fastmcp import FastMCP

WORKSPACE = Path(__file__).resolve().parents[2]
DEFAULT_KERNEL = Path("/tmp/asterina.img")
DEFAULT_INITRAMFS = WORKSPACE / "test/build/initramfs.cpio"
DEFAULT_DTB = "/mnt/d/pi_sd/bcm2710-rpi-3-b.dtb"
TMUX_SESSION = "qemu-test"


def _run(cmd: list[str], check: bool = True) -> str:
    result = subprocess.run(cmd, capture_output=True, text=True)
    if check and result.returncode != 0:
        raise RuntimeError(f"Command failed: {cmd} -> {result.stderr}")
    return result.stdout.strip()


def _qemu_command(kernel: str, initramfs: str, memory: str, cpus: int) -> str:
    return " ".join([
        "qemu-system-aarch64",
        "-machine", "raspi3b",
        "-cpu", "cortex-a53",
        "-smp", str(cpus),
        "-m", memory,
        "-display", "none",
        "-monitor", "none",
        "-nographic",
        "-dtb", DEFAULT_DTB,
        "-kernel", kernel,
        "-initrd", initramfs,
        "-append", "init=/init console=ttyAMA0",
    ])


mcp = FastMCP(
    name="QEMU Test MCP",
    instructions=(
        "Run and interact with the Asterinas AArch64 QEMU test image. "
        "Uses tmux for proper stdio serial interaction."
    ),
)


@mcp.tool
def qemu_start_tool(
    kernel: str = str(DEFAULT_KERNEL),
    initramfs: str = str(DEFAULT_INITRAMFS),
    memory: str = "1G",
    cpus: int = 4,
) -> str:
    """Start an interactive QEMU AArch64 instance in tmux."""
    if not Path(kernel).exists():
        return f"ERROR: kernel image not found: {kernel}"
    if not Path(initramfs).exists():
        return f"ERROR: initramfs not found: {initramfs}"

    # Kill existing session if any
    subprocess.run(["tmux", "kill-session", "-t", TMUX_SESSION],
                   capture_output=True)
    time.sleep(0.5)

    # Create new tmux session with QEMU
    qemu_cmd = _qemu_command(kernel, initramfs, memory, cpus)
    subprocess.run(
        ["tmux", "new-session", "-d", "-s", TMUX_SESSION, qemu_cmd],
        check=True
    )
    return f"QEMU STARTED in tmux session '{TMUX_SESSION}'"


@mcp.tool
def qemu_read_serial_tool(wait_seconds: float = 1.0, max_lines: int = 100) -> str:
    """Read QEMU serial output from tmux pane."""
    # Check if session exists
    result = subprocess.run(
        ["tmux", "has-session", "-t", TMUX_SESSION],
        capture_output=True
    )
    if result.returncode != 0:
        return "QEMU NOT RUNNING"

    # Capture the pane content
    output = _run(["tmux", "capture-pane", "-t", TMUX_SESSION, "-p"])

    lines = output.split("\n")
    # Take last max_lines
    if len(lines) > max_lines:
        lines = lines[-max_lines:]

    return f"QEMU running\n" + "\n".join(lines)


@mcp.tool
def qemu_write_serial_tool(input_text: str) -> str:
    """Send text to the QEMU serial console via tmux send-keys."""
    result = subprocess.run(
        ["tmux", "has-session", "-t", TMUX_SESSION],
        capture_output=True
    )
    if result.returncode != 0:
        return "ERROR: QEMU is not running"

    # Escape special characters and send
    escaped = input_text.replace("\\", "\\\\").replace('"', '\\"')
    _run(["tmux", "send-keys", "-t", TMUX_SESSION, escaped, "Enter"], check=False)
    return f"SERIAL WRITE OK: {len(input_text)} characters"


@mcp.tool
def qemu_run_tool(
    kernel: str = str(DEFAULT_KERNEL),
    initramfs: str = str(DEFAULT_INITRAMFS),
    command: str = "echo qemu-ok",
    boot_wait_seconds: float = 8.0,
    command_wait_seconds: float = 3.0,
) -> str:
    """Start QEMU, send one shell command, collect output, then stop it."""
    started = qemu_start_tool(kernel, initramfs)
    if not started.startswith("QEMU STARTED"):
        return started
    time.sleep(max(0.0, boot_wait_seconds))
    qemu_write_serial_tool(command.rstrip("\n"))
    time.sleep(max(0.0, command_wait_seconds))
    output = qemu_read_serial_tool(0, 200)
    qemu_stop_tool()
    return output


@mcp.tool
def qemu_stop_tool() -> str:
    """Stop the QEMU instance by killing the tmux session."""
    result = subprocess.run(
        ["tmux", "kill-session", "-t", TMUX_SESSION],
        capture_output=True
    )
    if result.returncode != 0:
        return "QEMU NOT RUNNING"
    return "QEMU STOPPED"


if __name__ == "__main__":
    mcp.run()
