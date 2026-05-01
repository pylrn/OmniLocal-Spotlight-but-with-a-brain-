# 🔍 SmartSearch

> **The Intelligent Desktop Search Engine.**  
> Local-first, privacy-focused search that understands your context.

![SmartSearch Hero](smart_search_hero_1777595654311.png)

SmartSearch is a high-performance, lightweight desktop search utility built with **Tauri 2.0** and **Rust**. It brings the power of modern RAG (Retrieval-Augmented Generation) to your local files with zero compromise on privacy and a footprint under 20MB.

---

## ✨ Key Features

### 🚀 Lightweight by Design
- **Native Performance**: Built with Rust and Tauri 2.0.
- **Minimal Footprint**: Binary size **< 20MB** and idle memory usage of **~30-50MB**.
- **BYOM (Bring Your Own Model)**: Unlike other tools that bundle 500MB+ of weights, SmartSearch connects to your local **Ollama** or **LM Studio** instance.

### 🧠 Intelligent Semantic Search
- **Semantic Chunking**: Uses local embedding model connected to ollama to find natural topic boundaries in your documents, avoiding mid-sentence cuts.
- **Hybrid Ranking**: Blends **BM25 (Keyword)** and **Vector (Semantic)** search using Reciprocal Rank Fusion (RRF) for the most accurate results.
- **Active Context Boosting**: Automatically detects your foreground application (e.g., VS Code, Obsidian) and boosts relevant file types in real-time.

### ⚡️ Real-Time Background Indexing
- **Event-Driven**: Uses the `notify` crate for instant re-indexing when you save a file.
- **Efficient**: Uses `BLAKE3` hashing to only process files that have actually changed.
- **Non-Intrusive**: Background indexing queue with bounded capacity ensures your system stays responsive.

### 🎨 Premium Experience
- **Spotlight-Style UI**: A floating, glassmorphic search bar accessible via global hotkey (`Cmd+Shift+Space`).
- **Rich Previews**: Instant snippets with syntax highlighting and relevance scores.
- **Dark Mode Native**: A refined, HSL-based design system optimized for focus.

---

## 🛠 Tech Stack

- **Backend**: [Rust](https://www.rust-lang.org/) (Tauri 2.0, Tokio, Rusqlite, Notify, Blake3)
- **Frontend**: [React 19](https://react.dev/), [TypeScript](https://www.typescriptlang.org/), [Framer Motion](https://www.framer.com/motion/)
- **Database**: SQLite (FTS5 + Vector Store)
- **AI Integration**: Ollama API, LM Studio API, Google Gemini (Optional Cloud)

---

## 🚀 Getting Started

### Prerequisites
1. **Ollama** (Recommended): [Download Ollama](https://ollama.com/) and pull an embedding model:
   ```bash
   ollama pull nomic-embed-text
   ```
2. **Rust**: [Install Rust](https://www.rust-lang.org/tools/install)
3. **Node.js**: [Install Node.js](https://nodejs.org/)

### Installation
1. Clone the repository:
   ```bash
   git clone https://github.com/your-username/smart-search.git
   cd smart-search
   ```
2. Install dependencies:
   ```bash
   npm install
   ```
3. Run in development mode:
   ```bash
   npm run tauri dev
   ```

---

## 🛡 Privacy & Security
SmartSearch is built on the principle that your data belongs to you.
- **Local-First**: All indexing, chunking, and vector storage happens on your machine.
- **No Telemetry**: We don't track your searches or file contents.
- **Optional Cloud**: Gemini integration is opt-in and requires your own API key.

---

## 📄 License
MIT License. See [LICENSE](LICENSE) for details.
