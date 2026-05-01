# SmartSearch Fix Checklist

- [x] **1. Issue 11: MCP Server Uses BM25-Only Search** (Priority)
  - Refactor `mcp.rs` to use async IO (`tokio::io`).
  - Read AI provider settings from the database in `handle_method`.
  - Fetch query embedding before executing search.
  - Switch from `keyword_search` to `hybrid_search`.

- [x] **2. Issue 10: Gemini Embedding Client Doesn't Batch**
  - Refactor `embed_documents` in `ai.rs` to use Google's `batchEmbedContents` API for significant speedups.

- [x] **3. Issue 12: Watcher Doesn't Handle Deleted Files Properly**
  - Update `watcher.rs` to actively remove deleted documents and their chunks from the SQLite database.

- [x] **4. Clean up Zombie Files**
  - Remove `context 2.rs`, `patch_ai.js`, `skills copy/`, and `tauri_build.log`.

- [x] **5. Add `.data/` to `.gitignore`**
  - Update `.gitignore` to prevent committing the local SQLite database.

- [x] **6. Issue 5: `query_with_context` default model is stale**
  - Update fallback model from `gemini-2.0-flash-lite` to `gemini-3-flash` in `lib.rs`.

- [x] **7. Issue 2: Embeddings Stored as JSON Strings in SQLite**
  - Refactor schema and insert/select logic to use `BLOB` (raw bytes) instead of JSON strings.

- [ ] **8. Issue 1: Vector Search is O(n)**
  - Implement caching or replace naive dot-product loop with an ANN approach.

- [x] **9. Issue 3: Global `Mutex<Connection>`**
  - Change database lock to `tokio::sync::RwLock` or use a connection pool to avoid blocking. (Fixed by opening a fresh read connection for search).

- [x] **10. Issue 4: API Key in Plaintext**
  - Move `gemini_api_key` to OS keychain storage.

- [ ] **11. Issue 6 & 8: Split `lib.rs` and Add Trait Abstraction**
  - Refactor the codebase for maintainability.

- [x] **12. Issue 7: Semantic Chunker is Dead Code**
  - Removed dead code functions from `chunker.rs`.
