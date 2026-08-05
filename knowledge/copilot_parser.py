import json
import pathlib

from . import config


def ingest():
    config.RAW_DIR.mkdir(parents=True, exist_ok=True)
    out_dir = config.RAW_DIR / "copilot"
    out_dir.mkdir(parents=True, exist_ok=True)

    count = 0
    for glob_pattern in config.COPILOT_TRANSCRIPT_GLOB:
        for path in pathlib.Path(glob_pattern).parent.glob(pathlib.Path(glob_pattern).name):
            try:
                session = parse_copilot_transcript(path)
                out_path = out_dir / f"{session['session_id']}.json"
                with open(out_path, "w") as f:
                    json.dump(session, f, indent=2, ensure_ascii=False)
                count += 1
            except Exception as e:
                print(f"[copilot_parser] Error on {path}: {e}", flush=True)
                continue

    print(f"[copilot_parser] Ingested {count} sessions", flush=True)
    return count


def parse_copilot_transcript(path: pathlib.Path) -> dict:
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
            data = entry.get("data", {})

            if msg_type == "assistant.message":
                content = data.get("content", "") if isinstance(data, dict) else ""
                tool_calls = []
                if isinstance(data, dict):
                    for tc in data.get("toolRequests", []):
                        tool_calls.append({
                            "tool_name": tc.get("name", ""),
                            "tool_args": tc.get("arguments", "")
                        })
                role = "assistant"
            elif msg_type == "user":
                content = data.get("input", "") if isinstance(data, dict) else ""
                role = "user"
                tool_calls = []
            else:
                continue

            timestamp = entry.get("timestamp")
            if not date and timestamp:
                date = timestamp[:10]

            messages.append({
                "role": role,
                "content": content,
                "tool_calls": tool_calls,
                "timestamp": timestamp
            })

    return {
        "session_id": session_id,
        "date": date,
        "source": "copilot",
        "messages": messages
    }
