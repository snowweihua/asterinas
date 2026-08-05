import json
import pathlib
from typing import Iterator

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
            data = entry.get("data", {})

            if msg_type == "user":
                content = data.get("input", "")
                role = "user"
                tool_calls = []
            elif msg_type == "assistant":
                content = ""
                tool_calls = []
                if isinstance(data, dict):
                    content = data.get("content", "")
                    for tc in data.get("toolRequests", []):
                        tool_calls.append({
                            "tool_name": tc.get("name", ""),
                            "tool_args": tc.get("arguments", "")
                        })
                role = "assistant"
            else:
                continue

            timestamp = entry.get("timestamp") or entry.get("createdAt")
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
