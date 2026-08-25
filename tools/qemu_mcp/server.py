#!/usr/bin/env python3
"""MCP server for interactive AArch64 QEMU tests."""

from __future__ import annotations

import os
import queue
import subprocess
import threading
import time
from pathlib import Path

from fastmcp import FastMCP

WORKSPACE = Path(__file__).resolve().parents[2]
DEFAULT_KERNEL = Path("/tmp/qemu.bin")
DEFAULT_INITRAMFS = WORKSPACE / "test/build/initramfs.cpio"
_process: subprocess.Popen[bytes] | None = None
_output: queue.Queue[bytes] = queue.Queue()
_output_history = bytearray()
_output_lock = threading.Lock()
_process_lock = threading.Lock()


def _append_output(data: bytes) -> None:
    if not data:
        return
    _output.put(data)
    with _output_lock:
        _output_history.extend(data)
        if len(_output_history) > 2_000_000:
            del _output_history[:-2_000_000]


def _reader(proc: subprocess.Popen[bytes]) -> None:
    assert proc.stdout is not None
    while True:
        data = os.read(proc.stdout.fileno(), 4096)
        if not data:
            break
        _append_output(data)


def _qemu_command(kernel: str, initramfs: str, memory: str, cpus: int) -> list[str]:
    return [
        "qemu-system-aarch64",
        "-machine", "raspi3b",
        "-cpu", "cortex-a53",
        "-smp", "4",
        "-m", "1G",
        "-nographic",
        "-monitor", "none",
        "-serial", "stdio",
        "-dtb", "/mnt/d/pi_sd/bcm2710-rpi-3-b.dtb",
        "-kernel", kernel,
        "-initrd", initramfs,
        "-append", "init=/init console=ttyAMA0",
    ]


def _drain_output() -> bytes:
    chunks = []
    while True:
        try:
            chunks.append(_output.get_nowait())
        except queue.Empty:
            break
    return b"".join(chunks)


def _decode(data: bytes) -> str:
    return data.decode("utf-8", errors="replace")


mcp = FastMCP(
    name="QEMU Test MCP",
    instructions=(
        "Run and interact with the Asterinas AArch64 QEMU virt test image. "
        "Start QEMU, read serial output, send shell input, and stop QEMU."
    ),
)


@mcp.tool
def qemu_start_tool(
    kernel: str = str(DEFAULT_KERNEL),
    initramfs: str = str(DEFAULT_INITRAMFS),
    memory: str = "512M",
    cpus: int = 1,
) -> str:
    """Start an interactive QEMU AArch64 virt instance."""
    global _process, _output_history
    with _process_lock:
        if _process is not None and _process.poll() is None:
            return "QEMU ALREADY RUNNING"
        if not Path(kernel).exists():
            return f"ERROR: kernel image not found: {kernel}"
        if not Path(initramfs).exists():
            return f"ERROR: initramfs not found: {initramfs}"
        _drain_output()
        with _output_lock:
            _output_history = bytearray()
        _process = subprocess.Popen(
            _qemu_command(kernel, initramfs, memory, cpus),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
        threading.Thread(target=_reader, args=(_process,), daemon=True).start()
        return f"QEMU STARTED pid={_process.pid}"


@mcp.tool
def qemu_read_serial_tool(wait_seconds: float = 1.0, max_bytes: int = 20000) -> str:
    """Read accumulated QEMU serial output, waiting briefly for new data."""
    deadline = time.monotonic() + max(0.0, wait_seconds)
    data = _drain_output()
    while not data and time.monotonic() < deadline:
        time.sleep(0.05)
        data = _drain_output()
    if len(data) > max_bytes:
        data = data[-max_bytes:]
    status = "not running"
    if _process is not None and _process.poll() is None:
        status = "running"
    elif _process is not None:
        status = f"exited={_process.returncode}"
    return f"QEMU {status}\n{_decode(data)}"


@mcp.tool
def qemu_write_serial_tool(input_text: str) -> str:
    """Send text to the interactive QEMU serial console."""
    if _process is None or _process.poll() is not None or _process.stdin is None:
        return "ERROR: QEMU is not running"
    try:
        _process.stdin.write(input_text.encode())
        _process.stdin.flush()
    except BrokenPipeError:
        return "ERROR: QEMU serial input is closed"
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
    qemu_write_serial_tool(command.rstrip("\n") + "\n")
    time.sleep(max(0.0, command_wait_seconds))
    output = qemu_read_serial_tool(0, 50000)
    qemu_stop_tool()
    return output


@mcp.tool
def qemu_stop_tool() -> str:
    """Stop the interactive QEMU instance."""
    global _process
    with _process_lock:
        if _process is None:
            return "QEMU NOT RUNNING"
        if _process.poll() is None:
            _process.terminate()
            try:
                _process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                _process.kill()
                _process.wait(timeout=3)
        result = f"QEMU STOPPED exit={_process.returncode}"
        _process = None
        return result


if __name__ == "__main__":
    mcp.run()
