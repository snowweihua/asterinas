import sqlite3
import json
import re
import pathlib
from dataclasses import dataclass, field, asdict
from datetime import datetime
from typing import Optional

from . import config


@dataclass
class Memory:
    id: str
    title: str
    type: str
    project: str = "asterinas"
    area: Optional[str] = None
    summary: str = ""
    details: str = ""
    confidence: int = 50
    importance: int = 5
    verified: bool = False
    tags: list = field(default_factory=list)
    sources: list = field(default_factory=list)
    chunk_ref: Optional[str] = None
    created: str = ""
    merged_from: list = field(default_factory=list)

    def to_dict(self):
        d = asdict(self)
        d["tags"] = json.dumps(self.tags) if isinstance(self.tags, list) else self.tags
        d["sources"] = json.dumps(self.sources) if isinstance(self.sources, list) else self.sources
        d["verified"] = 1 if self.verified else 0
        d["merged_from"] = json.dumps(self.merged_from)
        return d

    @classmethod
    def from_dict(cls, d):
        tags = d.get("tags", "[]")
        sources = d.get("sources", "[]")
        merged_from = d.get("merged_from", "[]")
        if isinstance(tags, str):
            tags = json.loads(tags)
        if isinstance(sources, str):
            sources = json.loads(sources)
        if isinstance(merged_from, str):
            merged_from = json.loads(merged_from)
        return cls(
            id=d["id"],
            title=d["title"],
            type=d["type"],
            project=d.get("project", "asterinas"),
            area=d.get("area"),
            summary=d.get("summary", ""),
            details=d.get("details", ""),
            confidence=int(d.get("confidence", 50)),
            importance=int(d.get("importance", 5)),
            verified=bool(d.get("verified", 0)),
            tags=tags,
            sources=sources,
            chunk_ref=d.get("chunk_ref"),
            created=d.get("created", ""),
            merged_from=merged_from,
        )


@dataclass
class Chunk:
    chunk_id: str
    source: str
    session_id: str
    topic_hint: Optional[str]
    content: str
    token_count: int
    created: str = ""


def get_db_connection():
    config.STAGING_DIR.mkdir(parents=True, exist_ok=True)
    conn = sqlite3.connect(str(config.DB_PATH))
    conn.row_factory = sqlite3.Row
    return conn


def init_db():
    conn = get_db_connection()
    cur = conn.cursor()

    cur.execute("""
        CREATE TABLE IF NOT EXISTS memories (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            type TEXT NOT NULL,
            project TEXT DEFAULT 'asterinas',
            area TEXT,
            summary TEXT NOT NULL,
            details TEXT DEFAULT '',
            confidence INTEGER DEFAULT 50,
            importance INTEGER DEFAULT 5,
            verified INTEGER DEFAULT 0,
            tags TEXT DEFAULT '[]',
            sources TEXT DEFAULT '[]',
            chunk_ref TEXT,
            created TEXT DEFAULT '',
            merged_from TEXT DEFAULT '[]',
            UNIQUE(title, sources)
        )
    """)

    cur.execute("""
        CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
            title, summary, details, tags,
            content='memories',
            content_rowid='rowid'
        )
    """)

    cur.execute("""
        CREATE TABLE IF NOT EXISTS chunks (
            chunk_id TEXT PRIMARY KEY,
            source TEXT,
            session_id TEXT,
            topic_hint TEXT,
            content TEXT,
            token_count INTEGER,
            created TEXT DEFAULT ''
        )
    """)

    cur.execute("""
        CREATE TABLE IF NOT EXISTS source_stats (
            source TEXT PRIMARY KEY,
            sessions_processed INTEGER DEFAULT 0,
            chunks_created INTEGER DEFAULT 0,
            memories_distilled INTEGER DEFAULT 0,
            last_run TEXT
        )
    """)

    conn.commit()
    conn.close()


def insert_memory(memory: Memory) -> bool:
    conn = get_db_connection()
    cur = conn.cursor()
    d = memory.to_dict()
    try:
        cur.execute("""
            INSERT OR IGNORE INTO memories
            (id, title, type, project, area, summary, details, confidence,
             importance, verified, tags, sources, chunk_ref, created, merged_from)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """, (
            d["id"], d["title"], d["type"], d["project"], d["area"],
            d["summary"], d["details"], d["confidence"], d["importance"],
            d["verified"], d["tags"], d["sources"], d["chunk_ref"],
            d["created"], d["merged_from"]
        ))
        conn.commit()
        inserted = cur.rowcount > 0
        if inserted:
            cur.execute("""
                INSERT INTO memories_fts(rowid, title, summary, details, tags)
                SELECT rowid, title, summary, details, tags FROM memories WHERE id = ?
            """, (d["id"],))
            conn.commit()
        conn.close()
        return inserted
    except Exception as e:
        conn.close()
        raise e


def insert_chunk(chunk: Chunk) -> bool:
    conn = get_db_connection()
    cur = conn.cursor()
    try:
        cur.execute("""
            INSERT OR IGNORE INTO chunks
            (chunk_id, source, session_id, topic_hint, content, token_count, created)
            VALUES (?, ?, ?, ?, ?, ?, ?)
        """, (
            chunk.chunk_id, chunk.source, chunk.session_id,
            chunk.topic_hint, chunk.content, chunk.token_count, chunk.created
        ))
        conn.commit()
        conn.close()
        return cur.rowcount > 0
    except Exception as e:
        conn.close()
        raise e


def update_memory(memory: Memory):
    conn = get_db_connection()
    cur = conn.cursor()
    d = memory.to_dict()
    cur.execute("""
        UPDATE memories SET
            title=?, type=?, project=?, area=?, summary=?, details=?,
            confidence=?, importance=?, verified=?, tags=?, sources=?,
            chunk_ref=?, created=?, merged_from=?
        WHERE id=?
    """, (
        d["title"], d["type"], d["project"], d["area"], d["summary"], d["details"],
        d["confidence"], d["importance"], d["verified"], d["tags"], d["sources"],
        d["chunk_ref"], d["created"], d["merged_from"], d["id"]
    ))
    conn.commit()
    conn.close()


def get_all_memories():
    conn = get_db_connection()
    cur = conn.cursor()
    cur.execute("SELECT * FROM memories")
    rows = cur.fetchall()
    conn.close()
    return [Memory.from_dict(dict(r)) for r in rows]


def get_memories_fts(query: str, limit: int = 10):
    safe = re.sub(r"[^\w\s]", " ", query)
    safe = " ".join(safe.split())
    if not safe:
        return []
    conn = get_db_connection()
    cur = conn.cursor()
    cur.execute("""
        SELECT m.* FROM memories m
        JOIN memories_fts fts ON m.rowid = fts.rowid
        WHERE memories_fts MATCH ?
        ORDER BY rank
        LIMIT ?
    """, (f'"{safe}"', limit))
    rows = cur.fetchall()
    conn.close()
    return [Memory.from_dict(dict(r)) for r in rows]


def update_source_stats(source: str, sessions: int = 0, chunks: int = 0, memories: int = 0):
    conn = get_db_connection()
    cur = conn.cursor()
    cur.execute("""
        INSERT INTO source_stats (source, sessions_processed, chunks_created, memories_distilled, last_run)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(source) DO UPDATE SET
            sessions_processed = sessions_processed + excluded.sessions_processed,
            chunks_created = chunks_created + excluded.chunks_created,
            memories_distilled = memories_distilled + excluded.memories_distilled,
            last_run = excluded.last_run
    """, (source, sessions, chunks, memories, datetime.now().isoformat()))
    conn.commit()
    conn.close()
