import os
import json
import pathlib

BASE_DIR = pathlib.Path(__file__).parent.resolve()
RAW_DIR = BASE_DIR / "raw"
STAGING_DIR = BASE_DIR / "staging"
CHUNKS_DIR = STAGING_DIR / "chunks"
DB_PATH = STAGING_DIR / "memories.db"

OPENCODE_TRANSCRIPT_DIR = pathlib.Path.home() / ".claude" / "transcripts"
COPILOT_TRANSCRIPT_GLOB = [
    pathlib.Path.home() / ".vscode-server" / "data" / "User" / "workspaceStorage" / "1bbb4e8862ac9b4ad52fa265e12b5dd6" / "GitHub.copilot-chat" / "transcripts" / "*.jsonl",
]
DEVIN_SESSION_GLOB = "/tmp/session_*.json"
DOCS_DIRS = [
    pathlib.Path(__file__).parent.parent / "specs",
]
DOCS_FILES = [
    pathlib.Path(__file__).parent.parent / "AGENTS.md",
    pathlib.Path(__file__).parent.parent / "SESSION_CONTEXT.md",
]

OPENCODE_MEM_API = "http://localhost:4747/api/memories"
OPENCODE_MEM_QUERY = "http://localhost:4747/api/memories/search"

MINIMAX_BASE_URL = "https://api.minimax.chat/v1"
MINIMAX_MODEL = "MiniMax-M2.7"

F0G_BASE_URL = "https://f0g.dev:8142/v1"
F0G_MODEL = "gpt-5.5"

def get_llm_config():
    minimax_key = os.environ.get("MINIMAX_API_KEY")
    if minimax_key:
        return {"base_url": MINIMAX_BASE_URL, "model": MINIMAX_MODEL, "api_key": minimax_key}
    f0g_key = os.environ.get("F0G_API_KEY") or os.environ.get("OPENAI_API_KEY")
    return {"base_url": F0G_BASE_URL, "model": F0G_MODEL, "api_key": f0g_key}

DEFAULT_CHUNK_MAX_TOKENS = 2000
DEFAULT_BATCH_SIZE = 10
DEFAULT_SIMILARITY_THRESHOLD = 0.7
DEFAULT_MIN_CONFIDENCE = 80
DEFAULT_RETRIES = 3
