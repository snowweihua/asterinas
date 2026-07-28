#!/usr/bin/env python3
"""MCP server for building and deploying Asterinas kernel for RPi3.

Run directly as a script (uses stdio protocol):
  python3 -m build_mcp.server
"""

import os
import shutil
import subprocess
import time

from fastmcp import FastMCP

WORKSPACE = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
DEFAULT_DEPLOY_PATH = "/mnt/d/pi_sd/asterina.img"
DEFAULT_ELF_PATH = os.path.join(WORKSPACE, "target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf")
DEFAULT_RAW_PATH = "/tmp/asterina.img"
DOCKER_IMAGE = "asterinas/aarch64-dev:latest"
LOG_FILE = "/home/snow/asterinas/tools/logs/build_mcp.log"


def log(msg):
    os.makedirs(os.path.dirname(LOG_FILE), exist_ok=True)
    with open(LOG_FILE, "a", encoding="utf-8") as f:
        f.write(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] {msg}\n")


log("=== Build MCP server starting ===")


def build_kernel() -> str:
    """Run the Dockerised cargo build."""
    cmd = (
        f"docker run --rm -v {WORKSPACE}:/root/asterinas {DOCKER_IMAGE} bash -c "
        f"'cd /root/asterinas && cargo osdk build --release --target-arch aarch64 --boot-method qemu-direct --scheme aarch64-rpi3'"
    )
    start = time.time()
    r = subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=3600)
    elapsed = time.time() - start
    if r.returncode != 0:
        return (f"BUILD FAILED (exit={r.returncode}, {elapsed:.0f}s)\n"
                f"stderr:\n{r.stderr}\nstdout:\n{r.stdout}")
    return f"BUILD OK ({elapsed:.0f}s)\n{r.stdout[-2000:]}"


def convert_kernel(src: str = None, dst: str = None) -> str:
    """objcopy ELF → raw binary."""
    if src is None:
        src = DEFAULT_ELF_PATH
    if dst is None:
        dst = DEFAULT_RAW_PATH
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    tmp = dst + ".tmp"
    for exe in ("llvm-objcopy", "aarch64-linux-gnu-objcopy", "aarch64-elf-objcopy"):
        r = subprocess.run(["which", exe], capture_output=True, text=True)
        if r.returncode == 0:
            objcopy = exe
            break
    else:
        return "ERROR: no objcopy found (tried llvm-objcopy, aarch64-linux-gnu-objcopy)"
    if not os.path.exists(src):
        return f"ERROR: source ELF not found: {src}"
    r = subprocess.run([objcopy, "-O", "binary", src, tmp], capture_output=True, text=True)
    if r.returncode != 0:
        return f"ERROR: objcopy failed: {r.stderr}"
    if not os.path.exists(tmp):
        return "ERROR: objcopy produced no output"
    shutil.move(tmp, dst)
    sz = os.path.getsize(dst)
    return f"CONVERT OK: {sz} bytes → {dst}"


def deploy(raw_path: str = None, deploy_path: str = None) -> str:
    """Copy raw binary to deploy path."""
    if raw_path is None:
        raw_path = DEFAULT_RAW_PATH
    if deploy_path is None:
        deploy_path = DEFAULT_DEPLOY_PATH
    if not os.path.exists(raw_path):
        return f"ERROR: raw binary not found: {raw_path} (run convert first)"
    shutil.copy2(raw_path, deploy_path)
    sz = os.path.getsize(deploy_path)
    return f"DEPLOY OK: {sz} bytes → {deploy_path}"


mcp = FastMCP(
    name="Build MCP",
    instructions="MCP server for building and deploying Asterinas kernel for RPi3. Provides tools: build_kernel, convert_kernel, deploy_kernel, build_and_deploy.",
)


@mcp.tool
def build_kernel_tool() -> str:
    """Run 'cargo osdk build --release' for aarch64 inside Docker. Takes 2-5 minutes."""
    log("build_kernel_tool called")
    result = build_kernel()
    log(f"build_kernel_tool result: {result[:200]}")
    return result


@mcp.tool
def convert_kernel_tool(source_elf: str = DEFAULT_ELF_PATH, output_img: str = DEFAULT_RAW_PATH) -> str:
    """Convert ELF to raw binary via objcopy."""
    log(f"convert_kernel_tool called: source_elf={source_elf}, output_img={output_img}")
    result = convert_kernel(source_elf, output_img)
    log(f"convert_kernel_tool result: {result}")
    return result


@mcp.tool
def deploy_kernel_tool(source_img: str = DEFAULT_RAW_PATH, deploy_path: str = DEFAULT_DEPLOY_PATH) -> str:
    """Copy raw binary to SD card mount point."""
    log(f"deploy_kernel_tool called: source_img={source_img}, deploy_path={deploy_path}")
    result = deploy(source_img, deploy_path)
    log(f"deploy_kernel_tool result: {result}")
    return result


@mcp.tool
def build_and_deploy_tool(deploy_path: str = DEFAULT_DEPLOY_PATH) -> str:
    """Build → convert → deploy in one call."""
    log("build_and_deploy_tool called")
    text = build_kernel()
    if "BUILD OK" in text:
        text += "\n---\n" + convert_kernel()
        text += "\n---\n" + deploy(deploy_path=deploy_path)
    log(f"build_and_deploy_tool result: {text[:200]}")
    return text


if __name__ == "__main__":
    log("=== Build MCP server running ===")
    mcp.run()
