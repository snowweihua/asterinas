#!/usr/bin/env python3
"""MCP HTTP server for reading RPi3 serial console.

Run in your WSL2 terminal where /dev/ttyUSB0 is accessible:
  python3 tools/serial_mcp_server.py

Listens on http://localhost:8910
Tool: serial_read — reads buffered data from RPi3 serial console.
"""

import json, sys, os, time, threading
from http.server import HTTPServer, BaseHTTPRequestHandler

SERIAL_DEVICE = "/dev/ttyUSB0"
BAUD = 115200
HOST, PORT = "localhost", 8910

buf, lock = [], threading.Lock()
stop = threading.Event()

def reader():
    import serial
    try:
        ser = serial.Serial(SERIAL_DEVICE, BAUD, timeout=0.1)
        ser.flushInput()
        lock.acquire(); buf.append(b"[Serial reader started]\n"); lock.release()
        while not stop.is_set():
            try:
                if ser.in_waiting > 0:
                    lock.acquire(); buf.append(ser.read(ser.in_waiting)); lock.release()
            except: time.sleep(0.1)
    except Exception as e:
        lock.acquire(); buf.append(f"ERROR: {e}\n".encode()); lock.release()

def read_serial(timeout_ms=5000, clear=False):
    if clear:
        lock.acquire(); buf.clear(); lock.release()
    n = len(buf); waited = 0
    while waited < timeout_ms:
        lock.acquire(); ok = len(buf) > n; lock.release()
        if ok: break
        time.sleep(0.05); waited += 50
    lock.acquire(); data = b"".join(buf); buf.clear(); lock.release()
    return data

class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get('Content-Length', 0)))
        try: req = json.loads(body)
        except: self.send_error(400); return
        rid, method, params = req.get("id"), req.get("method",""), req.get("params",{})
        resp = self._mcp_dispatch(method, params, rid)
        if resp:
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps(resp).encode())
        else:
            self.send_response(202); self.end_headers()

    def _mcp_dispatch(self, method, params, rid):
        if method == "initialize":
            return {"jsonrpc":"2.0","id":rid,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{"listChanged":False}},"serverInfo":{"name":"serial-mcp","version":"1.0.0"}}}
        if method == "notifications/initialized": return None
        if method == "tools/list":
            return {"jsonrpc":"2.0","id":rid,"result":{"tools":[{
                "name":"serial_read",
                "description":"Read accumulated data from RPi3 serial console (buffered since last call).",
                "inputSchema":{"type":"object","properties":{
                    "timeout_ms":{"type":"integer","default":5000},
                    "clear_buffer":{"type":"boolean","default":False}
                }}
            }]}}
        if method == "tools/call":
            tool = params.get("name","")
            args = params.get("arguments",{})
            if tool == "serial_read":
                data = read_serial(args.get("timeout_ms",5000), args.get("clear_buffer",False))
                return {"jsonrpc":"2.0","id":rid,"result":{"content":[{"type":"text","text":data.decode("utf-8",errors="replace")}]}}
            return {"jsonrpc":"2.0","id":rid,"error":{"code":-32601,"message":f"Unknown tool: {tool}"}}
        return {"jsonrpc":"2.0","id":rid,"error":{"code":-32601,"message":f"Unknown method: {method}"}}
    def log_message(self, *a): pass

t = threading.Thread(target=reader, daemon=True); t.start(); time.sleep(0.5)
print(f"\n=== Serial MCP on http://{HOST}:{PORT} ===", file=sys.stderr)
try: HTTPServer((HOST, PORT), Handler).serve_forever()
except KeyboardInterrupt: stop.set()
