#!/usr/bin/env python3
"""MCP HTTP server that reads from /dev/ttyUSB0.

Run in your WSL2 terminal where /dev/ttyUSB0 is accessible:
  python3 tools/serial_mcp_server.py

The server listens on http://localhost:8910 for MCP HTTP POST requests.

Install: pip install pyserial
"""

import json
import sys
import os
import time
import threading
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse

SERIAL_DEVICE = "/dev/ttyUSB0"
BAUD = 115200
HOST = "localhost"
PORT = 8910

# Global buffer
serial_buffer = []
buffer_lock = threading.Lock()
reader_thread = None
stop_event = threading.Event()
initialized = False

def serial_reader():
    import serial as pyserial
    try:
        ser = pyserial.Serial(SERIAL_DEVICE, BAUD, timeout=0.1)
        ser.flushInput()
        with buffer_lock:
            serial_buffer.append(b"[Serial reader started]\n")
        while not stop_event.is_set():
            try:
                if ser.in_waiting > 0:
                    data = ser.read(ser.in_waiting)
                    with buffer_lock:
                        serial_buffer.append(data)
            except Exception:
                time.sleep(0.1)
    except FileNotFoundError:
        with buffer_lock:
            serial_buffer.append(f"ERROR: {SERIAL_DEVICE} not found\n".encode())
    except Exception as e:
        with buffer_lock:
            serial_buffer.append(f"ERROR: {e}\n".encode())

def read_serial(timeout_ms=5000, clear=False):
    if clear:
        with buffer_lock:
            serial_buffer.clear()
    
    initial_len = len(serial_buffer)
    waited = 0
    while waited < timeout_ms:
        with buffer_lock:
            if len(serial_buffer) > initial_len:
                break
        time.sleep(0.05)
        waited += 50
    
    with buffer_lock:
        data = b"".join(serial_buffer)
        serial_buffer.clear()
    return data

class MCPHandler(BaseHTTPRequestHandler):
    def do_POST(self):
        content_length = int(self.headers.get('Content-Length', 0))
        body = self.rfile.read(content_length)
        
        try:
            request = json.loads(body)
        except json.JSONDecodeError:
            self.send_error(400, "Invalid JSON")
            return
        
        req_id = request.get("id")
        method = request.get("method", "")
        params = request.get("params", {})
        
        response = self.handle_mcp(method, params, req_id)
        if response is not None:
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps(response).encode())
        else:
            self.send_response(202)  # Accepted (for notifications)
            self.end_headers()
    
    def handle_mcp(self, method, params, req_id):
        if method == "initialize":
            return {
                "jsonrpc": "2.0", "id": req_id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {"tools": {"listChanged": False}},
                    "serverInfo": {"name": "serial-mcp", "version": "1.0.0"}
                }
            }
        elif method == "notifications/initialized":
            global initialized
            initialized = True
            return None
        elif method == "tools/list":
            return {
                "jsonrpc": "2.0", "id": req_id,
                "result": {
                    "tools": [{
                        "name": "serial_read",
                        "description": "Read accumulated data from RPi3 serial console (buffered since last call).",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "timeout_ms": {"type": "integer", "description": "Wait ms for new data (default 5000, 0=immediate)", "default": 5000},
                                "clear_buffer": {"type": "boolean", "description": "Clear stale data before waiting", "default": False}
                            }
                        }
                    }]
                }
            }
        elif method == "tools/call":
            tool = params.get("name", "")
            args = params.get("arguments", {})
            if tool == "serial_read":
                data = read_serial(args.get("timeout_ms", 5000), args.get("clear_buffer", False))
                text = data.decode("utf-8", errors="replace")
                return {"jsonrpc": "2.0", "id": req_id, "result": {"content": [{"type": "text", "text": text}]}}
            return {"jsonrpc": "2.0", "id": req_id, "error": {"code": -32601, "message": f"Unknown tool: {tool}"}}
        else:
            return {"jsonrpc": "2.0", "id": req_id, "error": {"code": -32601, "message": f"Unknown method: {method}"}}
    
    def log_message(self, format, *args):
        sys.stderr.write(f"[MCP] {args[0]} {args[1]} {args[2]}\n")

def main():
    # Install pyserial if needed
    try:
        import serial
    except ImportError:
        print("Installing pyserial...", file=sys.stderr)
        os.system(f"{sys.executable} -m pip install pyserial 2>/dev/null")
    
    # Start serial reader
    t = threading.Thread(target=serial_reader, daemon=True)
    t.start()
    time.sleep(0.5)
    
    # Start HTTP server
    server = HTTPServer((HOST, PORT), MCPHandler)
    print(f"\n=== Serial MCP Server running on http://{HOST}:{PORT} ===", file=sys.stderr)
    print(f"=== Reading from {SERIAL_DEVICE} ===", file=sys.stderr)
    print("=== Press Ctrl+C to stop ===", file=sys.stderr)
    
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down...", file=sys.stderr)
        stop_event.set()
        server.shutdown()

if __name__ == "__main__":
    main()
