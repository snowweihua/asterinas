import json
import pathlib
import datetime

from . import config


def ingest(incremental: bool = False):
    config.RAW_DIR.mkdir(parents=True, exist_ok=True)
    out_dir = config.RAW_DIR / "devin"
    out_dir.mkdir(parents=True, exist_ok=True)

    state = {}
    if incremental:
        state = config.load_ingest_state()
        if "devin" not in state:
            state["devin"] = {}

    count = 0
    skipped = 0
    for path in pathlib.Path(config.DEVIN_SESSION_GLOB).parent.glob(pathlib.Path(config.DEVIN_SESSION_GLOB).name):
        try:
            session = parse_devin_session(path)
            session_id = session["session_id"]

            if incremental and session_id in state.get("devin", {}):
                skipped += 1
                continue

            out_path = out_dir / f"{session_id}.json"
            with open(out_path, "w") as f:
                json.dump(session, f, indent=2, ensure_ascii=False)
            count += 1
            if incremental:
                state.setdefault("devin", {})[session_id] = True
        except Exception as e:
            print(f"[devin_parser] Error on {path}: {e}", flush=True)
            continue

    if incremental and count > 0:
        config.save_ingest_state(state)

    print(f"[devin_parser] Ingested {count} sessions (skipped {skipped} already processed)", flush=True)
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
            date = datetime.datetime.fromtimestamp(ts).strftime("%Y-%m-%d")

        for msg in data.get("messages", []):
            chat = msg.get("chat_message", {})
            if not isinstance(chat, dict):
                continue
            role = chat.get("role", "assistant")
            content = chat.get("content", "")
            if content and str(content).strip():
                messages.append({
                    "role": role,
                    "content": content,
                    "timestamp": None
                })

    return {
        "session_id": session_id,
        "date": date,
        "source": "devin",
        "messages": messages
    }
