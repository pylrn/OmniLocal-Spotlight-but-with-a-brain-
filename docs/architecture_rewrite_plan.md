# Architectural Rewrite Plan

This document outlines the detailed strategy for resolving the two final, large-scale technical debt items in the SmartSearch codebase: solving the O(n) vector search bottleneck, and decomposing the monolithic `lib.rs` file.

---

## Part 1: Resolving Vector Search O(n) (Issue #1)

Currently, `search.rs` performs a naive linear scan (dot product) across every chunk in the SQLite database for every search query. As the user's document collection grows into thousands of files, this will cause heavy CPU spikes and unacceptably slow search times.

### Target Approach: `sqlite-vec` or In-Memory `hnsw`
Given SmartSearch's local-first architecture built heavily around SQLite, keeping vector storage close to the metadata is highly desirable.

#### Option A: `sqlite-vec` Extension (Recommended)
`sqlite-vec` is an extremely fast, zero-dependency SQLite extension for vector search that integrates perfectly with local Rust apps.
1. **Dependency:** Add `sqlite-vec` to `Cargo.toml`.
2. **Initialization:** In `db::init_db`, register the `sqlite-vec` extension and create an explicit virtual table for the vectors (e.g., `CREATE VIRTUAL TABLE vec_chunks USING vec0(embedding float[768])`).
3. **Insertion:** When `ai.rs` completes an embedding, we `INSERT INTO vec_chunks(rowid, embedding) VALUES (?, ?)`.
4. **Search Query:** Refactor `search::hybrid_search` to use the `knn` (k-nearest neighbors) function directly within the SQL query.
   * *Example:* `SELECT rowid, distance FROM vec_chunks WHERE embedding MATCH ?1 AND k = 10`
   * This offloads the math from Rust to highly optimized C/SIMD logic, making it sub-millisecond.

#### Option B: In-Memory Caching + Rust ANN (`hnswlib-rs` / `instant-distance`)
If adding C extensions to SQLite via Tauri proves difficult across platforms (Windows/Mac/Linux), an pure-Rust approach is best.
1. **Cache Layer:** Load all vectors into a `tokio::sync::RwLock<HnswIndex>` inside `AppState` upon application startup.
2. **Event Hooks:** When the background worker embeds a new chunk, push the vector into the in-memory index *and* SQLite.
3. **Querying:** Search the in-memory HNSW index to get the top `N` `chunk_id`s in `O(log n)` time, and then fetch only those specific chunks from SQLite to build the UI context.

---

## Part 2: Splitting `lib.rs` & Trait Abstraction (Issues #6 & #8)

`lib.rs` has grown into an 800-line monolith responsible for Tauri commands, application state, database access wrappers, and background daemon logic. This makes it a bottleneck for maintainability and parallel development.

### Step 1: Directory Restructuring
We will create a structured modules hierarchy within `src-tauri/src`:
```text
src/
├── main.rs            # Tauri application entry point and plugin setup
├── lib.rs             # Exports modules, defines AppState
├── commands/          # Submodule for all #[tauri::command] handlers
│   ├── collections.rs # add_collection, list_collections, remove_collection
│   ├── settings.rs    # set_setting, get_stats, list_models
│   └── search.rs      # search_keyword, scan_collections, add_manual_files
├── workers/           # Submodule for background daemons
│   ├── embedding.rs   # Process pending embeddings loop
│   └── mod.rs         
├── core/
│   ├── ai.rs          # (Moved) Trait definitions and clients
│   ├── db.rs          # Database interaction
│   └── search.rs      # Search logic
```

### Step 2: Extracting Tauri Commands
- We will rip all `#[tauri::command]` functions out of `lib.rs`.
- Commands will be grouped by domain into `commands/collections.rs`, `commands/search.rs`, etc.
- In `lib.rs`, we will simply mount them:
  `invoke_handler: tauri::generate_handler![commands::collections::add_collection, ...]`

### Step 3: Trait Abstraction for `AiProvider`
Currently, `AiProvider` is a hardcoded `enum` holding clients (`Gemini`, `Ollama`, `LMStudio`). To support future providers (like OpenAI, Claude, or local CoreML models) without endless `match` statements, we will introduce trait abstraction using the `async-trait` crate.

1. **Define the Traits:**
```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed_documents(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String>;
    async fn embed_query(&self, query: String) -> Result<Vec<f32>, String>;
}

#[async_trait]
pub trait GenerationProvider: Send + Sync {
    async fn generate_answer(&self, prompt: &str, system_prompt: &str) -> Result<String, String>;
}
```

2. **Implementations:**
Have `GeminiClient`, `OllamaClient`, etc., explicitly implement these traits.

3. **Dynamic Dispatch (Optional but recommended):**
Store `Arc<dyn EmbeddingProvider>` in `AppState` instead of the rigid `AiProvider` enum. This allows the background worker to hot-swap providers via polymophism.
