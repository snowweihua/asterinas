import json
from typing import List

from . import config
from .schema import Memory, get_all_memories, get_memories_fts, update_memory, init_db


def similarity_score(a: str, b: str) -> float:
    a_words = set(a.lower().split())
    b_words = set(b.lower().split())
    if not a_words or not b_words:
        return 0.0
    intersection = len(a_words & b_words)
    union = len(a_words | b_words)
    return intersection / union if union > 0 else 0.0


def merge_memories(existing: Memory, new: Memory) -> Memory:
    merged_tags = list(set(existing.tags + new.tags))
    merged_sources = list(set(existing.sources + new.sources))
    merged_from = existing.merged_from + [new.id]
    best_confidence = max(existing.confidence, new.confidence)
    best_importance = max(existing.importance, new.importance)
    best_verified = existing.verified or new.verified

    return Memory(
        id=existing.id,
        title=existing.title,
        type=existing.type,
        area=existing.area or new.area,
        summary=existing.summary if existing.confidence >= new.confidence else new.summary,
        details=existing.details if existing.confidence >= new.confidence else new.details,
        confidence=best_confidence,
        importance=best_importance,
        verified=best_verified,
        tags=merged_tags,
        sources=merged_sources,
        chunk_ref=existing.chunk_ref,
        created=existing.created,
        merged_from=merged_from
    )


def merge_all(similarity_threshold: float = None):
    if similarity_threshold is None:
        similarity_threshold = config.DEFAULT_SIMILARITY_THRESHOLD

    init_db()
    memories = get_all_memories()
    print(f"[merge] Starting with {len(memories)} memories", flush=True)

    merged_count = 0
    i = 0
    while i < len(memories):
        current = memories[i]
        search_query = f"{current.title} {current.summary}"[:200]

        candidates = get_memories_fts(search_query, limit=20)

        best_match = None
        best_score = 0.0

        for candidate in candidates:
            if candidate.id == current.id:
                continue
            score = similarity_score(
                f"{current.title} {current.summary}",
                f"{candidate.title} {candidate.summary}"
            )
            if score > best_score:
                best_score = score
                best_match = candidate

        if best_match and best_score >= similarity_threshold:
            merged = merge_memories(best_match, current)
            update_memory(merged)
            memories[i] = merged
            memories.remove(best_match)
            merged_count += 1
            print(f"[merge] Merged '{current.title}' into '{best_match.title}' (score={best_score:.2f})", flush=True)
        else:
            i += 1

    print(f"[merge] Done. {merged_count} memories merged.", flush=True)
    return merged_count
