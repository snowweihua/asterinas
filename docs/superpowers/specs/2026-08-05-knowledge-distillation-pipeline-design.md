# Knowledge Distillation Pipeline — Design

**Date:** 2026-08-05
**Updated:** 2026-08-07
**Project:** Asterinas RPi3 Firmware — Engineering Memory System
**Status:** Implemented

---

## 1. Overview

Build a 6-stage knowledge distillation pipeline that extracts durable engineering knowledge from raw AI session logs and stores curated memories in opencode-mem. The pipeline transforms verbose, multi-session conversations into structured, searchable, versioned engineering facts.

**Goal:** Opencode-mem becomes a long-term engineering memory that survives beyond any single session or agent.

---

## 2. Knowledge Sources

All sources are project-local files, no remote APIs beyond the LLM endpoint.

| Source | Location | Format | Notes |
|--------|----------|--------|-------|
| OpenCode | `~/.claude/transcripts/ses_*.jsonl` | JSONL | Dozens of sessions, months of RPi3 history |
| GitHub Copilot | `~/.vscode-server/.../transcripts/*.jsonl` | JSONL | 1 session from 2026-07-13 |
| Devin | `/tmp/session_*.json` | JSON (dict with `cogs_json`, `messages`) | 3 sessions, 165MB total |
| Project Docs | `specs/001-rpi3-hardware-bringup/`, `SESSION_CONTEXT.md`, `AGENTS.md` | Markdown | Structured engineering records |

Windsurf: no accessible history found.

---

## 3. LLM Configuration

**Distillation Model:** MiniMax-M2.7 via Minimax token plan
**Endpoint:** OpenAI-compatible API (configured in `opencode.jsonc`)
**Fallback:** `gpt-5.5` via f0g.dev if Minimax is unavailable

---

## 4. Architecture

```
knowledge/
├── raw/                          ← Source-agnostic raw log store
│   ├── opencode/                  ← Normalized OpenCode sessions
│   ├── copilot/                  ← Normalized Copilot sessions
│   ├── devin/                    ← Normalized Devin sessions
│   └── docs/                     ← Normalized project docs
│
├── staging/
│   ├── chunks/                   ← Pre-distillation chunked conversations
│   ├── memories.db               ← SQLite staging DB (FTS5)
│   └── ingest_state.json         ← Tracks processed sessions (incremental mode)
│
├── __main__.py                   ← Entry point for python3 -m knowledge
├── pipeline.py                   ← CLI entry point (python3 -m knowledge.pipeline)
├── opencode_parser.py            ← Parse OpenCode JSONL transcripts
├── copilot_parser.py             ← Parse GitHub Copilot transcripts
├── devin_parser.py               ← Parse Devin exported sessions
├── docs_parser.py                ← Parse project markdown docs
├── chunk.py                      ← Conversation segmentation
├── distill.py                    ← LLM distillation logic
├── merge.py                      ← Deduplication and merge
├── score.py                      ← Confidence/importance scoring
├── publish.py                    ← opencode-mem publishing
├── schema.py                     ← Memory schema definitions
└── config.py                     ← LLM endpoint, DB paths, state tracking
```

---

## 5. Pipeline Stages

### Stage 1 — Ingest (`pipeline.py ingest`)
Parse raw source formats into a normalized intermediate format stored in `raw/`.

Parsers:
- `opencode_parser.py` — reads `ses_*.jsonl`, extracts messages, tool calls, assistant turns
- `copilot_parser.py` — reads Copilot JSONL transcript, extracts `assistant.message` content blocks
- `devin_parser.py` — reads Devin JSON dict, extracts from `cogs_json` and `messages` fields
- `docs_parser.py` — reads markdown files, segments by heading

Each normalized chunk gets metadata: `{source, session_id, date, turn_count}`.

**Incremental mode:** Use `--incremental` flag to skip sessions already processed. State is tracked in `staging/ingest_state.json`. New Devin sessions can be added without re-processing existing sessions.

### Stage 2 — Chunk (`pipeline.py chunk`)
Split long sessions into topic-focused chunks (max ~2000 tokens each).

Chunking strategy:
- Split on topic boundaries (detected via heading changes or significant context shifts)
- For transcript logs: split on conversation turns, merge small chunks
- For docs: split on H1/H2 headings
- Output: JSON files in `staging/chunks/` with `chunk_id`, `source_ref`, `content`, `topic_hint`

### Stage 3 — Distill (`pipeline.py distill`)
Feed each chunk to MiniMax-M2.7 with a structured prompt asking for engineering facts.

**Distillation prompt extracts:**
- Architecture decisions and the reasoning behind them
- Bug root causes (confirmed, not hypothetical)
- Verified fixes and what made them work
- Hardware quirks and platform-specific behavior
- Performance observations
- Coding conventions and patterns
- Build procedures and commands
- Failed attempts and why they failed
- Lessons learned

**Output per chunk:** JSON array of memory objects (may be empty if no knowledge found).

**Memory schema:**
```json
{
  "id": "MEM-YYYYMMDD-XXXXX",
  "title": "Short descriptive title",
  "type": "lesson | bug_root_cause | architecture | procedure | hardware_quirk | failed_attempt",
  "project": "asterinas",
  "area": "uart | allocator | boot | scheduler | ... | general",
  "summary": "One-line fact",
  "details": "Full explanation (2-5 sentences)",
  "confidence": 0-100,
  "importance": 1-10,
  "verified": false,
  "tags": ["tag1", "tag2"],
  "source": ["opencode", "copilot", "devin"],
  "chunk_ref": "path/to/chunk.json",
  "created": "YYYY-MM-DD"
}
```

Distilled memories written to `staging/memories.db`.

### Stage 4 — Merge (`pipeline.py merge`)
Detect and merge duplicate/similar memories using FTS5 similarity search.

For each new memory:
1. FTS5 search for `title + summary` similar memories (threshold: >70% similarity)
2. If duplicate found: merge by keeping highest confidence, combining tags, appending sources
3. If near-duplicate found: mark as related, link both
4. Write merged result back to `memories.db`

### Stage 5 — Score (`pipeline.py score`)
Assign quality metrics to each memory:

- **confidence**: based on source_count × verification status × LLM self-assessed certainty
- **importance**: based on how many times referenced in evidence, breadth of applicability
- **verified**: true only if confirmed by serial logs, working code, or explicit user validation

### Stage 6 — Publish (`pipeline.py publish`)
Push memories from staging DB to opencode-mem.

Mechanism: call the opencode-mem API via HTTP POST to `http://localhost:4747/api/memories` (opencode-mem web server). Memories with `verified=true` or `confidence>=80` are auto-published; others go to a review queue in `staging/review_queue.json`.

---

## 6. Staging Database Schema (SQLite + FTS5)

```sql
CREATE TABLE memories (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  type TEXT NOT NULL,
  project TEXT DEFAULT 'asterinas',
  area TEXT,
  summary TEXT NOT NULL,
  details TEXT,
  confidence INTEGER DEFAULT 50,
  importance INTEGER DEFAULT 5,
  verified INTEGER DEFAULT 0,
  tags TEXT,           -- JSON array
  sources TEXT,        -- JSON array
  chunk_ref TEXT,
  created TEXT,
  merged_from TEXT,    -- comma-separated memory IDs this was merged from
  UNIQUE(title, sources)
);

CREATE VIRTUAL TABLE memories_fts USING fts5(
  title, summary, details, tags,
  content='memories',
  content_rowid='rowid'
);

CREATE TABLE chunks (
  chunk_id TEXT PRIMARY KEY,
  source TEXT,
  session_id TEXT,
  topic_hint TEXT,
  content TEXT,
  token_count INTEGER,
  created TEXT
);

CREATE TABLE source_stats (
  source TEXT PRIMARY KEY,
  sessions_processed INTEGER,
  chunks_created INTEGER,
  memories_distilled INTEGER,
  last_run TEXT
);
```

---

## 7. CLI Interface

```bash
# Full pipeline
python3 -m knowledge.pipeline full

# Individual stages
python3 -m knowledge.pipeline ingest --sources opencode,copilot,devin,docs
python3 -m knowledge.pipeline ingest --sources devin --incremental   # Skip already-processed
python3 -m knowledge.pipeline chunk --max-tokens 2000
python3 -m knowledge.pipeline distill --batch-size 10
python3 -m knowledge.pipeline merge --similarity-threshold 0.7
python3 -m knowledge.pipeline score
python3 -m knowledge.pipeline publish --min-confidence 80

# Query staging DB
python3 -m knowledge.pipeline query "UART LSR bit6"
python3 -m knowledge.pipeline stats
```

---

## 8. Error Handling

- **LLM unavailable:** queue chunks to `staging/retry_queue.json`, retry with exponential backoff
- **Malformed source file:** log warning, skip file, continue with others
- **Duplicate memory on publish:** skip if already exists in opencode-mem (deduplicate by title+source)
- **Empty distillation result:** skip chunk, do not write to staging DB

---

## 9. Key Files

| File | Responsibility |
|------|----------------|
| `__main__.py` | Entry point for `python3 -m knowledge` |
| `pipeline.py` | CLI entry point, stage orchestration |
| `opencode_parser.py` | Parse OpenCode JSONL transcripts |
| `copilot_parser.py` | Parse GitHub Copilot transcripts |
| `devin_parser.py` | Parse Devin exported sessions |
| `docs_parser.py` | Parse project markdown docs |
| `chunk.py` | Topic segmentation and token budgeting |
| `distill.py` | LLM API calls, memory extraction prompt |
| `merge.py` | FTS5 similarity dedup and merging |
| `score.py` | Confidence/importance scoring |
| `publish.py` | opencode-mem API publishing |
| `schema.py` | Memory schema, DB creation |
| `config.py` | Paths, API endpoints, state tracking (`load_ingest_state`, `save_ingest_state`) |
