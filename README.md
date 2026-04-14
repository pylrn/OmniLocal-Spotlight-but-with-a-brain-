# 🧠 OmniLocal

**OmniLocal is an ultra-lightweight, private AI search app that replaces Spotlight. Running 100% offline, it uses tiny local AI to find files by their *meaning*, not just exact words. It quietly organizes data in the background and magically tailors your results based on the app you are currently using.**

---

## 💡 The Problem
Standard OS search (Spotlight, Windows Search) relies purely on exact keyword matching. If you search for "database migration", it won't find your note titled "Postgres setup steps". Cloud AI tools can find it, but uploading your private journals, company code, and financial PDFs to the cloud is a massive privacy risk.

**OmniLocal** solves this by acting as a "second brain" that lives entirely on your local machine. 

## ✨ Key Features

* 🔒 **100% Local & Private:** No API keys, no cloud servers, no telemetry. Your files never leave your machine.
* ⚡ **Ultra-Lightweight:** Built with Tauri instead of Electron, it uses native OS webviews to keep RAM usage incredibly low.
* 🧠 **Hybrid Search (Meaning + Keywords):** It combines the blazing speed of exact keyword matching (BM25) with semantic vector search. If you search for "password logic", it knows to return `auth_crypto.ts`.
* 🎯 **Active Context Boosting:** A "mind-reading" UX. If you press the search hotkey while VS Code is your active window, it automatically boosts `.ts` and `.py` files. If you are in Obsidian, it prioritizes `.md` files. 
* 📁 **Semantic Smart Folders:** Never organize files manually again. Create a folder named "Tax Documents" and the app uses vector math to automatically pull in receipts and invoices based on their *concept*, regardless of where they are saved.
* 🔄 **Invisible Background Sync:** No manual indexing. It silently watches your folders and updates its brain the millisecond you hit `Ctrl+S` on a file.

## 🏗️ How It Works (The Architecture)

OmniLocal doesn't scan your hard drive when you search—that would melt your computer. Instead, it uses **Event-Driven Indexing** and **Approximate Nearest Neighbors (ANN)** math:

1. **Ingestion:** A background daemon watches your user folders. When a file is created or changed, it reads the text, splits it into chunks, and passes it to a tiny local AI model to convert the text into a "Vector" (a mathematical representation of the concept).
2. **Storage:** These vectors and text chunks are saved into a highly efficient local SQLite database.
3. **Retrieval:** When you type a query, the database calculates the "distance" between your query's vector and your files' vectors using an HNSW index. It merges this with traditional keyword matches to give you the perfect result in milliseconds.

## 🛠️ Tech Stack

* **Frontend UI:** [Tauri](https://tauri.app/) + React/Svelte. Creates the fast, floating desktop search bar using native OS rendering.
* **Background Engine:** [Bun](https://bun.sh/) (or Node.js). Powers the invisible background file watcher (`chokidar`) and handles OS-level events (`active-win`).
* **Database:** SQLite. 
  * `FTS5` extension for instant keyword matching.
  * `sqlite-vec` extension for storing vector embeddings and fast semantic math.
* **Local AI:** [Ollama](https://ollama.com/) / `node-llama-cpp`. Runs small, open-source embedding models (like `nomic-embed-text`) entirely on your CPU/GPU.

## 🚀 Getting Started

*(Note: This project is currently in active development as a summer project. Installation instructions and binaries will be provided upon the v1.0 release.)*

### Prerequisites (For Developers)
* [Bun](https://bun.sh/) installed
* [Rust/Cargo](https://rustup.rs/) (required for Tauri)
* [Ollama](https://ollama.com/) installed and running locally

## 🗺️ Roadmap
- [ ] Core background file watcher and SQLite integration
- [ ] BM25 + Vector Hybrid Search logic
- [ ] Tauri GUI and Global Hotkey binding (`Cmd+Space`)
- [ ] Active OS Window Context detection
- [ ] LLM Synthesis (Press `Enter` to summarize top results)
- [ ] Pre-packaged binaries for macOS and Windows
