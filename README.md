# opencode-mem Infrastructure

Persistent, semantic memory for AI coding agents via [opencode-mem](https://github.com/Stranmor/opencode-mem) – a Rust MCP server backed by PostgreSQL + pgvector.

This setup gives your AI agents long-term memory across sessions: store observations, search semantically, auto-summarize work, and recall past decisions.

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
- **BGE-M3 embeddings** – runs locally via ONNX (CPU), no GPU needed
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

### 2.3 Clone and build

```bash
git clone https://github.com/Stranmor/opencode-mem.git
cd opencode-mem
cargo build --release
```

The binary will be at `target/release/opencode-mem`. Copy it to your PATH:

```bash
cp target/release/opencode-mem ~/.local/bin/opencode-mem-cli
```

### ⚠️ Known build error: function signature mismatch

You may encounter this error when building:

```
error[E0061]: this function takes 5 arguments but 4 arguments were supplied
   --> crates/llm/src/observation.rs:171:13
    |
171 |             build_compression_prompt(&input.tool, &filtered_title, &filtered_output, candidates);
    |             ^^^^^^^^^^^^^^^^^^^^^^^^------------------------------------------------------------ argument #5 of type `&str` is missing
```

**Cause:** The `build_compression_prompt` function expects a 5th argument `current_session_id: &str`, but the caller at `observation.rs:171` only passes 4 arguments.

**Fix:** Edit `crates/llm/src/observation.rs` and change line 170-171 from:

```rust
        let prompt =
            build_compression_prompt(&input.tool, &filtered_title, &filtered_output, candidates);
```

To:

```rust
        let prompt =
            build_compression_prompt(
                &input.tool,
                &filtered_title,
                &filtered_output,
                candidates,
                input.session_id.as_ref(),
            );
```

Then rebuild:

```bash
cargo build --release
```

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

BGE-M3 (~1.3 GB) downloads on first use. Ensure your machine has enough disk space and a working internet connection. The model is cached after first download.

### Rate limited by OpenRouter free tier

Swap to OpenCode Zen or a paid model like `deepseek/deepseek-chat` ($0.14/$0.28 per 1M tokens).
