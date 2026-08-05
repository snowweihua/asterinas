import json
import pathlib

from . import config


def ingest():
    config.RAW_DIR.mkdir(parents=True, exist_ok=True)
    out_dir = config.RAW_DIR / "devin"
    out_dir.mkdir(parents=True, exist_ok=True)

    count = 0
    for path in pathlib.Path(config.DEVIN_SESSION_GLOB).parent.glob(pathlib.Path(config.DEVIN_SESSION_GLOB).name):
        try:
            session = parse_devin_session(path)
            out_path = out_dir / f"{session['session_id']}.json"
            with open(out_path, "w") as f:
                json.dump(session, f, indent=2, ensure_ascii=False)
            count += 1
        except Exception as e:
            print(f"[devin_parser] Error on {path}: {e}", flush=True)
            continue

    print(f"[devin_parser] Ingested {count} sessions", flush=True)
    return count


def parse_devin_session(path: pathlib.Path) -> dict:
    messages = []
    session_id = path.stem.replace("session_", "").replace("_", "-")

    with open(path, "r", encoding="utf-8", errors="replace") as f:
        data = json.load(f)

    date = None
    if isinstance(data, dict):
        ts = data.get("created_at")
        if ts:
            import datetime
            date = datetime.datetime.fromtimestamp(ts).strftime("%Y-%m-%d")

        backend = data.get("backend_type", "devin")
        model = data.get("model", "")
        title = data.get("title", "")

        cogs = data.get("cogs_json", [])
        if isinstance(cogs, list):
            for cog in cogs:
                for msg in cog.get("messages", []):
                    role = msg.get("role", "assistant")
                    content = msg.get("content", "")
                    if content:
                        messages.append({
                            "role": role,
                            "content": content,
                            "tool_calls": [],
                            "timestamp": None
                        })

        for msg in data.get("messages", []):
            role = msg.get("role", "assistant")
            content = msg.get("content", "")
            if content:
                messages.append({
                    "role": role,
                    "content": content,
                    "tool_calls": [],
                    "timestamp": None
                })

    return {
        "session_id": session_id,
        "date": date,
        "source": "devin",
        "messages": messages
    }
