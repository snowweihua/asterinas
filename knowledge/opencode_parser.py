import json
import pathlib

from . import config


def iter_opencode_transcripts():
    for p in config.OPENCODE_TRANSCRIPT_DIR.glob("ses_*.jsonl"):
        yield p


def parse_opencode_session(path: pathlib.Path) -> dict:
    messages = []
    session_id = path.stem
    date = None

    with open(path, "r", encoding="utf-8", errors="replace") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                entry = json.loads(line)
            except json.JSONDecodeError:
                continue

            msg_type = entry.get("type", "")
            timestamp = entry.get("timestamp") or entry.get("createdAt")
            if not date and timestamp:
                date = timestamp[:10]

            if msg_type == "user":
                content = entry.get("content", "")
                messages.append({
                    "role": "user",
                    "content": content,
                    "timestamp": timestamp
                })
            elif msg_type == "tool_use":
                tool_name = entry.get("tool_name", "")
                tool_input = entry.get("tool_input", {})
                content = f"[tool: {tool_name}] {json.dumps(tool_input)}"
                messages.append({
                    "role": "assistant",
                    "content": content,
                    "timestamp": timestamp
                })
            elif msg_type == "tool_result":
                tool_name = entry.get("tool_name", "")
                tool_output = entry.get("tool_output", {})
                if isinstance(tool_output, dict):
                    output_str = json.dumps(tool_output)
                else:
                    output_str = str(tool_output)
                content = f"[result: {tool_name}] {output_str}"
                messages.append({
                    "role": "assistant",
                    "content": content,
                    "timestamp": timestamp
                })
            elif msg_type == "assistant":
                data = entry.get("data", {})
                if isinstance(data, dict):
                    content = data.get("content", "")
                    if content:
                        messages.append({
                            "role": "assistant",
                            "content": content,
                            "timestamp": timestamp
                        })

    return {
        "session_id": session_id,
        "date": date,
        "source": "opencode",
        "messages": messages
    }


def ingest():
    config.RAW_DIR.mkdir(parents=True, exist_ok=True)
    out_dir = config.RAW_DIR / "opencode"
    out_dir.mkdir(parents=True, exist_ok=True)

    count = 0
    for path in iter_opencode_transcripts():
        try:
            session = parse_opencode_session(path)
            out_path = out_dir / f"{session['session_id']}.json"
            with open(out_path, "w") as f:
                json.dump(session, f, indent=2, ensure_ascii=False)
            count += 1
        except Exception as e:
            print(f"[opencode_parser] Error on {path}: {e}", flush=True)
            continue

    print(f"[opencode_parser] Ingested {count} sessions", flush=True)
    return count
