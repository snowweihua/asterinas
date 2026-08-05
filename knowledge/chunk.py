import json
import pathlib
from datetime import datetime
from typing import Iterator

from . import config
from .schema import Chunk, init_db, insert_chunk


def count_tokens(text: str) -> int:
    return len(text.split())


def chunk_session(session_data: dict, max_tokens: int = None) -> list:
    if max_tokens is None:
        max_tokens = config.DEFAULT_CHUNK_MAX_TOKENS

    chunks = []
    messages = session_data.get("messages", [])
    if not messages:
        return chunks

    session_id = session_data.get("session_id", "unknown")
    source = session_data.get("source", "unknown")

    current_chunk_lines = []
    current_tokens = 0
    chunk_index = 0
    current_topic = "general"

    def emit_chunk():
        nonlocal current_chunk_lines, current_tokens, chunk_index
        if not current_chunk_lines:
            return
        content = "\n".join(current_chunk_lines)
        chunk_id = f"{session_id}_chunk_{chunk_index:04d}"
        chunk = Chunk(
            chunk_id=chunk_id,
            source=source,
            session_id=session_id,
            topic_hint=current_topic,
            content=content,
            token_count=current_tokens,
            created=datetime.now().isoformat()
        )
        chunks.append(chunk)
        current_chunk_lines = []
        current_tokens = 0
        chunk_index += 1

    for msg in messages:
        content = msg.get("content", "")
        topic_hint = msg.get("topic", current_topic)

        if topic_hint != current_topic and topic_hint != "general":
            if current_chunk_lines:
                emit_chunk()
            current_topic = topic_hint

        if not content:
            continue

        msg_tokens = count_tokens(content)

        if msg_tokens > max_tokens:
            if current_chunk_lines:
                emit_chunk()
            lines = content.split("\n")
            sub_chunk_lines = []
            sub_tokens = 0
            for ln in lines:
                ln_tokens = count_tokens(ln)
                if sub_tokens + ln_tokens > max_tokens:
                    if sub_chunk_lines:
                        content_piece = "\n".join(sub_chunk_lines)
                        chunk_id = f"{session_id}_chunk_{chunk_index:04d}"
                        chunk = Chunk(
                            chunk_id=chunk_id,
                            source=source,
                            session_id=session_id,
                            topic_hint=current_topic,
                            content=content_piece,
                            token_count=sub_tokens,
                            created=datetime.now().isoformat()
                        )
                        chunks.append(chunk)
                        chunk_index += 1
                        sub_chunk_lines = []
                        sub_tokens = 0
                sub_chunk_lines.append(ln)
                sub_tokens += ln_tokens
            if sub_chunk_lines:
                content_piece = "\n".join(sub_chunk_lines)
                chunk_id = f"{session_id}_chunk_{chunk_index:04d}"
                chunk = Chunk(
                    chunk_id=chunk_id,
                    source=source,
                    session_id=session_id,
                    topic_hint=current_topic,
                    content=content_piece,
                    token_count=sub_tokens,
                    created=datetime.now().isoformat()
                )
                chunks.append(chunk)
                chunk_index += 1
            continue

        if current_tokens + msg_tokens > max_tokens:
            emit_chunk()

        current_chunk_lines.append(content)
        current_tokens += msg_tokens

    if current_chunk_lines:
        emit_chunk()

    return chunks


def chunk_all(sources: list = None):
    init_db()
    config.CHUNKS_DIR.mkdir(parents=True, exist_ok=True)

    if sources is None:
        sources = ["opencode", "copilot", "devin", "docs"]

    total = 0
    for source in sources:
        raw_dir = config.RAW_DIR / source
        if not raw_dir.exists():
            continue

        for json_file in raw_dir.glob("*.json"):
            try:
                with open(json_file) as f:
                    session_data = json.load(f)
                chunks = chunk_session(session_data)
                for ch in chunks:
                    insert_chunk(ch)
                total += len(chunks)
            except Exception as e:
                print(f"[chunk] Error on {json_file}: {e}", flush=True)
                continue

    print(f"[chunk] Created {total} chunks across {len(sources)} sources", flush=True)
    return total
