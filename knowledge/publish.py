import json
import requests
from typing import Optional

from . import config
from .schema import Memory, get_all_memories, init_db


def publish_memory(mem: Memory) -> bool:
    payload = {
        "content": f"**{mem.title}** ({mem.type})\n\n{ mem.summary }\n\n{ mem.details }\n\nArea: {mem.area or 'general'} | Confidence: {mem.confidence} | Verified: {mem.verified}",
        "containerTag": mem.type,
        "tags": mem.tags + [mem.type, mem.area or "general"] + mem.sources
    }

    for attempt in range(3):
        try:
            resp = requests.post(
                config.OPENCODE_MEM_API,
                json=payload,
                timeout=10
            )
            if resp.status_code in (200, 201):
                return True
            elif resp.status_code == 409:
                return True
            else:
                if attempt < 2:
                    continue
                print(f"[publish] Failed to publish '{mem.title}': {resp.status_code}", flush=True)
                return False
        except Exception as e:
            if attempt < 2:
                continue
            print(f"[publish] Exception publishing '{mem.title}': {e}", flush=True)
            return False

    return False


def publish_all(min_confidence: int = None):
    if min_confidence is None:
        min_confidence = config.DEFAULT_MIN_CONFIDENCE

    init_db()
    memories = get_all_memories()

    review_queue = []
    published = 0
    skipped = 0

    config.STAGING_DIR.mkdir(parents=True, exist_ok=True)

    for mem in memories:
        if mem.verified or mem.confidence >= min_confidence:
            if publish_memory(mem):
                published += 1
            else:
                skipped += 1
        else:
            review_queue.append({
                "id": mem.id,
                "title": mem.title,
                "type": mem.type,
                "area": mem.area,
                "summary": mem.summary,
                "details": mem.details,
                "confidence": mem.confidence,
                "importance": mem.importance,
                "tags": mem.tags,
                "sources": mem.sources
            })

    review_path = config.STAGING_DIR / "review_queue.json"
    with open(review_path, "w") as f:
        json.dump(review_queue, f, indent=2)

    print(f"[publish] Published: {published}, Skipped: {skipped}, Review queue: {len(review_queue)}", flush=True)
    return published
