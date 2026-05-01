# SmartSearch — Architectural Critique & Suggestions

> **Reviewer perspective**: Senior Software Architect & Security Engineer.  
> **Scope**: Full Rust backend (`src-tauri/src/`), React frontend (`src/`), project structure, and product viability.  
> **Date**: 2026-05-01

---

## Verdict: Is This Idea Stupid?

**No. This is a genuinely good idea.** But the execution has critical gaps that will prevent it from gaining traction unless addressed. Here's why:

### Why It Works
1. **Real pain point**: Spotlight/Alfred can't search *inside* your code and notes semantically. Power users (devs, researchers, PKM enthusiasts) genuinely want this.
2. **Correct architecture bet**: Tauri + Rust for a lightweight, always-on background utility is the right call over Electron. The ~10MB binary vs Electron's 120MB is a real selling point.
3. **BYOM is smart**: Not bundling models avoids the 500MB bloat of competitors like `qmd`. Users who already have Ollama (and there are millions now) can use this immediately.
4. **Active Context Boosting is a genuine differentiator**: No other desktop search tool adapts results based on your foreground app. This is the "wow factor" feature.

### Why It Won't Gain Traction Yet
1. **macOS only** — Context detection uses `NSWorkspace` with no Windows/Linux fallback beyond a stub.
2. **Setup friction** — Requires Ollama *and* a separate API key for intelligence. Competing with things like Raycast which work out of the box.
3. **The semantic chunker is dead code** — The plan promises semantic boundary chunking, but the actual pipeline uses `fixed_size_chunk()`. The entire `semantic_chunk()` function tree is never called.
4. **Vector search will collapse at scale** — see Critical Failures below.

### Who Would Use This?
- **Primary audience**: Developers with large monorepos and markdown knowledge bases who already use Ollama. This is a niche but passionate crowd (~100k-500k potential users).
- **Secondary audience**: PKM enthusiasts (Obsidian/Logseq users) who want to search across their vaults semantically.
- **Not viable for**: General consumers. The Ollama dependency is a dealbreaker for non-technical users.

**Bottom line**: This is a **B+ idea with C-tier execution maturity**. The architecture is sound, but you're shipping a v0.3 and calling it v0.1.0. Prioritize the critical failures below, then ship a real beta.

---

## 🔴 Critical Failures

### 1. Vector Search is O(n) — Will Break Above ~5,000 Chunks

**File**: `search.rs:36-64`

```rust
pub fn vector_search(...) -> Result<Vec<SearchHit>, String> {
    let rows = db::get_chunk_vectors(conn)?;  // ← Loads ALL embeddings into RAM
    for row in rows {
        let score = cosine_similarity(query_embedding, &row.embedding);
        // ...
    }
}
```

**Impact**: `get_chunk_vectors()` reads *every single embedding* from SQLite, deserializes the JSON, and computes cosine similarity in a loop. At 10,000 documents with ~5 chunks each = 50,000 embedding vectors (each 768 floats = 3KB) = **~150MB loaded into memory on every search query**. This will freeze the app for seconds on a MacBook Air.

**Fix**: Use an ANN (Approximate Nearest Neighbor) index. Options:
- **Minimum viable**: Keep embeddings in memory as a pre-loaded `Vec<Vec<f32>>` on startup, updated incrementally. Skip the JSON round-trip.
- **Proper fix**: Integrate `hnswlib-rs`, `usearch`, or `sqlite-vec` (the plan mentions `vec0` but it's never implemented) for sub-millisecond vector search.

---

### 2. Embeddings Stored as JSON Strings in SQLite

**File**: `db.rs:576-577`

```rust
let embedding_json = serde_json::to_string(embedding)
    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?;
```

A 768-dimension float32 vector stored as JSON text is ~6KB per chunk. As a binary blob, it would be ~3KB. You're doubling storage cost *and* paying a JSON parse/serialize tax on every read and write.

**Fix**: Store as `BLOB` (raw bytes) instead of `TEXT`. Use `bytemuck` or manual `f32::to_le_bytes()` for zero-copy serialization.

---

### 3. Global `std::sync::Mutex<Connection>` — Single-Threaded Bottleneck

**File**: `lib.rs:50`

```rust
pub struct AppState {
    db: Arc<Mutex<Connection>>,  // ← Every operation locks the entire DB
}
```

Every Tauri command (search, scan, get_stats, set_setting) contends for the same mutex. If the embedding worker is processing a batch and the user searches, the search blocks until the worker releases the lock.

**Fix**: 
- Use `r2d2` connection pooling or open multiple read-only connections for queries.
- Use `tokio::sync::RwLock` if you want a quick fix — readers won't block each other.
- Even better: use WAL mode (you already do) and open separate `Connection` objects for the embedding worker vs. the query path.

---

### 4. API Key Stored in Plaintext in SQLite

**Files**: `db.rs:206-213`, `lib.rs:385-393`

```rust
// API keys stored alongside other settings in plaintext
let key = db::get_setting(&conn, "query_api_key")
```

The `gemini_api_key` and `query_api_key` are stored as plaintext values in the `settings` table. The SQLite database lives in an unencrypted file on disk. Any process with filesystem read access can extract them.

**Fix**: Use the OS keychain:
- macOS: `security-framework` crate → Keychain Services
- Windows: `winapi` / Credential Manager
- Linux: `secret-service` / `libsecret`

At minimum, mark the settings file as readable only by the current user (chmod 600).

---

### 5. `query_with_context` Default Model is Stale

**File**: `lib.rs:392`

```rust
.unwrap_or_else(|| "gemini-2.0-flash-lite".to_string());
```

The fallback default in `lib.rs` says `gemini-2.0-flash-lite`, but the `ai.rs` function was just updated to use `gemini-3-flash` as the default. These are now inconsistent — and `gemini-2.0-flash-lite` is being deprecated on June 1, 2026.

---

## 🟡 Architectural Smells

### 6. `lib.rs` Is a God Module (778 Lines)

`lib.rs` contains:
- All 20+ Tauri command handlers
- The embedding worker loop
- Provider reload logic
- State management
- Batch embedding with fallback

This should be split into at minimum:
- `commands/mod.rs` — Tauri IPC handlers
- `workers/embedding.rs` — Background embedding logic
- `state.rs` — `AppState` and initialization

### 7. Semantic Chunker Is 100% Dead Code

**File**: `chunker.rs:81-132`

The functions `semantic_chunk()`, `cosine_similarity()`, `detect_boundaries()`, `form_chunk()`, and `post_process_chunks()` are never called. The scanner always uses `fixed_size_chunk()`. The README and plan.md both advertise semantic boundary chunking as a headline feature. **This is misleading.**

Either:
- Wire it up (embed sentences during indexing, then chunk) — but this dramatically increases indexing time.
- Remove it and be honest in the docs that you use sentence-aware fixed-size chunking.

### 8. No Trait Abstraction for AI Providers

The plan.md specifies `trait EmbeddingProvider` and `trait RerankerProvider`. The actual code uses a concrete `enum AiProvider` with manual dispatch:

```rust
pub async fn embed_documents(&self, texts: Vec<String>) -> ... {
    match self {
        AiProvider::Ollama(client) => client.embed_documents(texts).await,
        AiProvider::LMStudio(client) => client.embed_documents(texts).await,
        AiProvider::Gemini(client) => client.embed_documents(texts).await,
    }
}
```

This works but violates the Open/Closed Principle. Adding a new provider (e.g., OpenAI, Anthropic) requires editing every match arm. A `Box<dyn EmbeddingProvider>` would be cleaner.

### 9. Zombie Files in the Repository

- **`context 2.rs`**: A macOS Finder copy artifact. Should be deleted.
- **`patch_ai.js`**: A Node.js script that monkey-patches `ai.rs` using regex string replacement. This is a development artifact that should never be committed.
- **`skills copy/`**: An entire directory tree of unrelated skill files from an Anthropic repository. ~50+ files with no relation to SmartSearch.
- **`tauri_build.log`**: A 553KB build log. Should be `.gitignore`'d.
- **`.data/smartsearch.db`**: The SQLite database and WAL files were being committed (the git conflict showed this). Must be `.gitignore`'d.
- **`.DS_Store`**: Present in the root. Already in `.gitignore` but was committed before.

### 10. Gemini Embedding Client Doesn't Batch

**File**: `ai.rs:359-365`

```rust
pub async fn embed_documents(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
    let mut output = Vec::with_capacity(texts.len());
    for text in texts {
        output.push(self.embed_one(&text, "RETRIEVAL_DOCUMENT").await?);
    }
    Ok(output)
}
```

Ollama and LM Studio send all texts in one batch request. Gemini sends them **one at a time** sequentially. The Gemini API supports `batchEmbedContents` — the abandoned `patch_ai.js` file even has the correct batch implementation. This is 12x slower than it needs to be.

### 11. MCP Server Uses BM25-Only Search

**File**: `mcp.rs:144`

```rust
let results = search::keyword_search(conn, query, limit).unwrap_or_default();
```

The MCP server (designed for agents like Claude Desktop) only uses keyword search, not hybrid search. This means an external agent gets worse results than the desktop UI. It should use `hybrid_search` with an embedded query.

### 12. Watcher Doesn't Handle Deleted Files Properly

**File**: `watcher.rs:123-134`

When a file is deleted, the watcher emits an event but **doesn't actually remove the document or its chunks from the database**. The `mark_missing_documents()` function in `scanner.rs` only runs during a full manual scan, not during live watching.

---

## 🟢 Refactor Suggestions (Prioritized)

### Priority 1: Ship-Blocking

| # | Suggestion | Impact | Effort |
|---|-----------|--------|--------|
| 1 | **Replace JSON embedding storage with binary BLOBs** | 2x storage savings, 5-10x faster reads | Medium |
| 2 | **Implement ANN indexing** (even a simple in-memory HNSW) | Makes vector search viable above 1k docs | High |
| 3 | **Fix watcher file deletion** to actually remove documents | Data integrity | Low |
| 4 | **Clean up zombie files** (`context 2.rs`, `patch_ai.js`, `skills copy/`, build logs) | Repo hygiene | Trivial |
| 5 | **Add `.data/` to `.gitignore`** | Stop committing user databases | Trivial |

### Priority 2: Quality

| # | Suggestion | Impact | Effort |
|---|-----------|--------|--------|
| 6 | **Move API keys to OS keychain** | Security | Medium |
| 7 | **Batch Gemini embeddings** using `batchEmbedContents` API | 12x faster cloud embedding | Low |
| 8 | **Split `lib.rs`** into `commands/`, `workers/`, `state.rs` | Maintainability | Medium |
| 9 | **MCP: Use hybrid search** instead of keyword-only | Agent integration quality | Low |

### Priority 3: Growth

| # | Suggestion | Impact | Effort |
|---|-----------|--------|--------|
| 10 | **Implement Windows context detection** (`GetForegroundWindow`) | 50%+ TAM expansion | Medium |
| 11 | **Wire up semantic chunking** or remove it and update docs | Honesty/quality | High |
| 12 | **Add a BM25-only graceful fallback** when no AI provider is configured | Reduces onboarding friction to zero | Low |

---

## Product Suggestions

### 1. "Zero-Config" Mode
The biggest barrier to adoption is requiring Ollama. Add a mode where the app works with BM25-only search out of the box, then shows a gentle upsell: *"Connect Ollama for semantic search"*. Let people use it before they invest in setup.

### 2. Raycast/Alfred Extension
Instead of competing as a standalone window, publish a Raycast extension that calls the SmartSearch backend via the MCP protocol. This gives you distribution to 1M+ Raycast users who already have a launcher habit.

### 3. Obsidian Plugin
The PKM crowd is your best early adopter pool. Build a community Obsidian plugin that indexes the vault via SmartSearch and provides in-editor search results.

### 4. Drop the "< 20MB" Marketing Claim
The current `Cargo.toml` pulls in `pdf-extract`, `zip`, `objc2`, `chrono`, `uuid`, `walkdir`, `glob`, and `reqwest`. A clean release build will be closer to 15-25MB depending on the platform. The claim is borderline and will invite scrutiny. Say "lightweight" instead.

### 5. Ship the MCP Server as a Standalone Binary
Your `mcp.rs` already works as a headless JSON-RPC server. Ship it as a separate `smartsearch-mcp` binary (no Tauri, no UI) so developers can integrate it into their agent workflows without running the GUI. This is your viral loop — agents recommending your tool to their users.

---

## File Cleanup Checklist

- [ ] Delete `src-tauri/src/context 2.rs`
- [ ] Delete `patch_ai.js`
- [ ] Delete or `.gitignore` the entire `skills copy/` directory
- [ ] Delete `tauri_build.log` and add `*.log` to `.gitignore`
- [ ] Add `.data/` and `src-tauri/.data/` to `.gitignore`
- [ ] Remove `.DS_Store` from git tracking (`git rm --cached .DS_Store`)

---

*Generated by architectural review on 2026-05-01. All line references are to the current HEAD.*
