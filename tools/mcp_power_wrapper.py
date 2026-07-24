#!/usr/bin/env python3
"""MCP stdio bridge for power MCP server."""

import json
import sys
import requests

POWER_URL = "http://localhost:8911/"

def initialize_server():
    try:
        requests.post(POWER_URL, json={
            "jsonrpc": "2.0", "method": "initialize", "id": 1,
            "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "stdio-bridge", "version": "1.0"}}
        }, timeout=5)
    except Exception:
        pass

def main():
    initialize_server()

    while True:
        line = sys.stdin.readline()
        if not line:
            break

        try:
            req = json.loads(line.strip())
            method = req.get("method", "")
            req_id = req.get("id", 1)
            params = req.get("params", {})

            if method == "tools/list":
                resp = requests.post(POWER_URL, json={"jsonrpc": "2.0", "method": "tools/list", "id": req_id}, timeout=5)
                result = resp.json().get("result", {})
                response = {"jsonrpc": "2.0", "id": req_id, "result": result}
                print(json.dumps(response), flush=True)

            elif method == "tools/call":
                tool_name = params.get("name", "")
                tool_args = params.get("arguments", {})
                resp = requests.post(POWER_URL, json={
                    "jsonrpc": "2.0", "method": "tools/call", "id": req_id,
                    "params": {"name": tool_name, "arguments": tool_args}
                }, timeout=60)
                data = resp.json()
                response = {"jsonrpc": "2.0", "id": req_id, "result": data.get("result", {})}
                print(json.dumps(response), flush=True)

            elif method == "initialize":
                response = {
                    "jsonrpc": "2.0", "id": req_id, "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "power-mcp-bridge", "version": "1.0.0"}
                    }
                }
                print(json.dumps(response), flush=True)

            elif method == "notifications/initialized":
                pass

        except Exception as e:
            error = {"jsonrpc": "2.0", "id": 1, "error": {"code": -32603, "message": str(e)}}
            print(json.dumps(error), flush=True)

if __name__ == "__main__":
    main()
