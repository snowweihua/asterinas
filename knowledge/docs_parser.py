import json
import pathlib
import re

from . import config


def ingest():
    config.RAW_DIR.mkdir(parents=True, exist_ok=True)
    out_dir = config.RAW_DIR / "docs"
    out_dir.mkdir(parents=True, exist_ok=True)

    count = 0

    for docs_dir in config.DOCS_DIRS:
        if not docs_dir.exists():
            continue
        for md_file in docs_dir.rglob("*.md"):
            try:
                session = parse_markdown(md_file, source_name=f"docs:{md_file.relative_to(docs_dir.parent)}")
                if session["messages"]:
                    out_path = out_dir / f"{session['session_id']}.json"
                    with open(out_path, "w") as f:
                        json.dump(session, f, indent=2, ensure_ascii=False)
                    count += 1
            except Exception as e:
                print(f"[docs_parser] Error on {md_file}: {e}", flush=True)
                continue

    for docs_file in config.DOCS_FILES:
        if not docs_file.exists():
            continue
        try:
            session = parse_markdown(docs_file, source_name=f"docs:{docs_file.name}")
            if session["messages"]:
                out_path = out_dir / f"{session['session_id']}.json"
                with open(out_path, "w") as f:
                    json.dump(session, f, indent=2, ensure_ascii=False)
                count += 1
        except Exception as e:
            print(f"[docs_parser] Error on {docs_file}: {e}", flush=True)
            continue

    print(f"[docs_parser] Ingested {count} documents", flush=True)
    return count


def parse_markdown(path: pathlib.Path, source_name: str = "") -> dict:
    with open(path, "r", encoding="utf-8", errors="replace") as f:
        content = f.read()

    messages = []
    current_section = ""
    current_body = []

    for line in content.splitlines():
        header_match = re.match(r"^(#{1,3})\s+(.+)", line)
        if header_match:
            if current_body:
                text = "\n".join(current_body).strip()
                if text:
                    messages.append({
                        "role": "document",
                        "content": text,
                        "topic": current_section,
                        "tool_calls": [],
                        "timestamp": None
                    })
                current_body = []
            current_section = header_match.group(2).strip()
        else:
            current_body.append(line)

    if current_body:
        text = "\n".join(current_body).strip()
        if text:
            messages.append({
                "role": "document",
                "content": text,
                "topic": current_section,
                "tool_calls": [],
                "timestamp": None
            })

    return {
        "session_id": f"doc_{path.stem}",
        "date": None,
        "source": source_name or f"docs:{path.name}",
        "messages": messages
    }
