# SmartSearch Performance Optimization & Crash Reduction Guide

## Executive Summary
You are experiencing severe lag and crashes when opening the "Library" or "Advanced Settings" views because the application currently suffers from **Frontend Rendering Overload** and **Unthrottled IPC Communication**. The SQLite database has grown to ~3GB, containing thousands of documents, and the frontend attempts to load and render all of them simultaneously while constantly re-fetching them during indexing.

---

## 🔴 Root Causes of Crashes & Lag

### 1. Massive Unvirtualized DOM Rendering
**Where**: `src/components/Settings/Settings.tsx`
**The Problem**: The app fetches `documentStatuses` (which could be 10,000+ rows) and maps over them directly into the React DOM. 
* Browsers cannot handle rendering 10,000 complex DOM rows (each with buttons, SVGs, and state) at once. It causes immediate freezing, memory spikes, and an eventual "Aw, Snap!" browser crash.

### 2. Unthrottled IPC Polling (Event Storming)
**Where**: `src/components/Settings/Settings.tsx` (in the `useEffect` hook)
**The Problem**: You are listening to the `listenToEmbeddingProgress` event and triggering `refreshDocuments()` on *every single event*.
* If the Rust backend processes 10 chunks a second, it fires 10 events a second.
* The frontend responds by requesting the *entire* list of 10,000 documents via the Tauri IPC bridge 10 times a second.
* Serializing, transferring, parsing, and rendering 300MB of JSON per second via IPC instantly locks up the UI thread and crashes the webview.

### 3. Backend Global DB Locking & Memory Bloat
**Where**: `src-tauri/src/core/db.rs` and Vector Search
**The Problem**: 
* Every time the frontend calls `listDocumentStatuses`, it requires a lock on the database. Under heavy event storming, this starves the indexing worker.
* The vector search currently uses $O(n)$ RAM lookup. When you open settings and run searches, the backend loads massive amounts of data into RAM, leading to Out-Of-Memory (OOM) crashes in the Rust process.

---

## 🟢 Actionable Solutions (How to Fix)

### Frontend Fixes (High Priority)
1. **Implement Virtualization (Windowing)**:
   - Do not render the entire `documentStatuses` array. Use a library like `react-virtual` or `react-window` to only render the 15-20 rows currently visible on the screen.
2. **Throttle/Debounce Progress Refreshing**:
   - Instead of calling `refreshDocuments()` on every single embedding progress tick, debounce it. Only re-fetch the massive library list once every 2-3 seconds, or only update the *specific* document being processed rather than the whole list.
3. **Pagination or Search Filtering**:
   - In the Library tab, default to showing only the first 50 documents, and add a "Search Library" text input to filter down the list on the Rust side, limiting the IPC payload size.

### Backend Fixes (Memory Optimization)
4. **HNSW Vector Indexing**:
   - Implement `sqlite-vec` properly. Stop loading all vectors into `Vec<Vec<f32>>` during search. This is the #1 reason the backend crashes on large datasets.
5. **Optimize IPC Payloads**:
   - `listDocumentStatuses` should accept `limit` and `offset` parameters so the Rust backend only serializes 50 rows at a time to JSON instead of thousands.

*Note: These insights have been logged and the documentation files have been updated to reflect these performance tuning requirements.*
