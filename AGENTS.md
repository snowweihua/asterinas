# AArch64 Development Agent Instructions

## last session status refer to specs/001-rpi3-hardware-bringup/SESSION_CONTEXT.md
## development important information refer to specs/001-rpi3-hardware-bringup/quickstart.md

## Current work flow (must follow exactly)

**Research and analysis -> code change -> Build and Deploy -> Ask user to power on RPi3B -> capture serial log via MCP -> analyze log -> If new progress, commit code -> continue next research and analysis**

### Detailed steps:

1. **Code change** — Edit source files as needed
2. **Build** — Run in Docker container (fast, ~1 min):
   ```bash
   cd /home/snow/asterinas && docker run --rm -v $(pwd):/root/asterinas asterinas/aarch64-dev:latest bash -c 'cd /root/asterinas && cargo osdk build --release --target-arch aarch64 --boot-method qemu-direct --scheme aarch64'
   ```
3. **Deploy** — Use the deploy MCP server (port 8911, runs in user's WSL2 terminal):
   ```bash
   curl -s -X POST http://localhost:8911 -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"deploy_kernel","arguments":{}}}'
   ```
   This runs `aarch64-linux-gnu-objcopy` + copies to `/mnt/d/pi_sd/asterina.img`.
4. **Clear serial buffer** — Clear stale data from serial MCP:
   ```bash
   curl -s -X POST http://localhost:8910 -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"serial_read","arguments":{"timeout_ms":100,"clear_buffer":true}}}'
   ```
5. **Ask user** — Use `ask` tool with options: `"Done — read log"` / `"Need more time"`
6. **Capture log** — Read serial output via MCP (port 8910, runs in user's WSL2 terminal):
   ```bash
   curl -s -X POST http://localhost:8910 -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"serial_read","arguments":{"timeout_ms":300000,"clear_buffer":false}}}'
   ```
7. **Analyze** — Look for kernel boot markers (e.g., `[fa.canary]`, `[node.alloc]`, `Synchronous Abort` handler)
8. **Commit** — `git add -A && git commit -m "<area>: <what> — <why>"`

### If MCP servers are down at session start:

Ask user to run these in two separate WSL2 terminals:

**Terminal 1 (serial):**
```bash
cd /home/snow/asterinas && python3 tools/serial_mcp_server.py
```

**Terminal 2 (deploy):**
```bash
cd /home/snow/asterinas && python3 tools/deploy_mcp_server.py
```

Then verify both are running:
```bash
curl -s http://localhost:8910  # should respond
curl -s http://localhost:8911  # should respond
```

## Git rules

### Time to commit
**When new progress, commit it immediately, then do next debugging

### "Commit on Progress" — Non-Negotiable

**After any meaningful change**, commit immediately with a descriptive message:
- Code change that fixes or changes behavior
- Document update with new understanding

**Before ANY risky operation** (checkout/reset/stash):
```bash
git add -A && git commit -m "WIP: <description>"
```

### Commit Message Format
```
<area>: <what changed> — <why/result>
```
Example: `aarch64/cpu: replace spin::Once with SimpleOnce — fixes RPi3 boot hang at enable_cpu_features`


## Current Session: Generic Result Return Crash at `ret` on AArch64 RPi3

**Root Cause**: `alloc_frame_with<M>() -> Result<Frame<M>, Error>` crashes at `ret` when returning ANY value on AArch64 RPi3. Not a compiler bug, not boot PT. Spin loop works (avoids return).
**Fixes applied this session**: boot PT slots 1-3 zeroed on RPi3, `new_kernel_page_table()` copies boot PT entries, linear+meta mapping skipped on AArch64, `KERNEL_PAGE_TABLE` → `SimpleOnce`.
**Blocker**: `PageTableNode::alloc()` still calls `alloc_frame_with()` → spin loop. Need to provide a working Frame allocation that returns `Ok` instead of `Err`.
**MCP tools**: serial_mcp_server (port 8910), deploy_mcp_server (port 8911) — restart at session start if needed.
**Details**: `specs/001-rpi3-hardware-bringup/SESSION_CONTEXT.md`





