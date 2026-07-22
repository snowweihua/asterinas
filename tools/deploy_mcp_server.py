#!/usr/bin/env python3
"""MCP HTTP server for deploying kernel to RPi3 SD card.

Run in your WSL2 terminal where /mnt/d/pi_sd/ is writable:
  python3 tools/deploy_mcp_server.py

Listens on http://localhost:8911
Tool: deploy_kernel — converts ELF to raw binary and copies to /mnt/d/pi_sd/asterina.img
"""

import json, sys, os, shutil
from http.server import HTTPServer, BaseHTTPRequestHandler

HOST = "localhost"
PORT = 8911
DEPLOY_PATH = "/mnt/d/pi_sd/asterina.img"
DEFAULT_SRC = "/home/snow/asterinas/target/osdk/aster-nix/aster-nix-osdk-bin.qemu_elf"

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
            return {"jsonrpc":"2.0","id":rid,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{"listChanged":False}},"serverInfo":{"name":"deploy-mcp","version":"1.0.0"}}}
        elif method == "notifications/initialized":
            return None
        elif method == "tools/list":
            return {"jsonrpc":"2.0","id":rid,"result":{"tools":[{
                "name":"deploy_kernel",
                "description":"Convert ELF to raw binary and copy to /mnt/d/pi_sd/asterina.img",
                "inputSchema":{"type":"object","properties":{
                    "source_elf":{"type":"string","description":"Path to compiled ELF kernel","default":DEFAULT_SRC}
                }}
            }]}}
        elif method == "tools/call":
            tool = params.get("name","")
            args = params.get("arguments",{})
            if tool == "deploy_kernel":
                src = args.get("source_elf", DEFAULT_SRC)
                try:
                    raw = "/tmp/asterina_deploy.img"
                    r = os.system(f"aarch64-linux-gnu-objcopy -O binary '{src}' '{raw}' 2>/dev/null")
                    if r != 0:
                        return {"jsonrpc":"2.0","id":rid,"result":{"content":[{"type":"text","text":f"ERROR: objcopy failed (exit={r})"}]}}
                    if not os.path.exists(raw):
                        return {"jsonrpc":"2.0","id":rid,"result":{"content":[{"type":"text","text":"ERROR: objcopy produced no output"}]}}
                    shutil.copy2(raw, DEPLOY_PATH)
                    sz = os.path.getsize(DEPLOY_PATH)
                    os.remove(raw)
                    return {"jsonrpc":"2.0","id":rid,"result":{"content":[{"type":"text","text":f"OK: deployed {sz} bytes to {DEPLOY_PATH}"}]}}
                except Exception as e:
                    return {"jsonrpc":"2.0","id":rid,"result":{"content":[{"type":"text","text":f"ERROR: {e}"}]}}
            return {"jsonrpc":"2.0","id":rid,"error":{"code":-32601,"message":f"Unknown tool: {tool}"}}
        else:
            return {"jsonrpc":"2.0","id":rid,"error":{"code":-32601,"message":f"Unknown method: {method}"}}

    def log_message(self, *a):
        sys.stderr.write(f"[MCP] {a[0]} {a[1]} {a[2]}\n")

print(f"\n=== Deploy MCP on http://{HOST}:{PORT} ===", file=sys.stderr)
print(f"=== Deploy target: {DEPLOY_PATH} ===", file=sys.stderr)
print("=== Press Ctrl+C to stop ===", file=sys.stderr)
HTTPServer((HOST, PORT), MCPHandler).serve_forever()
