# Knowledge Distillation Pipeline — Implementation Plan

**Based on:** `docs/superpowers/specs/2026-08-05-knowledge-distillation-pipeline-design.md`
**Project:** knowledge/
**Status:** In Progress

---

## Phase 0: Foundation

### Task 1 — Create `knowledge/config.py`
- Define source paths (opencode transcripts, copilot, devin, docs)
- Define LLM endpoint (Minimax via OpenAI-compatible API)
- Define staging DB path (`knowledge/staging/memories.db`)
- Define raw data path (`knowledge/raw/`)
- Define opencode-mem publish endpoint (`http://localhost:4747/api/memories`)
- Use environment variable `MINIMAX_API_KEY` for token plan auth
- Default to f0g.dev gpt-5.5 as fallback if Minimax unavailable

### Task 2 — Create `knowledge/schema.py`
- Define `Memory` dataclass matching the schema
- Define `Chunk` dataclass
- Define `SourceStats` dataclass
- Create SQLite tables: `memories`, `chunks`, `source_stats`
- Create FTS5 virtual table `memories_fts` on title, summary, details, tags
- Add DB helper: `get_db_connection()`, init_db()

---

## Phase 1: Source Parsing

### Task 3 — Create `knowledge/opencode_parser.py`
- Read `~/.claude/transcripts/ses_*.jsonl`
- Parse JSONL, extract: session_id, timestamp, messages (role, content, tool_calls)
- Normalize to: `{session_id, date, messages: [{role, content, tool_name, tool_args}]}`
- Store normalized sessions in `knowledge/raw/opencode/`

### Task 4 — Create `knowledge/copilot_parser.py`
- Read Copilot transcript JSONL from workspace storage
- Extract `assistant.message` blocks with content + tool_calls
- Normalize same format as opencode_parser
- Store in `knowledge/raw/copilot/`

### Task 5 — Create `knowledge/devin_parser.py`
- Read `/tmp/session_*.json`
- Parse dict with `cogs_json` + `messages` fields
- Extract message content from both fields
- Normalize to standard format
- Store in `knowledge/raw/devin/`

### Task 6 — Create `knowledge/docs_parser.py`
- Read markdown files from `specs/` and `SESSION_CONTEXT.md`
- Segment by H1/H2 headings
- Store as `{source: 'docs', session_id: filepath, topic, content}`

---

## Phase 2: Chunking

### Task 7 — Create `knowledge/chunk.py`
- Load normalized sessions from `knowledge/raw/{source}/`
- Split into topic-focused chunks (max ~2000 tokens)
- For transcripts: split on conversation turns, merge small chunks
- For docs: split on headings
- Output: JSON files in `knowledge/staging/chunks/` with `chunk_id`, `source_ref`, `content`, `topic_hint`, `token_count`
- Token counting: simple whitespace split (no tiktoken needed for MVP)

---

## Phase 3: Distillation

### Task 8 — Create `knowledge/distill.py`
- LLM call wrapper: `call_llm(prompt, system_prompt) -> str`
- Use OpenAI-compatible API client (`openai` pip package or `requests`)
- Auth via `MINIMAX_API_KEY` env var with Minimax base URL, or fallback to f0g.dev
- Distillation prompt: extract structured memory JSON from a conversation chunk
- Output: list of Memory objects (may be empty)
- Handle rate limits and retries (3 retries, exponential backoff)
- Write results to `knowledge/staging/memories.db`

---

## Phase 4: Merging & Scoring

### Task 9 — Create `knowledge/merge.py`
- FTS5 similarity search for deduplication
- Threshold: >70% similarity = merge
- Merge logic: highest confidence wins, combine tags, append sources
- Write merged results back to `memories.db`

### Task 10 — Create `knowledge/score.py`
- confidence = source_count × 15 + (verified ? 20 : 0), capped at 100
- importance = 1-10 based on evidence breadth (manual for now)
- verified = False by default (only true if confirmed by serial log or working code)
- Update `memories.db`

---

## Phase 5: Publishing

### Task 11 — Create `knowledge/publish.py`
- Read memories from `staging/memories.db`
- POST to opencode-mem at `http://localhost:4747/api/memories`
- Auto-publish: confidence >= 80 OR verified == True
- Others go to `staging/review_queue.json`
- Skip if memory with same title+source already published (dedup)

---

## Phase 6: CLI

### Task 12 — Create `knowledge/pipeline.py`
```bash
python3 -m knowledge.pipeline ingest --sources opencode,copilot,devin,docs
python3 -m knowledge.pipeline chunk --max-tokens 2000
python3 -m knowledge.pipeline distill --batch-size 10
python3 -m knowledge.pipeline merge
python3 -m knowledge.pipeline score
python3 -m knowledge.pipeline publish --min-confidence 80
python3 -m knowledge.pipeline --full
python3 -m knowledge.pipeline query "UART LSR bit6"
python3 -m knowledge.pipeline stats
```

### Task 13 — Create `knowledge/__main__.py`
- Entry point for `python3 -m knowledge`

---

## Task Dependencies

```
config.py, schema.py
     │
     ├── opencode_parser.py ──┐
     ├── copilot_parser.py ──┤
     ├── devin_parser.py ───┤
     └── docs_parser.py ────┴─► ingest.py
                                   │
chunk.py ◄─────────────────────────┘
     │
     │ (staging/chunks/)
     ▼
distill.py ──► memories.db
     │
     ▼
merge.py ──► memories.db (updated)
     │
     ▼
score.py ──► memories.db (updated)
     │
     ▼
publish.py ──► opencode-mem
     │
     ▼
pipeline.py (orchestrator)
     │
     ▼
__main__.py
```

---

## Verification

### Verify pipeline.py --help
```bash
python3 -m knowledge.pipeline --help
# Expected: shows ingest/chunk/distill/merge/score/publish commands
```

### Verify ingest on one opencode session
```bash
python3 -m knowledge.pipeline ingest --sources opencode
# Expected: creates knowledge/raw/opencode/ with normalized JSON files
```

### Verify chunk
```bash
python3 -m knowledge.pipeline chunk
# Expected: creates knowledge/staging/chunks/ with chunked conversation files
```

### Verify distill (dry run on 1 chunk)
```bash
# Set MINIMAX_API_KEY env var first
python3 -m knowledge.pipeline distill --batch-size 1
# Expected: calls LLM, writes memories to memories.db
```
