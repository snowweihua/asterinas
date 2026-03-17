### Communication Style

- Be concise, action-oriented, and avoid unnecessary back-and-forth.
- Start work directly when the user intent is clear.

### Workspace Workflow Rules

- Do not ask for approval for routine `/tmp` read/write operations.
- Do not store progress, status, logs, checkpoints, or recovery notes in `/tmp`.
- Treat `/tmp` as ephemeral scratch space only.

### Persistent State and Logs

- Store all durable debugging logs and progress artifacts inside this repository.
- Preferred locations:
	- `target/agent_logs/` for run logs and command outputs.
	- `.github/agent_state/` for session notes, recovery checkpoints, and handoff status.
- If these folders do not exist, create them and continue.

### Recovery Safety

- Assume sessions or terminals can crash at any time.
- Any information needed to resume work must be saved to repository-local paths, not `/tmp`.
- Before ending a major debugging step, write a short recovery note to `.github/agent_state/`.

### AArch64 Debugging Preference

- For AArch64 bring-up and phase debugging, keep marker outputs and boot traces under `target/agent_logs/`.
- Avoid relying on temporary files for critical bootstrap diagnostics.
