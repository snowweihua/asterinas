#!/usr/bin/env python3
"""MCP stdio bridge for HTTP MCP servers.

This script acts as an MCP server via stdio and proxies requests
to the actual HTTP MCP server.
"""

import json
import subprocess
import sys
import os

MCP_SERVERS = {
    "build-mcp": "http://localhost:8912/",
    "serial-mcp": "http://localhost:8910/",
}

def get_server(name):
    return MCP_SERVERS.get(name)

def handle_request(req):
    method = req.get("method", "")
    server_name = req.get("server", "")
    params = req.get("params", {})

    if method == "initialize":
        return {"protocolVersion": "2024-11-05", "capabilities": {"tools": {}}, "serverInfo": {"name": "stdio-bridge", "version": "1.0"}}

    if method == "tools/list":
        url = get_server(server_name) if server_name else None
        if not url:
            return {"tools": []}
        import requests
        resp = requests.post(url, json={"jsonrpc": "2.0", "method": "tools/list", "id": 1}, timeout=5)
        return resp.json().get("result", {"tools": []})

    return {"tools": []}

def main():
    import requests

    # Initialize the actual HTTP servers
    for name, url in MCP_SERVERS.items():
        try:
            requests.post(url, json={"jsonrpc": "2.0", "method": "initialize", "id": 1,
                         "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "bridge", "version": "1.0"}}}, timeout=5)
        except:
            pass

    while True:
        line = sys.stdin.readline()
        if not line:
            break
        try:
            req = json.loads(line)
            result = handle_request(req)
            response = {"jsonrpc": "2.0", "id": req.get("id", 1), "result": result}
            print(json.dumps(response), flush=True)
        except Exception as e:
            error = {"jsonrpc": "2.0", "id": 1, "error": {"code": -32603, "message": str(e)}}
            print(json.dumps(error), flush=True)

if __name__ == "__main__":
    main()
