# Knowledge Distillation Pipeline

Extracts durable engineering knowledge from AI session logs and stores curated memories in opencode-mem.

## Quick Start

```bash
# Full pipeline (one-time)
python3 -m knowledge.pipeline full

# Individual stages
python3 -m knowledge.pipeline ingest
python3 -m knowledge.pipeline chunk
python3 -m knowledge.pipeline distill
python3 -m knowledge.pipeline merge
python3 -m knowledge.pipeline score
python3 -m knowledge.pipeline publish --min-confidence 80
```

## Incremental Mode (for new Devin sessions)

After exporting new sessions from Devin terminal to `/tmp/session_*.json`:

```bash
python3 -m knowledge.pipeline ingest --sources devin --incremental
python3 -m knowledge.pipeline chunk
python3 -m knowledge.pipeline distill
python3 -m knowledge.pipeline merge
python3 -m knowledge.pipeline score
python3 -m knowledge.pipeline publish --min-confidence 80
```

The `--incremental` flag skips sessions already in `knowledge/staging/ingest_state.json`.

## Query

```bash
python3 -m knowledge.pipeline query "uart tx fifo"
python3 -m knowledge.pipeline stats
```

## Design

See `docs/superpowers/specs/2026-08-05-knowledge-distillation-pipeline-design.md` for full architecture.
