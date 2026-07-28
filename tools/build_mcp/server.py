#!/usr/bin/env python3
"""MCP server for building and deploying Asterinas kernel for RPi3.

Run directly as a script (uses stdio protocol):
  python3 -m build_mcp.server
"""

import os
import shutil
import subprocess
import threading
import time
import traceback

from fastmcp import FastMCP

WORKSPACE = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
DEFAULT_DEPLOY_PATH = "/mnt/d/pi_sd/asterina.img"
DEFAULT_ELF_PATH = os.path.join(WORKSPACE, "target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf")
DEFAULT_RAW_PATH = "/tmp/asterina.img"
DOCKER_IMAGE = "asterinas/aarch64-dev:latest"
LOG_FILE = "/home/snow/asterinas/tools/logs/build_mcp.log"

build_result = {"status": "idle", "result": None}
build_lock = threading.Lock()


def log(msg):
    os.makedirs(os.path.dirname(LOG_FILE), exist_ok=True)
    with open(LOG_FILE, "a", encoding="utf-8") as f:
        f.write(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] {msg}\n")


log("=== Build MCP server starting ===")


def do_build_kernel():
    """Run the Dockerised cargo build in background thread."""
    global build_result
    cmd = (
        f"docker run --rm -v {WORKSPACE}:/root/asterinas {DOCKER_IMAGE} bash -c "
        f"'cd /root/asterinas && cargo osdk build --release --target-arch aarch64 --boot-method qemu-direct --scheme aarch64-rpi3'"
    )
    start = time.time()
    result = None
    try:
        r = subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=3600)
        elapsed = time.time() - start
        if r.returncode != 0:
            result = (f"BUILD FAILED (exit={r.returncode}, {elapsed:.0f}s)\n"
                    f"stderr:\n{r.stderr[-1000:]}\nstdout:\n{r.stdout[-1000:]}")
        else:
            result = f"BUILD OK ({elapsed:.0f}s)\n{r.stdout[-2000:]}"
    except subprocess.TimeoutExpired:
        result = "BUILD FAILED: timeout after 3600s"
    except Exception as e:
        result = f"BUILD FAILED: exception {e}\n{traceback.format_exc()}"
    finally:
        with build_lock:
            build_result = {"status": "done", "result": result}
        log(f"do_build_kernel: completed with status={build_result['status']}")


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
    """Run 'cargo osdk build --release' for aarch64 inside Docker. Takes 2-5 minutes.

    If a build is already running, returns its status. Poll this tool repeatedly to get the result.
    """
    global build_result

    with build_lock:
        if build_result["status"] == "running":
            return "BUILD IN PROGRESS (check again in ~30s)"
        elif build_result["status"] == "done":
            build_result = {"status": "idle", "result": None}
            return build_result["result"]

    log("build_kernel_tool: starting background build")
    with build_lock:
        build_result = {"status": "running", "result": None}

    threading.Thread(target=do_build_kernel, daemon=True).start()
    return "BUILD STARTED (check again in ~30s for result)"


@mcp.tool
def convert_kernel_tool(source_elf: str = DEFAULT_ELF_PATH, output_img: str = DEFAULT_RAW_PATH) -> str:
    """Convert ELF to raw binary via objcopy."""
    log(f"convert_kernel_tool called")
    result = convert_kernel(source_elf, output_img)
    log(f"convert_kernel_tool result: {result[:100]}")
    return result


@mcp.tool
def deploy_kernel_tool(source_img: str = DEFAULT_RAW_PATH, deploy_path: str = DEFAULT_DEPLOY_PATH) -> str:
    """Copy raw binary to SD card mount point."""
    log(f"deploy_kernel_tool called")
    result = deploy(source_img, deploy_path)
    log(f"deploy_kernel_tool result: {result[:100]}")
    return result


@mcp.tool
def build_and_deploy_tool(deploy_path: str = DEFAULT_DEPLOY_PATH) -> str:
    """Build → convert → deploy. WARNING: May take 5-10 minutes. Use build_kernel_tool first."""
    log("build_and_deploy_tool called")
    text = build_kernel_tool()
    if "IN PROGRESS" in text:
        return text
    if "STARTED" in text:
        return "BUILD STARTED - check build_kernel_tool for result, then use convert_kernel_tool and deploy_kernel_tool separately"
    if "BUILD OK" in text:
        text += "\n---\n" + convert_kernel()
        text += "\n---\n" + deploy(deploy_path=deploy_path)
    log(f"build_and_deploy_tool result: {text[:200]}")
    return text


if __name__ == "__main__":
    log("=== Build MCP server running ===")
    mcp.run()
