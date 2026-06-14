# opencode-mem Infrastructure

**Give your AI coding agent persistent memory across sessions.** Stop losing context every time you close the terminal. This is a complete, production-ready infrastructure for [opencode-mem](https://github.com/Stranmor/opencode-mem) — a Rust MCP server that gives OpenCode, Claude Code, Codex CLI, and any MCP-compatible agent durable, searchable memory backed by PostgreSQL + pgvector.

Built for developers who want:
- **Zero cloud dependency** — embeddings run locally via ONNX (BGE-M3, multilingual), or optionally via Cohere API
- **Free LLM compression** — dedup and summarization via OpenRouter free tier (or OpenCode Zen)
- **Docker-first** — single `docker compose up` to start
- **Privacy** — your data never leaves your machine (embedding) or is routed through providers with zero-retention policies (compression)

**Keywords:** opencode memory, MCP server, persistent memory, AI agent memory, pgvector, semantic search, PostgreSQL vector database, Rust MCP, coding agent context, OpenCode MCP server, LLM memory, cross-session context, AI coding assistant memory, opencode-mem setup, Claude Code memory, Codex CLI memory

---

## Why This Exists

AI coding agents forget everything between sessions. Every time you start a new terminal, your agent has no memory of:
- What you worked on yesterday
- Decisions you made about architecture
- Gotchas you discovered
- Project conventions you established

This infrastructure solves that. It gives your agent a durable, searchable memory — facts are stored by *meaning* (vector embeddings), not just keywords, and can be recalled across any session, any project, any CLI.

---

## Architecture

```
                    ┌─────────────────────┐
                    │  opencode-mem-cli    │  MCP server (stdio)
                    │  (Rust binary)       │
                    └──────┬──────────────┘
                           │ DATABASE_URL
                    ┌──────▼──────────────┐
                    │  PostgreSQL +        │  Docker container
                    │  pgvector            │  port 4004
                    └─────────────────────┘
```

- **PostgreSQL 16** with `pgvector` extension – stores observations, sessions, embeddings
- **opencode-mem** – MCP server that connects to PG, provides 18 memory tools
- **BGE-M3 embeddings** – runs locally via ONNX (CPU, requires AVX2), or via Cohere API (no CPU requirement, free trial)
- **LLM compression** – uses a remote API (OpenRouter/OpenCode Zen) for dedup + summarization

---

## Requirements

- Docker & Docker Compose
- Rust (for building opencode-mem)
- An OpenAI-compatible API key (OpenRouter or OpenCode Zen)

---

## Step 1: PostgreSQL with pgvector

### 1.1 Create `docker-compose.yml`

```yaml
services:
  postgres:
    image: pgvector/pgvector:pg16
    container_name: opencode-mem-pg
    restart: unless-stopped
    ports:
      - "${POSTGRES_PORT:-4004}:5432"
    environment:
      POSTGRES_USER: ${POSTGRES_USER:-opencode_mem}
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}
      POSTGRES_DB: ${POSTGRES_DB:-opencode_mem}
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ${POSTGRES_USER:-opencode_mem} -d ${POSTGRES_DB:-opencode_mem}"]
      interval: 5s
      timeout: 5s
      retries: 5

volumes:
  pgdata:
```

### 1.2 Create `.env`

```bash
POSTGRES_USER=opencode_mem
POSTGRES_PASSWORD=<your-strong-password>
POSTGRES_DB=opencode_mem
POSTGRES_PORT=4004
```

### 1.3 Start the container

```bash
docker compose up -d
```

Verify it's healthy:

```bash
docker inspect --format='{{.State.Health.Status}}' opencode-mem-pg
# Should print: healthy
```

---

## Step 2: Build opencode-mem

### 2.1 Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup default stable
```

### 2.2 Install build dependencies (Linux)

```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev
```

### 2.3 Build from this repo (includes fixes + API embedding support)

```bash
cd source
cargo build --release
```

The binary will be at `source/target/release/opencode-mem`. Copy it to your PATH:

```bash
cp target/release/opencode-mem ~/.local/bin/opencode-mem-cli
```

### ⚠️ Note: function signature fix already applied

The `source/` directory in this repo already includes the fix for `build_compression_prompt` (5th argument `session_id`). If building from the upstream repo, see the troubleshooting section below.

---

## Step 3: Configure MCP (OpenCode)

### 3.1 Register the MCP server

Add to `~/.config/opencode/opencode.jsonc`:

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "opencode-mem": {
      "type": "local",
      "command": ["/path/to/opencode-mem-cli", "mcp"],
      "enabled": true,
      "environment": {
        "DATABASE_URL": "postgres://opencode_mem:<password>@localhost:4004/opencode_mem",
        "OPENCODE_MEM_API_URL": "https://openrouter.ai/api/v1",
        "OPENCODE_MEM_API_KEY": "sk-or-...your-openrouter-key...",
        "OPENCODE_MEM_MODEL": "qwen/qwen3-coder:free"
      }
    }
  }
}
```

### 3.2 Verify the server is connected

```bash
opencode mcp list
```

Expected output:

```
┌  MCP Servers
│
●  ✓ opencode-mem connected
│      /home/user/.local/bin/opencode-mem-cli mcp
│
└  1 server(s)
```

---

## Step 4: LLM Model Configuration

The compression pipeline needs an LLM for dedup, summarization, and metadata extraction. Two free options:

### Option A: OpenRouter (Primary – no credit card needed)

- Sign up at [openrouter.ai](https://openrouter.ai)
- Create an API key at [openrouter.ai/keys](https://openrouter.ai/keys)
- Free tier: 20 req/min, ~200 req/day — ample for compression

```env
OPENCODE_MEM_API_URL=https://openrouter.ai/api/v1
OPENCODE_MEM_API_KEY=sk-or-v1-...
OPENCODE_MEM_MODEL=qwen/qwen3-coder:free
```

### Option B: OpenCode Zen (Fallback)

- Sign in at [opencode.ai/auth](https://opencode.ai/zen)
- Create an API key
- Needs billing info even for free models

```env
OPENCODE_MEM_API_URL=https://opencode.ai/zen/v1/chat/completions
OPENCODE_MEM_API_KEY=sk-...
OPENCODE_MEM_MODEL=deepseek-v4-flash-free
```

### Rate limit handling

If you hit rate limits, the server retries 3x (`OPENCODE_MEM_MAX_RETRY=3`) then moves to DLQ. No data loss, no crash. Swap the model or provider in the config to resume compression.

---

## How It Works

### Available MCP Tools (18 total)

| Tool | Purpose |
|------|---------|
| `search` / `memory_search` | Semantic + hybrid search across observations |
| `save_memory` / `memory_store` | Store an observation |
| `timeline` | Chronological context within a time range |
| `memory_get` | Get a single observation by ID |
| `memory_recent` | Most recent observations |
| `memory_hybrid_search` | FTS + keyword search |
| `memory_semantic_search` | Pure vector search |
| `memory_graph` | Association graph (connected, path, subgraph, infer, suggest) |
| `knowledge_search` | Search global knowledge base |
| `knowledge_save` | Save a knowledge entry (skill, pattern, gotcha) |
| `knowledge_get/list/delete` | Knowledge CRUD |
| `infinite_expand` | Expand summary to child events |
| `infinite_time_range` | Events within a time range |
| `infinite_drill_hour/minute` | Drill down summaries |
| `__IMPORTANT` | 3-Layer workflow docs |

### Usage Pattern

**At session start** – AI searches for relevant context:

```
search(query="map tile service decisions")
```

**At session end** – AI saves key facts:

```
save_memory(
  tool="opencode",
  title="Deployed map tiles",
  output="Iran mbtiles deployed to production at mahanfile.com/map, maxzoom 14"
)
```

These calls are handled automatically if you add instructions to your `.opencode/instructions` file.

---

## Configuration Reference

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DATABASE_URL` | Yes | — | PostgreSQL connection string |
| `OPENCODE_MEM_API_KEY` | Yes | — | API key for the LLM provider |
| `OPENCODE_MEM_API_URL` | No | `https://api.openai.com` | OpenAI-compatible base URL |
| `OPENCODE_MEM_MODEL` | No | — | Model for compression |
| `OPENCODE_MEM_DISABLE_EMBEDDINGS` | No | `false` | Disable vector embeddings |
| `OPENCODE_MEM_EMBEDDINGS_API_KEY` | No | — | API key for remote embeddings (Cohere/OpenAI-compatible). When set, overrides local ONNX |
| `OPENCODE_MEM_EMBEDDINGS_API_URL` | No | `https://api.cohere.com/v1/embed` | URL for remote embedding API (Cohere-compatible format) |
| `OPENCODE_MEM_MAX_RETRY` | No | `3` | LLM compression retries |
| `OPENCODE_MEM_VISIBILITY_TIMEOUT` | No | `300s` | Queue visibility timeout |
| `OPENCODE_MEM_DEDUP_THRESHOLD` | No | `0.85` | Cosine similarity for dedup |
| `OPENCODE_MEM_FILTER_PATTERNS` | No | — | Noise filter regex patterns |

---

## Troubleshooting

### MCP server wont start

**Symptom:** `Error: DATABASE_URL environment variable must be set`

**Fix:** Ensure `DATABASE_URL` is set either in opencode.jsonc's `environment` block or as a shell environment variable before starting opencode.

### Server shows "disconnected" in `opencode mcp list`

Check the PostgreSQL container:

```bash
docker logs opencode-mem-pg
docker inspect --format='{{.State.Health.Status}}' opencode-mem-pg
```

### Embeddings fail to load

**If using local ONNX (BGE-M3):** The model (~1.3 GB) downloads on first use. Ensure enough disk space and internet. BGE-M3 requires AVX2 CPU support — older CPUs (e.g., Sandy Bridge i7-2630QM) will crash with `Illegal instruction`.

**Solution:** Use the **Cohere API** embedding provider instead. Set `OPENCODE_MEM_EMBEDDINGS_API_KEY` to a free Cohere trial key (no credit card needed, sign up at [cohere.com](https://cohere.com)):

```env
OPENCODE_MEM_EMBEDDINGS_API_KEY=your-cohere-trial-key
OPENCODE_MEM_EMBEDDINGS_API_URL=https://api.cohere.com/v1/embed  # default
```

The API provider uses `embed-multilingual-v3.0` (1024-dim, matching pgvector schema).

### Rate limited by OpenRouter free tier

Swap to OpenCode Zen or a paid model like `deepseek/deepseek-chat` ($0.14/$0.28 per 1M tokens).

---

## FAQ

### Why does my AI agent forget everything between sessions?

This is the fundamental limitation of all AI coding tools. Each session is isolated — the agent has no built-in way to persist what it learned. opencode-mem solves this by storing observations in PostgreSQL and retrieving them via semantic search across sessions.

### Is this only for OpenCode?

No. opencode-mem is an MCP server — it works with any MCP-compatible client: **OpenCode**, **Claude Code**, **Codex CLI**, and any other tool that supports the Model Context Protocol. The setup guide above shows OpenCode config, but the same binary works for all of them.

### Does this work without internet?

**Partially.** The BGE-M3 embeddings run 100% locally via ONNX (no internet needed). The LLM compression step (dedup, summarization, metadata extraction) needs an API endpoint — either remote (OpenRouter, OpenCode Zen) or local (Ollama). You can disable compression entirely and still have full search and storage.

### What's the cheapest way to run this?

**Free.** The primary config uses OpenRouter's free tier (`qwen/qwen3-coder:free`) — no credit card required. Embeddings run locally at no cost. The only paid option would be if you exceed OpenRouter's free tier limits (~200 req/day), in which case you'd pay pennies for a fallback model.

### How is my data handled?

- **Embeddings:** Your data never leaves your machine (local ONNX runtime)
- **LLM compression:** Routed through OpenRouter or OpenCode Zen with zero-retention policies
- **Database:** Your own PostgreSQL, fully under your control

### Can I use a local LLM instead of a cloud API?

Yes. Point `OPENCODE_MEM_API_URL` at any OpenAI-compatible local server (Ollama, llama.cpp, LM Studio):

```env
OPENCODE_MEM_API_URL=http://localhost:11434/v1
OPENCODE_MEM_API_KEY=ollama  # Ollama accepts any key
OPENCODE_MEM_MODEL=qwen2.5:3b
```

### What hardware do I need?

**With Cohere API embeddings (recommended):**
- **CPU:** Any x86_64
- **RAM:** 2+ GB
- **Disk:** ~100 MB (binary + database)

**With local ONNX (BGE-M3):**
- **CPU:** x86_64 with AVX2 support (check with `lscpu | grep avx2`). Older CPUs (Sandy Bridge, Ivy Bridge) will crash.
- **RAM:** 4+ GB for PostgreSQL + opencode-mem
- **Disk:** ~2 GB for the binary + embedding model + database
- **GPU:** Not required (but supported if available)

### What is the function signature error when building?

See [Step 2.3](#-known-build-error-function-signature-mismatch) above. The upstream repo has a minor mismatch between a function definition and its caller. The fix is a one-line addition — edit the file, add the 5th argument, and rebuild.

### What models support Persian / Arabic / non-Latin text?

BGE-M3 (the default embedding model) supports 100+ languages including Persian, Arabic, Chinese, Japanese, and Korean. For the compression LLM, Qwen3 Coder and DeepSeek have strong multilingual support.

### Can I use this across multiple projects?

Yes. opencode-mem supports a global store (cross-project facts, preferences) accessible from any project directory. You can scope searches to current or all projects.

### What happens if PostgreSQL goes down?

opencode-mem has a built-in circuit breaker. It gracefully degrades when PostgreSQL is unavailable and automatically recovers on reconnect. No data loss, no crash.

### How do I back up my memory data?

Your PostgreSQL data volume persists across container restarts. To back up:

```bash
docker exec opencode-mem-pg pg_dump -U opencode_mem opencode_mem > backup.sql
```

### How is this different from .opencode/instructions or AGENTS.md?

`.opencode/instructions` and `AGENTS.md` are static text files loaded into every session's prompt. They're good for project-level conventions but can't scale to session-by-session observations. opencode-mem is a dynamic database — it stores facts from every session, retrieves them by semantic meaning, and auto-compresses them into summaries.
