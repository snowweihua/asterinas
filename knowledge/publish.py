import json
import os
import hashlib
import subprocess
import requests
from pathlib import Path

from . import config
from .schema import Memory, get_all_memories, init_db


PROJECT_PATH = str(Path(__file__).parent.parent.resolve())
PROJECT_NAME = "asterinas"


def get_project_container_tag() -> str:
    try:
        remote = subprocess.check_output(
            ["git", "config", "--get", "remote.origin.url"], text=True, stderr=subprocess.DEVNULL
        ).strip()
        project_identity = f"remote:{remote}"
        hash_val = hashlib.sha256(project_identity.encode()).hexdigest()[:16]
        return f"opencode_project_{hash_val}"
    except Exception:
        return f"{PROJECT_NAME}_project_"


PROJECT_TAG = get_project_container_tag()


def memory_exists_by_title(title: str) -> bool:
    try:
        resp = requests.get(
            config.OPENCODE_MEM_API,
            params={"tag": PROJECT_TAG, "pageSize": 100},
            timeout=10
        )
        if resp.status_code == 200:
            data = resp.json()
            items = data.get("data", {}).get("items", [])
            for item in items:
                content = item.get("content", "")
                if title in content:
                    return True
    except Exception:
        pass
    return False


def publish_memory(mem: Memory) -> bool:
    payload = {
        "content": f"**{mem.title}** ({mem.type})\n\n{ mem.summary }\n\n{ mem.details }\n\nArea: {mem.area or 'general'} | Confidence: {mem.confidence} | Verified: {mem.verified}",
        "containerTag": PROJECT_TAG,
        "projectPath": PROJECT_PATH,
        "projectName": PROJECT_NAME,
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
            if memory_exists_by_title(mem.title):
                skipped += 1
                continue
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
