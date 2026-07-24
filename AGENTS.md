# AArch64 Development Agent Instructions

## last session status refer to specs/001-rpi3-hardware-bringup/SESSION_CONTEXT.md

## Current work flow (must follow exactly)

**Research and analysis -> code change -> Build and Deploy (via build MCP) -> do power off/on through power MCP -> capture serial log via serial MCP -> analyze log -> If new progress, commit code -> continue next research and analysis**

### Detailed steps:

1. **Code change** — Edit source files as needed
 2. **Build & Deploy** — Use the build MCP server (port 8912, must be running in user's terminal):
   - `build-mcp_build_kernel` (opencode mcp tool) — Docker cargo osdk build for aarch64
   - `build-mcp_convert_kernel` (opencode mcp tool) — objcopy ELF → raw binary (use `/tmp/asterina.img` as output_img)
   - Deploy via bash `cp /tmp/asterina.img /mnt/d/pi_sd/asterina.img` (build-mcp_deploy_kernel fails with permission denied)
3. **Power cycle** — Use the power MCP server (port 8911, must be running in user's terminal):
   - Power OFF: `power-mcp_power_off` (opencode mcp tool) 
   - Power ON: `power-mcp_power_on` (opencode mcp tool)
   - Wait ~45s after power ON before reading serial for full boot
4. **Read serial** — Use serial MCP server (port 8910, must be running in user's terminal):
   - Clear buffer: `serial-mcp_serial_read [timeout_ms=300000, clear_buffer=true]`
   - Capture boot: `serial-mcp_serial_read [timeout_ms=300000, clear_buffer=false]`
5. **Analyze** — Look for kernel boot markers, Synchronous Abort handler, etc.
6. **Commit** — Only commit when there's actual progress (new understanding, working fix). Use:
   ```bash
   git add -A && git commit -m "<area>: <what changed> — <why/result>"
   ```

### MCP Servers (start in separate terminals before session)

**Terminal 1 (serial):**
```bash
cd /home/snow/asterinas && python3 tools/serial_mcp_server.py
```

**Terminal 2 (build+deploy):**
```bash
cd /home/snow/asterinas && python3 tools/build_mcp_server.py
```

**Terminal 3 (power control):**
```bash
cd /home/snow/asterinas && python3 tools/power_mcp_server.py
```

## Git rules

### "Commit on Progress" — Non-Negotiable

**After any meaningful change** (new understanding, working fix), commit immediately with a descriptive message.

### Commit Message Format
```
<area>: <what changed> — <why/result>
```
Example: `aarch64/cpu: replace spin::Once with SimpleOnce — fixes RPi3 boot hang at enable_cpu_features`
