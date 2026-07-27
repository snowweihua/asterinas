#!/usr/bin/env python3
"""MCP stdio server - wraps serial_mcp FastMCP server."""

import sys
sys.path.insert(0, "tools")

from serial_mcp.server import mcp

mcp.run()
