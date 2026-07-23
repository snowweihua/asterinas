#!/usr/bin/env python3
"""MCP HTTP server for building and deploying Asterinas kernel for RPi3.

Run in your workspace root (where Docker can volume-mount $(pwd)):
  python3 tools/build_mcp_server.py

Listens on http://localhost:8912
Tools:
  build_kernel     — run Docker cargo osdk build for aarch64
  convert_kernel   — objcopy ELF→raw binary
  deploy_kernel    — copy raw binary to SD card mount
  build_and_deploy — all three in sequence
"""

import json, sys, os, shutil, subprocess, time
from http.server import HTTPServer, BaseHTTPRequestHandler

HOST = "localhost"
PORT = 8912

WORKSPACE = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
DEFAULT_DEPLOY_PATH = "/mnt/d/pi_sd/asterina.img"
DEFAULT_ELF_PATH    = os.path.join(WORKSPACE, "target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf")
DEFAULT_RAW_PATH    = os.path.join(WORKSPACE, "target/osdk/aster-nix/asterina.img")
DOCKER_IMAGE        = "asterinas/aarch64-dev:latest"

# ---------------------------------------------------------------------------

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
    # try llvm-objcopy first, fall back to aarch64-linux-gnu-objcopy
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

# ---------------------------------------------------------------------------

class MCPHandler(BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get('Content-Length', 0)))
        try:
            req = json.loads(body)
        except json.JSONDecodeError:
            self.send_error(400, "Invalid JSON"); return

        rid, method, params = req.get("id"), req.get("method",""), req.get("params",{})
        resp = self._mcp_dispatch(method, params, rid)
        if resp is not None:
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps(resp).encode())
        else:
            self.send_response(202); self.end_headers()

    def _mcp_dispatch(self, method, params, rid):
        if method == "initialize":
            return {"jsonrpc":"2.0","id":rid,"result":{
                "protocolVersion":"2024-11-05",
                "capabilities":{"tools":{"listChanged":False}},
                "serverInfo":{"name":"build-mcp","version":"1.0.0"}
            }}
        elif method == "notifications/initialized":
            return None
        elif method == "tools/list":
            return {"jsonrpc":"2.0","id":rid,"result":{"tools":[
                {
                    "name":"build_kernel",
                    "description":"Run 'cargo osdk build --release' for aarch64 inside Docker. Takes 2-5 minutes.",
                    "inputSchema":{"type":"object","properties":{}}
                },
                {
                    "name":"convert_kernel",
                    "description":"Convert ELF to raw binary via objcopy.",
                    "inputSchema":{"type":"object","properties":{
                        "source_elf":{"type":"string","description":"Path to compiled ELF","default":DEFAULT_ELF_PATH},
                        "output_img":{"type":"string","description":"Path for raw binary","default":DEFAULT_RAW_PATH}
                    }}
                },
                {
                    "name":"deploy_kernel",
                    "description":"Copy raw binary to SD card mount point.",
                    "inputSchema":{"type":"object","properties":{
                        "source_img":{"type":"string","description":"Path to raw binary","default":DEFAULT_RAW_PATH},
                        "deploy_path":{"type":"string","description":"Destination path on SD card","default":DEFAULT_DEPLOY_PATH}
                    }}
                },
                {
                    "name":"build_and_deploy",
                    "description":"Build → convert → deploy in one call.",
                    "inputSchema":{"type":"object","properties":{
                        "deploy_path":{"type":"string","description":"Destination path on SD card","default":DEFAULT_DEPLOY_PATH}
                    }}
                }
            ]}}
        elif method == "tools/call":
            tool = params.get("name","")
            args = params.get("arguments",{})
            try:
                if tool == "build_kernel":
                    text = build_kernel()
                elif tool == "convert_kernel":
                    text = convert_kernel(args.get("source_elf"), args.get("output_img"))
                elif tool == "deploy_kernel":
                    text = deploy(args.get("source_img"), args.get("deploy_path"))
                elif tool == "build_and_deploy":
                    text = build_kernel()
                    if "BUILD OK" in text:
                        text += "\n---\n" + convert_kernel()
                        text += "\n---\n" + deploy(deploy_path=args.get("deploy_path", DEFAULT_DEPLOY_PATH))
                else:
                    return {"jsonrpc":"2.0","id":rid,"error":{"code":-32601,"message":f"Unknown tool: {tool}"}}
                return {"jsonrpc":"2.0","id":rid,"result":{"content":[{"type":"text","text":text}]}}
            except subprocess.TimeoutExpired:
                return {"jsonrpc":"2.0","id":rid,"result":{"content":[{"type":"text","text":"ERROR: build timed out (600s)"}]}}
            except Exception as e:
                return {"jsonrpc":"2.0","id":rid,"result":{"content":[{"type":"text","text":f"ERROR: {e}"}]}}
        else:
            return {"jsonrpc":"2.0","id":rid,"error":{"code":-32601,"message":f"Unknown method: {method}"}}

    def log_message(self, *a):
        sys.stderr.write(f"[build-mcp] {a[0]} {a[1]} {a[2]}\n")

if __name__ == "__main__":
    print(f"\n=== Build/Deploy MCP on http://{HOST}:{PORT} ===", file=sys.stderr)
    print(f"=== Workspace: {WORKSPACE}", file=sys.stderr)
    print(f"=== Docker image: {DOCKER_IMAGE}", file=sys.stderr)
    print(f"=== Deploy target: {DEFAULT_DEPLOY_PATH}", file=sys.stderr)
    print("=== Press Ctrl+C to stop ===", file=sys.stderr)
    HTTPServer((HOST, PORT), MCPHandler).serve_forever()
