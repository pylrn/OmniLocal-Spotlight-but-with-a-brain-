# SmartSearch: Application Overview & Feature State

**Date:** May 6, 2026
**Architecture:** Tauri 2 (Rust) + React 19 (TypeScript, Vite)

This document provides a comprehensive overview of every feature, component, and architectural layer in the current state of the application.

---

## 1. Core Concept
SmartSearch is an intelligent, local-first hybrid desktop search engine. It provides semantic search, keyword search, file indexing, and AI-driven insights directly integrated into the user's desktop environment without relying on external cloud providers.

---

## 2. Frontend Features (React / UI)
The user interface is built with React 19, TypeScript, and animated using `framer-motion`.

### 2.1 Search Experience
*   **Search Bar:** Handles standard keyword inputs and acts as the trigger for the semantic operations.
*   **Hybrid Results List:** Displays search results dynamically using card components (`ResultCard`/`CompactRow`), rendering file names, summaries/snippets, and meta-data.
*   **Result Filters:** Filters populated dynamically. Users can refine search results by **File Type** (`.pdf`, `.md`, `.rs`, etc.) and by **Collection Name**.
*   **Empty & State Management:** Dynamic visual feedback for No Results, Initial Setup, and Indexing loading indicators.

### 2.2 AI Insight Panel
*   **Contextual Summaries:** Automatically triggered when results are gathered. Extracts the top 5 results' snippets and queries the local AI to generate a natural-language contextual answer mapping to the user's query.
*   **Dismissable & Expandable:** UI allows the user to dismiss AI insights to save space or jump directly into the "Intelligence" settings.

### 2.3 Settings & Configuration (Multi-layered)
*   **Quick Settings:** A popover for rapid access to commonly tweaked configurations.
*   **Advanced Settings / Settings Window:** Comprehensive configuration panels covering:
    *   **Library:** Managing indexed folders/directories.
    *   **Intelligence:** Configuring AI providers (LM Studio, Ollama).
    *   **Overview/General:** General desktop integrations.

### 2.4 Desktop Integration & UX
*   **Global Shortcut Integration:** Quick toggle via `Cmd + Shift + Space` (from Tauri global shortcut).
*   **Native File Opener:** Clicking a result opens the file natively in the OS (`@tauri-apps/plugin-opener`).
*   **Manual File Addition:** Uses native system dialogs (`@tauri-apps/plugin-dialog`) to allow users to manually pick multiple files for instant indexing without setting up a watched library collection.
*   **Visual Enhancements:** Features like the `AuroraBackdrop` component providing aesthetic ambient lighting/animations behind the search panel.

---

## 3. Backend Features & Systems (Rust / Tauri)
The backend is a robust Rust application powered by `.tauri`.

### 3.1 Database & Semantic Engine
*   **Vector Search & Storage:** Uses `rusqlite` bundled with the **`sqlite-vec`** extension. It enables semantic/vector search operations directly inside a fast SQLite database without deploying heavier standalone vector systems.
*   **Collections:** Supports grouping resources securely via `collections.rs` allowing users to index files partitioned by specific projects or drives.

### 3.2 AI & Inference Layer (`ai.rs` & `embedding.rs`)
*   **LLM Providers:** Implements robust client functions using `reqwest` to interact with LLMs compatible with the OpenAI spec — primarily targeting local inference loops like **Ollama** and **LM Studio**.
*   **Embedding Pipeline:** Automatically processes structured inputs, breaking them apart and creating vector embeddings (`uuid` tracked chunking via `blake3` fast hashing) mapped to file paths.

### 3.3 File System Indexing & Parsing
*   **Watchers:** `notify` and `notify-debouncer-mini` continuously scan indexed directories for changes, triggering delta-updates to the SQLite engine.
*   **Document Extractors:** Capable of indexing native text via `pdf-extract` for PDFs and `zip` capabilities for raw DOCX extraction.
*   **Chunker (`chunker.rs`):** Employs intelligent character chunking algorithms mapped via `unicode-segmentation` to preserve sentence structure when breaking large documents like reports or source code.

### 3.4 Model Context Protocol (`mcp.rs`)
*   **Extensible AI Tools:** Implements Model Context Protocol (MCP) integrations allowing the SmartSearch system to hook into local/desktop MCP servers as a Host. It allows the AI insight panel or semantic searches to potentially augment their data with tools running locally on stdio.

### 3.5 macOS Specific Capabilities
*   **Context Awareness:** `objc2` and `NSWorkspace` integrations allow the backend to identify the current active application/running processes on macOS, theoretically using active context (e.g. what IDE or browser tab is opened) to tune search relevance or capture quick insights.
