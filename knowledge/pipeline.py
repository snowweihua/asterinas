import argparse
import sys

from . import config
from .schema import init_db, get_db_connection
from . import opencode_parser
from . import copilot_parser
from . import devin_parser
from . import docs_parser
from . import chunk
from . import distill
from . import merge
from . import score
from . import publish


def cmd_ingest(args):
    sources = args.sources.split(",") if args.sources else ["opencode", "copilot", "devin", "docs"]
    total = 0
    if "opencode" in sources:
        total += opencode_parser.ingest()
    if "copilot" in sources:
        total += copilot_parser.ingest()
    if "devin" in sources:
        total += devin_parser.ingest()
    if "docs" in sources:
        total += docs_parser.ingest()
    print(f"[pipeline] Ingest complete: {total} sessions processed")
    return total


def cmd_chunk(args):
    sources = args.sources.split(",") if args.sources else None
    return chunk.chunk_all(sources)


def cmd_distill(args):
    return distill.distill_all(batch_size=args.batch_size)


def cmd_merge(args):
    return merge.merge_all(similarity_threshold=args.similarity_threshold)


def cmd_score(args):
    return score.score_all()


def cmd_publish(args):
    return publish.publish_all(min_confidence=args.min_confidence)


def cmd_full(args):
    print("[pipeline] Starting full pipeline...", flush=True)
    cmd_ingest(args)
    cmd_chunk(args)
    cmd_distill(args)
    cmd_merge(args)
    cmd_score(args)
    cmd_publish(args)
    print("[pipeline] Full pipeline complete", flush=True)


def cmd_query(args):
    init_db()
    from .schema import get_memories_fts
    results = get_memories_fts(args.query, limit=args.limit)
    if not results:
        print("No results found.")
        return
    for mem in results:
        print(f"\n[{mem.id}] {mem.title}")
        print(f"  Type: {mem.type} | Area: {mem.area or 'general'} | Confidence: {mem.confidence}")
        print(f"  Summary: {mem.summary}")
        print(f"  Details: {mem.details[:200]}")
        print(f"  Tags: {', '.join(mem.tags)}")
        print(f"  Sources: {', '.join(mem.sources)}")


def cmd_stats(args):
    init_db()
    conn = get_db_connection()
    cur = conn.cursor()

    cur.execute("SELECT COUNT(*) FROM memories")
    total_memories = cur.fetchone()[0]

    cur.execute("SELECT COUNT(*) FROM chunks")
    total_chunks = cur.fetchone()[0]

    cur.execute("SELECT source, COUNT(*) FROM source_stats GROUP BY source")
    stats = cur.fetchall()

    print(f"Memories: {total_memories}")
    print(f"Chunks: {total_chunks}")
    print("\nBy source:")
    for row in stats:
        print(f"  {row[0]}: sessions={row[1]}")

    conn.close()


def main():
    parser = argparse.ArgumentParser(prog="knowledge.pipeline")
    sub = parser.add_subparsers(dest="command")

    p_ingest = sub.add_parser("ingest", help="Ingest raw sources")
    p_ingest.add_argument("--sources", default="opencode,copilot,devin,docs",
                          help="Comma-separated sources (default: all)")

    p_chunk = sub.add_parser("chunk", help="Chunk normalized sessions")
    p_chunk.add_argument("--sources", help="Comma-separated sources")
    p_chunk.add_argument("--max-tokens", type=int, default=config.DEFAULT_CHUNK_MAX_TOKENS)

    p_distill = sub.add_parser("distill", help="Distill chunks into memories via LLM")
    p_distill.add_argument("--batch-size", type=int, default=config.DEFAULT_BATCH_SIZE)

    p_merge = sub.add_parser("merge", help="Merge duplicate memories")
    p_merge.add_argument("--similarity-threshold", type=float,
                         default=config.DEFAULT_SIMILARITY_THRESHOLD)

    p_score = sub.add_parser("score", help="Score memories (confidence/importance)")

    p_publish = sub.add_parser("publish", help="Publish memories to opencode-mem")
    p_publish.add_argument("--min-confidence", type=int, default=config.DEFAULT_MIN_CONFIDENCE)

    p_full = sub.add_parser("full", help="Run full pipeline (ingest->chunk->distill->merge->score->publish)")

    p_query = sub.add_parser("query", help="Query memories via FTS")
    p_query.add_argument("query", help="Search query")
    p_query.add_argument("--limit", type=int, default=10)

    p_stats = sub.add_parser("stats", help="Show pipeline statistics")

    args = parser.parse_args()

    if args.command == "ingest":
        cmd_ingest(args)
    elif args.command == "chunk":
        cmd_chunk(args)
    elif args.command == "distill":
        cmd_distill(args)
    elif args.command == "merge":
        cmd_merge(args)
    elif args.command == "score":
        cmd_score(args)
    elif args.command == "publish":
        cmd_publish(args)
    elif args.command == "full":
        cmd_full(args)
    elif args.command == "query":
        cmd_query(args)
    elif args.command == "stats":
        cmd_stats(args)
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
