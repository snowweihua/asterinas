# AArch64 Development Agent Instructions

## last session status refer to specs/001-rpi3-hardware-bringup/SESSION_CONTEXT.md

## Development Environment

**Main development is in WSL2.**
**TFTP runs on Windows with root directory `D:/pi_sd/`; `/srv/tftp` is not the TFTP root.** The WSL2 mapping is `/mnt/d/pi_sd/`. This is not the physical SD card, so if SD-card files need updating (for example, `boot.scr`), ask the user to copy them manually.
**Build MCP runs in WSL2; serial/power MCP runs in Windows.**

## Current work flow (must follow exactly)

**Research and analysis -> code change -> Build and Deploy -> power off -> serial buffer clear -> power on -> wait 60s and serial read -> analyze log -> If new progress, commit code -> continue next research and analysis**

### Detailed steps:

1. **Code change** — Edit source files as needed
2. **Build & Deploy** — Use the build MCP server:
   - `build_kernel_tool(target="rpi3"|"qemu")` — Start Docker cargo osdk build (runs in background, ~2-5 min)
     - First call starts the build and returns "BUILD STARTED"
     - Poll repeatedly until result appears (returns "BUILD OK" or "BUILD FAILED")
   - `convert_kernel_tool(source_elf, output_img)` — objcopy ELF → raw binary (use `/tmp/asterina.img` as output_img for rpi3, `/tmp/qemu.bin` for qemu)
   - `deploy_kernel_tool(source_img, deploy_path)` — copy raw binary to SD card mount point
   - `build_and_deploy_tool(target="rpi3"|"qemu", deploy_path)` — Start build, poll for result, then convert and (for rpi3) deploy
3. **Power cycle** — Use the power MCP server:
   - `power_power_off_tool` — Power OFF the RPi3B board
   - `power_power_on_tool` — Power ON the RPi3B board
   - `power_power_status_tool` — Check power switch status (returns CH1:ON/OFF CH2:ON/OFF)
   - Wait ~60s after power ON before reading serial for full boot
4. **Read serial** — Use serial MCP server:
   - `serial_serial_clear` — Clear buffered serial output
   - `serial_serial_read` — Read accumulated serial output (max 5s wait per call)
   - `serial_serial_write` — Send text to serial port
   - `serial_serial_wait` — Wait until regex pattern appears (max 30s)
   - `serial_serial_is_open` — Check if serial port is connected
   - **Note**: For long reads (e.g., boot capture), poll repeatedly with `serial_serial_read timeout_ms=5000` until empty response confirms completion
5. **Analyze** — Look for kernel boot markers, Synchronous Abort handler, etc.
6. **Commit** — Only commit when there's actual progress (new understanding, working fix). Use:
   ```bash
   git add -A && git commit -m "<area>: <what changed> — <why/result>"
   ```

## MCP Servers

For opencode, MCP servers are registered in `.opencode/opencode.json` and for Devin, MCP server are registered in '.devin/mcp_config.local.json', started automatically. 

**Available MCP tools:**

| Server | Tools |
|--------|-------|
| build | `build_kernel_tool`, `convert_kernel_tool`, `deploy_kernel_tool`, `build_and_deploy_tool` |
| power | `power_on_tool`, `power_off_tool`, `power_status_tool` |
| serial | `serial_read`, `serial_write`, `serial_wait`, `serial_clear`, `serial_capture`, `serial_is_open`, `serial_reconnect` |

## Git rules

### "Commit on Progress" — Non-Negotiable

**After any meaningful change** (new understanding, working fix), commit immediately with a descriptive message.

### Commit Message Format
```
<area>: <what changed> — <why/result>
```
Example: `aarch64/cpu: replace spin::Once with SimpleOnce — fixes RPi3 boot hang at enable_cpu_features`

## Development rules

### Skipping is not fixing, just avoiding, Don't treat skipping as fixing!
