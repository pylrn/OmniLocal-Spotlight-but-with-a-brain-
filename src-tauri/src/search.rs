// SmartSearch — Hybrid Search Engine
// Combines BM25 keyword search with vector similarity search,
// applies Reciprocal Rank Fusion, and active context boosting.

use std::collections::HashMap;
use rusqlite::Connection;
use log::{debug, info};

use crate::db::{self, SearchHit, DocumentRow};

/// A fully enriched search result returned to the frontend
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub document_id: i64,
    pub chunk_id: i64,
    pub title: Option<String>,
    pub path: String,
    pub abs_path: String,
    pub collection_name: String,
    pub collection_context: Option<String>,
    pub file_type: String,
    pub snippet: String,
    pub score: f64,
    pub chunk_index: i32,
    pub start_line: Option<i32>,
    pub end_line: Option<i32>,
    pub context_boosted: bool,
}

/// Search modes
#[derive(Debug, Clone, serde::Deserialize)]
pub enum SearchMode {
    Keyword,    // BM25 only
    Vector,     // Vector similarity only
    Hybrid,     // BM25 + Vector + RRF
}

/// Perform BM25 keyword search
pub fn keyword_search(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>, String> {
    let hits = db::search_bm25(conn, query, limit)
        .map_err(|e| format!("BM25 search error: {}", e))?;

    enrich_hits(conn, hits, false)
}

/// Perform hybrid search with RRF fusion
///
/// `bm25_hits` and `vector_hits` are the raw results from each backend.
/// They are combined using Reciprocal Rank Fusion.
pub fn hybrid_search_rrf(
    bm25_hits: &[SearchHit],
    vector_hits: &[SearchHit],
    active_context_boost: Option<(&str, &dyn Fn(&str) -> f64)>,
    k: f64,  // RRF constant (default: 60.0)
) -> Vec<(i64, i64, f64, bool)> {
    // Map from (document_id, chunk_id) -> (rrf_score, boosted)
    let mut scores: HashMap<(i64, i64), (f64, bool)> = HashMap::new();

    // Add BM25 results with rank-based scoring
    for (rank, hit) in bm25_hits.iter().enumerate() {
        let rrf_score = 1.0 / (k + rank as f64 + 1.0);
        let entry = scores.entry((hit.document_id, hit.chunk_id)).or_insert((0.0, false));
        entry.0 += rrf_score;
    }

    // Add vector results with rank-based scoring
    for (rank, hit) in vector_hits.iter().enumerate() {
        let rrf_score = 1.0 / (k + rank as f64 + 1.0);
        let entry = scores.entry((hit.document_id, hit.chunk_id)).or_insert((0.0, false));
        entry.0 += rrf_score;
    }

    // Apply active context boost if available
    if let Some((foreground_app, boost_fn)) = active_context_boost {
        // We need document info — for now we'll apply boost based on what we have
        // The actual file_type lookup happens when we enrich
        for ((doc_id, chunk_id), (score, boosted)) in scores.iter_mut() {
            // This is a placeholder — actual boost needs file_type from the document
            // The boost is applied during enrichment
        }
    }

    // Sort by score descending
    let mut sorted: Vec<(i64, i64, f64, bool)> = scores
        .into_iter()
        .map(|((doc_id, chunk_id), (score, boosted))| (doc_id, chunk_id, score, boosted))
        .collect();

    sorted.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    sorted
}

/// Apply active context boosting based on foreground application
pub fn get_context_boost(foreground_app: &str, file_type: &str) -> f64 {
    let app = foreground_app.to_lowercase();

    let code_apps = ["visual studio code", "code", "cursor", "zed",
                     "neovim", "nvim", "intellij", "webstorm", "pycharm",
                     "goland", "rustrover", "vim", "emacs", "sublime text"];
    let writing_apps = ["obsidian", "notion", "typora", "ia writer",
                        "bear", "ulysses", "word", "pages", "logseq",
                        "roam", "craft"];
    let browser_apps = ["safari", "chrome", "chromium", "firefox", "arc",
                        "edge", "brave", "opera", "vivaldi"];

    let code_types = ["rs", "py", "ts", "tsx", "js", "jsx", "go", "java",
                      "c", "cpp", "h", "hpp", "rb", "swift", "kt", "scala",
                      "zig", "lua", "sh", "bash", "zsh"];
    let doc_types = ["md", "markdown", "txt", "org", "tex", "rst", "adoc",
                     "docx", "rtf"];
    let web_types = ["html", "css", "scss", "sass", "less", "vue", "svelte"];

    // Code editor active → boost code files
    if code_apps.iter().any(|a| app.contains(a)) {
        if code_types.contains(&file_type) {
            return 0.20;
        }
    }

    // Writing app active → boost document files
    if writing_apps.iter().any(|a| app.contains(a)) {
        if doc_types.contains(&file_type) {
            return 0.20;
        }
    }

    // Browser active → boost web files
    if browser_apps.iter().any(|a| app.contains(a)) {
        if web_types.contains(&file_type) {
            return 0.15;
        }
    }

    0.0
}

/// Enrich search hits with full document metadata
pub fn enrich_hits(
    conn: &Connection,
    hits: Vec<SearchHit>,
    is_context_boosted: bool,
) -> Result<Vec<SearchResult>, String> {
    let mut results = Vec::new();

    for hit in hits {
        if let Some(doc) = db::get_document_by_id(conn, hit.document_id)
            .map_err(|e| format!("DB error: {}", e))?
        {
            // Create a snippet (first 200 chars of chunk content)
            let snippet = if hit.content.len() > 200 {
                format!("{}...", &hit.content[..200])
            } else {
                hit.content.clone()
            };

            results.push(SearchResult {
                document_id: hit.document_id,
                chunk_id: hit.chunk_id,
                title: doc.title,
                path: doc.path,
                abs_path: doc.abs_path,
                collection_name: doc.collection_name,
                collection_context: doc.collection_context,
                file_type: doc.file_type,
                snippet,
                score: hit.score,
                chunk_index: hit.chunk_index,
                start_line: hit.start_line,
                end_line: hit.end_line,
                context_boosted: is_context_boosted,
            });
        }
    }

    // Deduplicate by document (keep highest scoring chunk per document)
    let mut seen_docs: HashMap<i64, usize> = HashMap::new();
    let mut deduped: Vec<SearchResult> = Vec::new();

    for result in results {
        if let Some(&existing_idx) = seen_docs.get(&result.document_id) {
            if result.score > deduped[existing_idx].score {
                deduped[existing_idx] = result;
            }
        } else {
            seen_docs.insert(result.document_id, deduped.len());
            deduped.push(result);
        }
    }

    // Sort by final score
    deduped.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    Ok(deduped)
}
