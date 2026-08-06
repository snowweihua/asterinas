import json
import re
import time
import requests
from typing import Optional

from . import config
from .schema import Memory, init_db, get_db_connection


DISTILL_SYSTEM_PROMPT = """You are an expert knowledge distillation engine for embedded systems firmware development.
Extract durable engineering facts from conversations. Output ONLY a JSON array of memory objects.
Rules:
- Extract ONLY verified facts, root causes, working solutions, and confirmed hardware behavior
- Do NOT include greetings, planning talk, or temporary hypotheses
- For each memory: title (≤10 words), type (lesson|bug_root_cause|architecture|procedure|hardware_quirk|failed_attempt),
  area (uart|allocator|boot|scheduler|mmu|interrupt|general), summary (≤25 words), details (2-5 sentences),
  tags (max 4), confidence (1-100 based on how certain you are)
- Return empty array [] if no meaningful engineering knowledge found
- JSON only, no markdown, no explanation"""


DISTILL_USER_PROMPT = """Extract engineering knowledge from this conversation chunk:

---
{content}
---

Output as JSON array of memories."""


def call_llm(prompt: str, system_prompt: str, retries: int = None) -> Optional[str]:
    if retries is None:
        retries = config.DEFAULT_RETRIES

    llm_cfg = config.get_llm_config()
    headers = {
        "Authorization": f"Bearer {llm_cfg['api_key']}",
        "Content-Type": "application/json"
    }
    payload = {
        "model": llm_cfg["model"],
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.3,
        "max_tokens": 2048
    }

    for attempt in range(retries):
        try:
            resp = requests.post(
                f"{llm_cfg['base_url']}/chat/completions",
                headers=headers,
                json=payload,
                timeout=120
            )
            if resp.status_code == 200:
                data = resp.json()
                return data["choices"][0]["message"]["content"]
            elif resp.status_code == 429:
                time.sleep(2 ** attempt)
                continue
            else:
                print(f"[distill] LLM error {resp.status_code}: {resp.text[:200]}", flush=True)
                if attempt < retries - 1:
                    time.sleep(2 ** attempt)
                    continue
                return None
        except Exception as e:
            print(f"[distill] LLM exception: {e}", flush=True)
            if attempt < retries - 1:
                time.sleep(2 ** attempt)
                continue
            return None

    return None


def extract_json(text: str) -> str:
    text = re.sub(r"<thinking>[\s\S]*?</thinking>", "", text)
    text = re.sub(r"<think>[\s\S]*?</think>", "", text)
    text = text.strip()
    start = text.find("[")
    end = text.rfind("]") + 1
    if start == -1 or end == 0:
        return ""
    return text[start:end]


def distill_chunk(chunk_data: dict) -> list:
    content = chunk_data.get("content", "")
    if not content or len(content.strip()) < 50:
        return []

    prompt = DISTILL_USER_PROMPT.format(content=content[:3000])
    response = call_llm(prompt, DISTILL_SYSTEM_PROMPT)
    if not response:
        print(f"[distill] WARN: LLM returned None/empty for {chunk_data.get('chunk_id')}", flush=True)
        return []
    if len(response) < 50:
        print(f"[distill] WARN: LLM short response for {chunk_data.get('chunk_id')}: {repr(response[:100])}", flush=True)

    try:
        json_str = extract_json(response)
        if not json_str:
            print(f"[distill] WARN: empty json_str for {chunk_data.get('chunk_id')}, response[:100]={response[:100]}", flush=True)
            return []
        items = json.loads(json_str)
        if not isinstance(items, list):
            print(f"[distill] WARN: non-list response for {chunk_data.get('chunk_id')}: {type(items)}", flush=True)
            return []
        return items
    except json.JSONDecodeError as e:
        print(f"[distill] JSON parse error: {e}, response[:200]={response[:200]}", flush=True)
        return []


def distill_all(batch_size: int = None):
    if batch_size is None:
        batch_size = config.DEFAULT_BATCH_SIZE

    init_db()
    conn = get_db_connection()
    cur = conn.cursor()
    cur.execute("SELECT chunk_id, source, session_id, topic_hint, content FROM chunks WHERE distilled = 0")
    rows = cur.fetchall()
    conn.close()

    print(f"[distill] Processing {len(rows)} chunks with batch_size={batch_size}", flush=True)
    memories_created = 0
    date_str = time.strftime("%Y-%m-%d")

    for i, row in enumerate(rows):
        chunk_id = row["chunk_id"]
        source = row["source"]
        session_id = row["session_id"]
        topic_hint = row["topic_hint"]
        content = row["content"]

        chunk_data = {"chunk_id": chunk_id, "source": source, "session_id": session_id,
                       "topic_hint": topic_hint, "content": content}

        try:
            items = distill_chunk(chunk_data)
        except Exception as e:
            print(f"[distill] Error on chunk {chunk_id}: {e}", flush=True)
            continue

        from .schema import insert_memory
        for j, item in enumerate(items):
            try:
                mem_id = f"MEM-{date_str.replace('-','')}-{chunk_id[-8:]}_{j}"
                mem = Memory(
                    id=mem_id,
                    title=item.get("title", "Untitled")[:100],
                    type=item.get("type", "lesson"),
                    area=item.get("area"),
                    summary=item.get("summary", "")[:200],
                    details=item.get("details", ""),
                    confidence=int(item.get("confidence", 50)),
                    importance=int(item.get("importance", 5)),
                    verified=False,
                    tags=item.get("tags", [])[:4],
                    sources=[source],
                    chunk_ref=chunk_id,
                    created=date_str
                )
                if insert_memory(mem):
                    memories_created += 1
            except Exception as e:
                print(f"[distill] Error creating memory: {e}", flush=True)
                continue

        conn2 = get_db_connection()
        cur2 = conn2.cursor()
        cur2.execute("UPDATE chunks SET distilled = 1 WHERE chunk_id = ?", (chunk_id,))
        conn2.commit()
        conn2.close()

        if (i + 1) % batch_size == 0:
            print(f"[distill] Processed {i + 1}/{len(rows)} chunks, {memories_created} memories created", flush=True)

    print(f"[distill] Done. Total: {memories_created} memories from {len(rows)} chunks", flush=True)
    return memories_created
