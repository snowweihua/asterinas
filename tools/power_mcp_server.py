#!/usr/bin/env python3
"""MCP HTTP server for controlling RPi3B power via USB-controlled power switch.

The power switch uses a simple serial protocol on /dev/ttyACM0:
  - Power ON:  0xA0 0x01 0x01 0xA2  then  0xA0 0x02 0x01 0xA3
  - Power OFF: 0xA0 0x01 0x00 0xA1  then  0xA0 0x02 0x00 0xA2

Run:
  python3 tools/power_mcp_server.py

Listens on http://localhost:8911
"""

import json, sys, os, time
from http.server import HTTPServer, BaseHTTPRequestHandler

HOST = "localhost"
PORT = 8911
TTY_DEVICE = "/dev/ttyACM0"

ON_SEQ  = [b"\xa0\x01\x01\xa2", b"\xa0\x02\x01\xa3"]
OFF_SEQ = [b"\xa0\x01\x00\xa1", b"\xa0\x02\x00\xa2"]

def send_raw_bytes(data: bytes):
    with open(TTY_DEVICE, "wb") as f:
        f.write(data)
        f.flush()
        time.sleep(0.5)

def power_on():
    for cmd in ON_SEQ:
        send_raw_bytes(cmd)
    return "OK: power ON"

def power_off():
    for cmd in OFF_SEQ:
        send_raw_bytes(cmd)
    return "OK: power OFF"

def power_status():
    if not os.path.exists(TTY_DEVICE):
        return "ABSENT"
    st = os.stat(TTY_DEVICE)
    return f"PRESENT (mode={oct(st.st_mode)})"

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
                "serverInfo":{"name":"power-mcp","version":"1.0.0"}
            }}
        elif method == "notifications/initialized":
            return None
        elif method == "tools/list":
            return {"jsonrpc":"2.0","id":rid,"result":{"tools":[
                {"name":"power_on",  "description":"Power ON the RPi3B board", "inputSchema":{"type":"object","properties":{}}},
                {"name":"power_off", "description":"Power OFF the RPi3B board", "inputSchema":{"type":"object","properties":{}}},
                {"name":"power_status","description":"Check power switch TTY status", "inputSchema":{"type":"object","properties":{}}},
            ]}}
        elif method == "tools/call":
            tool = params.get("name","")
            try:
                if tool == "power_on":
                    text = power_on()
                elif tool == "power_off":
                    text = power_off()
                elif tool == "power_status":
                    text = power_status()
                else:
                    return {"jsonrpc":"2.0","id":rid,"error":{"code":-32601,"message":f"Unknown tool: {tool}"}}
                return {"jsonrpc":"2.0","id":rid,"result":{"content":[{"type":"text","text":text}]}}
            except Exception as e:
                return {"jsonrpc":"2.0","id":rid,"result":{"content":[{"type":"text","text":f"ERROR: {e}"}]}}
        else:
            return {"jsonrpc":"2.0","id":rid,"error":{"code":-32601,"message":f"Unknown method: {method}"}}

    def log_message(self, *a):
        sys.stderr.write(f"[power-mcp] {a[0]}\n")

if __name__ == "__main__":
    print(f"\n=== Power MCP on http://{HOST}:{PORT} ===", file=sys.stderr)
    print(f"=== TTY: {TTY_DEVICE}", file=sys.stderr)
    print("=== Press Ctrl+C to stop ===", file=sys.stderr)
    HTTPServer((HOST, PORT), MCPHandler).serve_forever()
