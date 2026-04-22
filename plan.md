# SmartSearch — Intelligent Desktop Search Engine

> **Goal**: Build a local-first, privacy-focused desktop search engine that transcends qmd's CLI limitations with a native GUI, real-time background indexing, context-aware ranking, and pluggable AI models — all running on your machine, under 20 MB.

---

## Reference: What qmd Does Today

[qmd](https://github.com/tobi/qmd) (21.3k ★) is a CLI search engine for markdown/docs that combines:
- **BM25 (FTS5)** keyword search
- **Vector semantic search** via local GGUF embeddings (`embeddinggemma-300M`)
- **LLM re-ranking** via `qwen3-reranker` 
- **Reciprocal Rank Fusion** to blend results
- **MCP server** for agent integration

**Its key limitations** (which SmartSearch solves):

| Problem | qmd | SmartSearch |
|---|---|---|
| Indexing | Manual `qmd update` + `qmd embed` | Real-time file watching, auto re-index |
| Context | Only the search query | Detects foreground app, boosts relevant file types |
| AI Engine | Bundles `node-llama-cpp` + GGUF models (~500MB+) | Connects to Ollama/LM Studio APIs (~0 bundled weight) |
| Chunking | Fixed 900-token regex split + 15% overlap | Semantic boundary detection via cosine similarity |
| Interface | Terminal only | Native desktop GUI with global hotkey |

---

## Technology Stack & Rationale

### Why Tauri 2.0 (Not Electron)

| Criterion | Tauri 2.0 | Electron |
|---|---|---|
| Binary size | **~5–10 MB** | ~120 MB |
| Memory (idle) | **~30–50 MB** | ~200 MB |
| Backend | **Rust** (native perf, `notify`, `rusqlite`) | Node.js |
| Security | Sandboxed WebView, opt-in APIs | Full Node.js access |
| macOS APIs | Direct FFI to `NSWorkspace` | Requires native modules |

**Verdict**: Tauri wins on every dimension that matters for a lightweight, always-on background utility.

### Full Stack

```
┌─────────────────────────────────────────────────────────────────┐
│                        SmartSearch                              │
├─────────────────────────────────────────────────────────────────┤
│  FRONTEND (React 19 + TypeScript + Vite)                       │
│  ├── Spotlight-style search bar (Cmd+Space global hotkey)      │
│  ├── Results panel with previews, scores, context chips        │
│  ├── Settings UI (collections, model config, file types)       │
│  └── Indexing status dashboard                                 │
├─────────────────────────────────────────────────────────────────┤
│  TAURI IPC BRIDGE (invoke/listen)                              │
├─────────────────────────────────────────────────────────────────┤
│  BACKEND (Rust)                                                │
│  ├── File Watcher ─── notify crate ─── tokio::mpsc ──┐        │
│  ├── Indexing Engine ◄───────────────────────────────┘         │
│  │   ├── Diff calculator (file hash comparison)                │
│  │   ├── Semantic chunker (sentence → embed → boundary)        │
│  │   └── Embedding generator (Ollama API client)               │
│  ├── Search Engine                                             │
│  │   ├── BM25 via SQLite FTS5                                  │
│  │   ├── Vector search via sqlite-vec                          │
│  │   ├── RRF fusion + active-context boosting                  │
│  │   └── LLM re-ranking (optional, via Ollama)                 │
│  ├── Active Context Detector                                   │
│  │   └── macOS: NSWorkspace → foreground app name              │
│  └── SQLite (rusqlite + FTS5 + sqlite-vec)                     │
└─────────────────────────────────────────────────────────────────┘
         │                              │
         ▼                              ▼
  ┌──────────────┐              ┌───────────────┐
  │ Ollama API   │              │ LM Studio API │
  │ :11434       │              │ :1234         │
  │ (embeddings) │              │ (embeddings)  │
  │ (reranking)  │              │ (reranking)   │
  └──────────────┘              └───────────────┘
```

### Rust Crates

| Crate | Purpose |
|---|---|
| `tauri` 2.x | App framework, IPC, window management, global shortcuts |
| `notify` 7.x | Cross-platform filesystem watching |
| `rusqlite` + `bundled` | Embedded SQLite with FTS5 support |
| `sqlite-vec` | Vector similarity search within SQLite |
| `reqwest` | HTTP client for Ollama/LM Studio APIs |
| `tokio` | Async runtime, channels, background tasks |
| `serde` / `serde_json` | JSON serialization for API communication |
| `blake3` | Fast file content hashing for change detection |
| `unicode-segmentation` | Sentence boundary detection |
| `objc2` (macOS) | FFI to NSWorkspace for foreground app detection |

### Frontend Libraries

| Library | Purpose |
|---|---|
| React 19 | Component framework |
| TypeScript | Type safety |
| Vite | Build tooling (Tauri plugin) |
| `@tauri-apps/api` | IPC invoke/listen bindings |
| Framer Motion | Micro-animations, transitions |
| CSS Modules | Scoped styling (no Tailwind) |

---

## Project Structure

```
smart-search_2/
├── src-tauri/                          # Rust backend
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/                   # Tauri 2 permission capabilities
│   │   └── default.json
│   ├── src/
│   │   ├── main.rs                     # Tauri setup, state init
│   │   ├── lib.rs                      # Module declarations
│   │   ├── commands/                   # Tauri IPC command handlers
│   │   │   ├── mod.rs
│   │   │   ├── search.rs              # search, vsearch, hybrid_search
│   │   │   ├── collections.rs         # add/remove/list collections
│   │   │   ├── settings.rs            # model config, preferences
│   │   │   └── status.rs              # indexing status, health
│   │   ├── indexer/                    # Indexing engine
│   │   │   ├── mod.rs
│   │   │   ├── watcher.rs             # File system watcher (notify)
│   │   │   ├── differ.rs              # Content diff & hash comparison
│   │   │   ├── chunker.rs             # Semantic boundary chunking
│   │   │   └── pipeline.rs            # Orchestrates: detect → chunk → embed → store
│   │   ├── search/                     # Search engine
│   │   │   ├── mod.rs
│   │   │   ├── bm25.rs                # FTS5 keyword search
│   │   │   ├── vector.rs              # sqlite-vec cosine similarity
│   │   │   ├── fusion.rs              # RRF fusion + active-context boost
│   │   │   └── reranker.rs            # LLM re-ranking via Ollama
│   │   ├── ai/                         # AI model abstraction
│   │   │   ├── mod.rs
│   │   │   ├── provider.rs            # Trait: EmbeddingProvider, RerankerProvider
│   │   │   ├── ollama.rs              # Ollama API client
│   │   │   └── lmstudio.rs            # LM Studio API client
│   │   ├── context/                    # Active context detection
│   │   │   ├── mod.rs
│   │   │   └── macos.rs               # NSWorkspace foreground app detection
│   │   └── db/                         # Database layer
│   │       ├── mod.rs
│   │       ├── schema.rs              # Table creation, migrations
│   │       └── queries.rs             # Prepared statements, helpers
│   └── icons/                          # App icons
├── src/                                # React frontend
│   ├── main.tsx                        # React entry point
│   ├── App.tsx                         # Root component
│   ├── components/
│   │   ├── SearchBar/
│   │   │   ├── SearchBar.tsx
│   │   │   └── SearchBar.module.css
│   │   ├── ResultsList/
│   │   │   ├── ResultsList.tsx
│   │   │   └── ResultsList.module.css
│   │   ├── ResultCard/
│   │   │   ├── ResultCard.tsx
│   │   │   └── ResultCard.module.css
│   │   ├── Settings/
│   │   │   ├── Settings.tsx
│   │   │   └── Settings.module.css
│   │   ├── StatusBar/
│   │   │   ├── StatusBar.tsx
│   │   │   └── StatusBar.module.css
│   │   └── CollectionManager/
│   │       ├── CollectionManager.tsx
│   │       └── CollectionManager.module.css
│   ├── hooks/
│   │   ├── useSearch.ts
│   │   ├── useIndexStatus.ts
│   │   └── useSettings.ts
│   ├── styles/
│   │   ├── globals.css                # Design tokens, reset, typography
│   │   └── theme.css                  # Dark/light mode variables
│   └── lib/
│       ├── tauri.ts                   # Typed IPC wrappers
│       └── types.ts                   # Shared TypeScript types
├── index.html
├── package.json
├── tsconfig.json
├── vite.config.ts
└── plan.md                            # This file
```

---

## Database Schema (SQLite)

```sql
-- ═══════════════════════════════════════════════════════════════
-- COLLECTIONS
-- ═══════════════════════════════════════════════════════════════
CREATE TABLE collections (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT    NOT NULL UNIQUE,
    path            TEXT    NOT NULL,
    glob_pattern    TEXT    DEFAULT '**/*.md',
    ignore_patterns TEXT,                          -- JSON array
    context         TEXT,                          -- user-provided description
    created_at      TEXT    DEFAULT (datetime('now')),
    updated_at      TEXT    DEFAULT (datetime('now'))
);

-- ═══════════════════════════════════════════════════════════════
-- DOCUMENTS
-- ═══════════════════════════════════════════════════════════════
CREATE TABLE documents (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    collection_id   INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    path            TEXT    NOT NULL,               -- relative to collection
    abs_path        TEXT    NOT NULL,               -- absolute filesystem path
    title           TEXT,
    content_hash    TEXT    NOT NULL,               -- blake3 hash for change detection
    file_size       INTEGER NOT NULL,
    file_type       TEXT    NOT NULL,               -- extension: md, py, ts, etc.
    last_indexed    TEXT    DEFAULT (datetime('now')),
    last_modified   TEXT,                           -- filesystem mtime
    is_active       INTEGER DEFAULT 1,
    UNIQUE(collection_id, path)
);

CREATE INDEX idx_documents_hash ON documents(content_hash);
CREATE INDEX idx_documents_type ON documents(file_type);

-- ═══════════════════════════════════════════════════════════════
-- CHUNKS (semantic boundary chunks)
-- ═══════════════════════════════════════════════════════════════
CREATE TABLE chunks (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id     INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    chunk_index     INTEGER NOT NULL,               -- order within document
    content         TEXT    NOT NULL,
    start_line      INTEGER,
    end_line        INTEGER,
    token_count     INTEGER,
    has_embedding   INTEGER DEFAULT 0
);

CREATE INDEX idx_chunks_document ON chunks(document_id);

-- ═══════════════════════════════════════════════════════════════
-- FTS5 (BM25 keyword search)
-- ═══════════════════════════════════════════════════════════════
CREATE VIRTUAL TABLE chunks_fts USING fts5(
    content,
    content='chunks',
    content_rowid='id',
    tokenize='porter unicode61'
);

-- Triggers to keep FTS in sync
CREATE TRIGGER chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts(rowid, content) VALUES (new.id, new.content);
END;

CREATE TRIGGER chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, content)
        VALUES('delete', old.id, old.content);
END;

CREATE TRIGGER chunks_au AFTER UPDATE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, content)
        VALUES('delete', old.id, old.content);
    INSERT INTO chunks_fts(rowid, content) VALUES (new.id, new.content);
END;

-- ═══════════════════════════════════════════════════════════════
-- VECTOR EMBEDDINGS (sqlite-vec)
-- ═══════════════════════════════════════════════════════════════
CREATE VIRTUAL TABLE chunk_vectors USING vec0(
    chunk_id  INTEGER PRIMARY KEY,
    embedding float[768]                  -- dimension depends on model
);

-- ═══════════════════════════════════════════════════════════════
-- SETTINGS
-- ═══════════════════════════════════════════════════════════════
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Default settings
INSERT INTO settings(key, value) VALUES
    ('ai_provider',         'ollama'),
    ('ollama_base_url',     'http://localhost:11434'),
    ('lmstudio_base_url',   'http://localhost:1234'),
    ('embed_model',         'nomic-embed-text'),
    ('rerank_model',        'qwen3-reranker'),
    ('embed_dimensions',    '768'),
    ('active_context',      'true'),
    ('watch_enabled',       'true'),
    ('theme',               'dark');
```

---

## Core Feature Designs

### Feature 1: Event-Driven Background Indexing

```
┌───────────────────────────────────────────────────────────────┐
│                    INDEXING PIPELINE                          │
└───────────────────────────────────────────────────────────────┘

  File System                     SmartSearch Backend
  ──────────                      ──────────────────
  User saves file.md
        │
        ▼
  ┌───────────────┐    notify     ┌─────────────────┐
  │ OS FSEvents   │──────────────▶│ Watcher Service │
  │ (kqueue/macOS)│               │ (notify crate)  │
  └───────────────┘               └────────┬────────┘
                                           │
                                    Debounce 500ms
                                    (coalesce rapid saves)
                                           │
                                           ▼
                                  ┌─────────────────┐
                                  │ Change Detector │
                                  │ blake3(content) │
                                  │ vs stored hash  │
                                  └────────┬────────┘
                                           │
                                    Hash changed?
                                    ├── No → skip
                                    └── Yes ▼
                                  ┌─────────────────┐
                                  │ Semantic Chunker│
                                  │ (only changed   │
                                  │  content)       │
                                  └────────┬────────┘
                                           │
                                           ▼
                                  ┌─────────────────┐
                                  │ Background Queue│
                                  │ (tokio::mpsc)   │
                                  └────────┬────────┘
                                           │
                                           ▼
                                  ┌─────────────────┐
                                  │ Embed via       │
                                  │ Ollama API      │
                                  │ POST /api/embed │
                                  └────────┬────────┘
                                           │
                                           ▼
                                  ┌─────────────────┐
                                  │ SQLite Update   │
                                  │ • chunks table  │
                                  │ • chunks_fts    │
                                  │ • chunk_vectors │
                                  └─────────────────┘
                                           │
                                  emit("index-updated")
                                           │
                                           ▼
                                  ┌─────────────────┐
                                  │ Frontend gets   │
                                  │ live status via │
                                  │ Tauri events    │
                                  └─────────────────┘
```

**Key implementation details:**
- **Debouncing**: Group FS events within 500ms windows to avoid re-indexing during rapid saves
- **Hash-based diffing**: Compare `blake3` hash of file content vs stored `content_hash` — skip if identical
- **Incremental re-chunking**: Delete old chunks for the document, re-chunk only the changed file
- **Background queue**: `tokio::mpsc` channel with bounded capacity (e.g., 100) to prevent backpressure
- **Progress events**: Emit Tauri events (`index-progress`, `index-complete`) for the frontend status bar

---

### Feature 2: Active Context Boosting

```rust
// Pseudocode for the active context boost logic

fn get_active_context_boost(foreground_app: &str, file_type: &str) -> f64 {
    let app = foreground_app.to_lowercase();
    
    let code_apps = ["visual studio code", "cursor", "zed", "neovim", "intellij"];
    let writing_apps = ["obsidian", "notion", "typora", "ia writer", "word"];
    let browser_apps = ["safari", "chrome", "firefox", "arc"];
    
    let code_types = ["rs", "py", "ts", "js", "go", "java", "c", "cpp", "rb"];
    let doc_types = ["md", "txt", "docx", "org", "tex"];
    let web_types = ["html", "css", "jsx", "tsx", "vue", "svelte"];
    
    if code_apps.iter().any(|a| app.contains(a))
       && code_types.contains(&file_type) {
        return 0.20; // 20% boost for code files when coding
    }
    if writing_apps.iter().any(|a| app.contains(a))
       && doc_types.contains(&file_type) {
        return 0.20; // 20% boost for docs when writing
    }
    if browser_apps.iter().any(|a| app.contains(a))
       && web_types.contains(&file_type) {
        return 0.15; // 15% boost for web files when browsing
    }
    
    0.0 // No boost
}
```

**macOS implementation** (via `objc2` / `NSWorkspace`):
```rust
// Simplified — actual implementation uses objc2 crate for safety
fn get_foreground_app() -> Option<String> {
    // NSWorkspace.shared.frontmostApplication?.localizedName
    unsafe {
        let workspace = msg_send![class!(NSWorkspace), sharedWorkspace];
        let app = msg_send![workspace, frontmostApplication];
        let name = msg_send![app, localizedName];
        nsstring_to_rust(name)
    }
}
```

**How it integrates into search**: During RRF fusion, after computing the base score, we add the context boost:
```
final_score = rrf_score * (1.0 + active_context_boost)
```

> [!IMPORTANT]
> This feature requires the user to grant **Accessibility permissions** in System Settings → Privacy & Security → Accessibility. The app should gracefully degrade (no boost, no error) if permissions are denied.

---

### Feature 3: BYOM (Bring Your Own Model) via API

```
┌────────────────────────────────────────────────────────────┐
│             AI PROVIDER ABSTRACTION LAYER                  │
├────────────────────────────────────────────────────────────┤
│                                                            │
│  trait EmbeddingProvider {                                  │
│      async fn embed(&self, texts: Vec<String>)             │
│          -> Result<Vec<Vec<f32>>>;                          │
│      fn dimensions(&self) -> usize;                        │
│      fn model_name(&self) -> &str;                         │
│  }                                                         │
│                                                            │
│  trait RerankerProvider {                                   │
│      async fn rerank(&self, query: &str,                   │
│          documents: Vec<String>) -> Result<Vec<f64>>;      │
│  }                                                         │
│                                                            │
├──────────────┬─────────────────┬──────────────────────────┤
│   Ollama     │   LM Studio     │   Future: OpenAI-compat  │
│              │                  │                          │
│ POST /api/   │ POST /v1/       │ POST /v1/embeddings      │
│   embed      │   embeddings    │                          │
│              │                  │                          │
│ Models:      │ Models:          │                          │
│ nomic-embed  │ Whatever user    │                          │
│ mxbai-embed  │ has loaded       │                          │
│ all-minilm   │                  │                          │
└──────────────┴─────────────────┴──────────────────────────┘
```

**Ollama API integration**:
```
Embedding request:
POST http://localhost:11434/api/embed
{
    "model": "nomic-embed-text",
    "input": ["chunk 1 text", "chunk 2 text", "chunk 3 text"]
}

Response:
{
    "embeddings": [[0.123, -0.456, ...], [0.789, ...], ...]
}
```

**Health check on startup**: The app pings `GET http://localhost:11434/api/tags` and shows a clear status indicator:
- 🟢 Ollama connected, model `nomic-embed-text` available
- 🟡 Ollama connected, required model not pulled (offer one-click pull)
- 🔴 Ollama not running (show instructions to install/start)

> [!NOTE]
> By not bundling any AI model, SmartSearch stays under **10 MB** compared to qmd's 500MB+ with bundled GGUF models. The user just needs Ollama (which they likely already have).

---

### Feature 4: Semantic Boundary Chunking

```
┌────────────────────────────────────────────────────────────────┐
│              SEMANTIC CHUNKING PIPELINE                       │
└────────────────────────────────────────────────────────────────┘

Input Document:
"The authentication system uses JWT tokens for session management.
 Tokens are signed with RS256 and expire after 24 hours.
 Users can refresh tokens using the /auth/refresh endpoint.
 
 The database layer uses PostgreSQL with connection pooling.
 We use pgbouncer for managing connection limits.
 The maximum pool size is configured via DATABASE_POOL_SIZE."

Step 1: Sentence Segmentation
├── S1: "The authentication system uses JWT tokens..."
├── S2: "Tokens are signed with RS256..."
├── S3: "Users can refresh tokens..."
├── S4: "The database layer uses PostgreSQL..."
├── S5: "We use pgbouncer..."
└── S6: "The maximum pool size is configured..."

Step 2: Embed Each Sentence (via Ollama, batched)
├── S1 → [0.82, 0.15, -0.33, ...]  (auth-related vector)
├── S2 → [0.79, 0.18, -0.31, ...]  (auth-related vector)
├── S3 → [0.75, 0.20, -0.28, ...]  (auth-related vector)
├── S4 → [0.12, 0.85, 0.44, ...]  (DB-related vector)  ← BIG SHIFT
├── S5 → [0.15, 0.82, 0.41, ...]  (DB-related vector)
└── S6 → [0.18, 0.80, 0.39, ...]  (DB-related vector)

Step 3: Cosine Similarity Between Adjacent Sentences
├── sim(S1, S2) = 0.97  (high → same topic)
├── sim(S2, S3) = 0.94  (high → same topic)
├── sim(S3, S4) = 0.23  (LOW → TOPIC BOUNDARY!) ← CUT HERE
├── sim(S4, S5) = 0.95  (high → same topic)
└── sim(S5, S6) = 0.93  (high → same topic)

Step 4: Apply Percentile Threshold
threshold = percentile(all_distances, 90) ≈ 0.75
Boundaries where similarity < (1 - threshold): [between S3 and S4]

Step 5: Form Chunks
├── Chunk 1: S1 + S2 + S3 → "The authentication system... refresh endpoint."
└── Chunk 2: S4 + S5 + S6 → "The database layer... DATABASE_POOL_SIZE."

Result: Two coherent, topically focused chunks instead of
        one chunk split mid-sentence at token 900.
```

**Fallback strategy**: If a resulting chunk exceeds 1500 tokens, fall back to splitting at the midpoint sentence. If a chunk is below 50 tokens, merge it with the adjacent chunk.

**Performance optimization**: Sentence embeddings for chunking use batch API calls — sending all sentences of a document in one request to Ollama rather than one-by-one.

---

## Hybrid Search Pipeline

```
┌─────────────────┐
│  User Query      │
│  + Active App    │
└────────┬────────┘
         │
         ▼
┌────────────────────┐
│ Query Embedding    │
│ (Ollama /api/embed)│
└────────┬───────────┘
         │
    ┌────┴────┐
    ▼         ▼
┌────────┐ ┌──────────┐
│ BM25   │ │ Vector   │
│ FTS5   │ │sqlite-vec│
│ top 30 │ │ top 30   │
└───┬────┘ └────┬─────┘
    │            │
    └──────┬─────┘
           ▼
  ┌─────────────────┐
  │ RRF Fusion      │
  │                 │
  │ score = Σ 1/    │
  │   (60 + rank)   │
  └────────┬────────┘
           │
           ▼
  ┌─────────────────┐
  │ Active Context  │
  │ Boost           │
  │                 │
  │ if VS Code →    │
  │   +20% to .rs   │
  │   +20% to .py   │
  └────────┬────────┘
           │
           ▼
  ┌─────────────────┐
  │ Optional:       │
  │ LLM Re-ranking  │
  │ (via Ollama)    │
  └────────┬────────┘
           │
           ▼
  ┌─────────────────┐
  │ Final Results   │
  │ top 10 returned │
  └─────────────────┘
```

---

## UI Design Concept

### Spotlight-Style Search Window

The primary interface is a floating, always-accessible search bar triggered by a global hotkey (default: `Cmd+Shift+Space` or configurable). It should feel like macOS Spotlight but smarter.

```
┌──────────────────────────────────────────────────────────────┐
│  🔍  Search your knowledge...                    ⚙️  ●──●──● │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │ 📄 authentication-flow.md           Score: 96%  [.md] │  │
│  │    notes/engineering                                   │  │
│  │    "The JWT authentication uses RS256 signing with     │  │
│  │     automatic token refresh via /auth/refresh..."      │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │ 🔧 auth-middleware.ts               Score: 89%  [.ts] │  │
│  │    work/api-server                    ✨ Context Boost │  │
│  │    "export function verifyToken(req, res, next) {      │  │
│  │     const token = req.headers.authorization..."        │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │ 📝 meeting-2025-04-10.md            Score: 72%  [.md] │  │
│  │    meetings/q2                                         │  │
│  │    "Discussed migrating auth from session cookies      │  │
│  │     to JWT-based authentication..."                    │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│  Indexed: 1,247 files  •  🟢 Ollama connected  •  ↻ 0 pend │
└──────────────────────────────────────────────────────────────┘
```

**Design system:**
- **Dark mode by default** with HSL-based color palette
- Glassmorphism: translucent background with blur (`backdrop-filter: blur(20px)`)
- Smooth result transitions with Framer Motion (`AnimatePresence`)
- File-type color coding: `.md` = blue, `.ts/.js` = yellow, `.py` = green, `.rs` = orange
- Score bar visualization (colored gradient)
- "Context Boost" chip shown when active-context boosting affects ranking

---

## Development Phases

### Phase 1: Foundation (Week 1–2)
> Scaffold the project, database, and basic search

- [ ] Initialize Tauri 2.0 + React + Vite project
- [ ] Set up Rust workspace with all crate dependencies
- [ ] Implement SQLite schema with `rusqlite` (tables, FTS5, triggers)
- [ ] Build collection management (add/remove/list directories)
- [ ] Implement basic file scanner (walk directory, store document metadata)
- [ ] Wire up Tauri IPC commands for collections

### Phase 2: AI Integration & Indexing (Week 3–4)
> Connect to Ollama, implement chunking and embedding

- [ ] Build Ollama API client (`/api/embed`, `/api/tags`, health check)
- [ ] Build LM Studio API client (`/v1/embeddings`)
- [ ] Implement `EmbeddingProvider` trait with Ollama/LMStudio backends
- [ ] Implement semantic boundary chunking algorithm
- [ ] Build indexing pipeline: scan → hash → chunk → embed → store
- [ ] Integrate `sqlite-vec` for vector storage
- [ ] Add settings UI for model selection and API endpoints

### Phase 3: Search Engine (Week 5–6)
> BM25 + vector + fusion + results UI

- [ ] Implement BM25 search via FTS5
- [ ] Implement vector similarity search via `sqlite-vec`
- [ ] Build RRF fusion algorithm
- [ ] Build the Spotlight-style search UI (SearchBar, ResultsList, ResultCard)
- [ ] Wire search results through Tauri IPC
- [ ] Add result previews with snippet highlighting
- [ ] Register global hotkey (`Cmd+Shift+Space`)

### Phase 4: Real-Time File Watching (Week 7)
> Event-driven background indexing

- [ ] Implement file watcher service using `notify` crate
- [ ] Add debounce logic (500ms window)
- [ ] Build `blake3` content hash comparison for change detection
- [ ] Implement incremental re-indexing (only changed files)
- [ ] Build background queue with `tokio::mpsc`
- [ ] Emit Tauri events for indexing progress
- [ ] Add StatusBar component showing live indexing status

### Phase 5: Active Context & Re-ranking (Week 8)
> Desktop-native intelligence features

- [ ] Implement macOS foreground app detection via `objc2`/`NSWorkspace`
- [ ] Build active-context boost scoring logic
- [ ] Integrate boost into RRF fusion step
- [ ] Implement LLM re-ranking via Ollama (optional, toggle in settings)
- [ ] Add "Context Boost" indicator in search results UI
- [ ] Handle Accessibility permission flow gracefully

### Phase 6: Polish & MCP (Week 9–10)
> Production quality, MCP server, edge cases

- [ ] Implement dark/light theme toggle
- [ ] Add micro-animations (result appear, hover, score bars)
- [ ] Implement glassmorphism for the search window
- [ ] Build MCP server for agent integration (optional parity with qmd)
- [ ] Handle edge cases (large files, binary files, permission errors)
- [ ] Performance optimization (batch embedding, query caching)
- [ ] Write integration tests
- [ ] Build first release binary

---

## Open Questions

> [!IMPORTANT]
> **1. Global Hotkey Conflict**: `Cmd+Space` is taken by macOS Spotlight. Should we default to `Cmd+Shift+Space`, `Option+Space`, or let the user configure it during onboarding?

> [!IMPORTANT]
> **2. Ollama as Hard Requirement**: Should the app strictly require Ollama/LM Studio to function, or should we have a **fallback mode** with BM25-only search (no embeddings) when no AI provider is available?

> [!WARNING]
> **3. File Type Support Scope**: qmd focuses on Markdown + code. For v1, should SmartSearch support:
> - A) Markdown only (simplest)
> - B) Markdown + common code files (.ts, .py, .rs, .go, .js)
> - C) All text-based files (including .txt, .json, .yaml, .toml)
> - D) Binary formats like .pdf, .docx (requires additional parsing libraries)

> [!NOTE]
> **4. Code Chunking Strategy**: For code files specifically, should we use semantic chunking (same as text), or implement AST-based chunking (like qmd's `tree-sitter` approach)? AST chunking respects function/class boundaries but requires bundling grammar files per language.

---

## Verification Plan

### Automated Tests
```bash
# Rust unit tests
cargo test --workspace

# Semantic chunker tests (verify chunk boundaries)
cargo test -p smart-search -- chunker::tests

# Search quality tests (precision/recall on test corpus)
cargo test -p smart-search -- search::integration_tests

# Frontend component tests
npm test
```

### Manual Verification
1. **File watching**: Edit a markdown file in VS Code → verify search results update within 2s
2. **Search quality**: Compare SmartSearch results vs qmd results on the same corpus
3. **Active context**: Trigger search from VS Code vs Obsidian → verify rank differences
4. **Ollama failover**: Kill Ollama → verify graceful degradation to BM25-only
5. **Performance**: Index 1000+ markdown files → verify total time under 30 seconds
6. **Binary size**: Verify final `.app` bundle is under 20 MB (excluding Ollama)
