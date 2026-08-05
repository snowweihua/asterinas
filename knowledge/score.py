from . import config
from .schema import Memory, get_all_memories, update_memory, init_db


def score_all():
    init_db()
    memories = get_all_memories()
    print(f"[score] Scoring {len(memories)} memories", flush=True)

    for mem in memories:
        changed = False
        source_count = len(mem.sources) if mem.sources else 1
        old_confidence = mem.confidence

        new_confidence = min(100, source_count * 15 + (20 if mem.verified else 0))
        if new_confidence != old_confidence:
            mem.confidence = new_confidence
            changed = True

        if mem.importance < 1:
            mem.importance = 5
            changed = True
        elif mem.importance > 10:
            mem.importance = 10
            changed = True

        if changed:
            update_memory(mem)

    print(f"[score] Done scoring {len(memories)} memories", flush=True)
    return len(memories)
