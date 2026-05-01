// SmartSearch — Hybrid Search Engine
// Combines BM25 keyword search with vector similarity search,
// applies Reciprocal Rank Fusion, and active context boosting.

use std::cmp::Ordering;
use std::collections::HashMap;

use rusqlite::Connection;

use crate::db::{self, SearchHit};

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

pub fn keyword_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<SearchResult>, String> {
    let hits = db::search_bm25(conn, query, limit).map_err(|e| format!("BM25 search error: {}", e))?;
    enrich_hits(conn, hits, false)
}

pub fn vector_search(
    conn: &Connection,
    query_embedding: &[f32],
    limit: usize,
) -> Result<Vec<SearchHit>, String> {
    let rows = db::get_chunk_vectors(conn).map_err(|e| format!("Vector DB error: {}", e))?;
    let mut hits = Vec::new();

    for row in rows {
        if row.embedding.len() != query_embedding.len() || row.embedding.is_empty() {
            continue;
        }

        let score = cosine_similarity(query_embedding, &row.embedding);
        if score.is_finite() {
            hits.push(SearchHit {
                chunk_id: row.chunk_id,
                document_id: row.document_id,
                content: row.content,
                chunk_index: row.chunk_index,
                start_line: row.start_line,
                end_line: row.end_line,
                score,
            });
        }
    }

    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
    hits.truncate(limit);
    Ok(hits)
}

pub fn hybrid_search(
    conn: &Connection,
    query: &str,
    query_embedding: Option<&[f32]>,
    limit: usize,
) -> Result<Vec<SearchResult>, String> {
    let bm25_hits = db::search_bm25(conn, query, limit.saturating_mul(3).max(10))
        .map_err(|e| format!("BM25 search error: {}", e))?;

    let vector_hits = if let Some(embedding) = query_embedding {
        vector_search(conn, embedding, limit.saturating_mul(3).max(10))?
    } else {
        Vec::new()
    };

    if vector_hits.is_empty() {
        return enrich_hits(conn, bm25_hits, false).map(|mut results| {
            results.truncate(limit);
            results
        });
    }

    let fused = hybrid_search_rrf(&bm25_hits, &vector_hits, 60.0);
    enrich_hits(conn, fused, false).map(|mut results| {
        results.truncate(limit);
        results
    })
}

pub fn hybrid_search_rrf(bm25_hits: &[SearchHit], vector_hits: &[SearchHit], k: f64) -> Vec<SearchHit> {
    let mut scores: HashMap<(i64, i64), SearchHit> = HashMap::new();

    for (rank, hit) in bm25_hits.iter().enumerate() {
        let rrf_score = 1.0 / (k + rank as f64 + 1.0);
        let entry = scores.entry((hit.document_id, hit.chunk_id)).or_insert_with(|| SearchHit {
            score: 0.0,
            ..hit.clone()
        });
        entry.score += rrf_score;
    }

    for (rank, hit) in vector_hits.iter().enumerate() {
        let rrf_score = 1.0 / (k + rank as f64 + 1.0);
        let entry = scores.entry((hit.document_id, hit.chunk_id)).or_insert_with(|| SearchHit {
            score: 0.0,
            ..hit.clone()
        });
        entry.score += rrf_score;
    }

    let mut fused: Vec<SearchHit> = scores.into_values().collect();
    fused.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
    fused
}

pub fn get_context_boost(foreground_app: &str, file_type: &str) -> f64 {
    let app = foreground_app.to_lowercase();

    let code_apps = [
        "visual studio code",
        "code",
        "cursor",
        "zed",
        "neovim",
        "nvim",
        "intellij",
        "webstorm",
        "pycharm",
        "goland",
        "rustrover",
        "vim",
        "emacs",
        "sublime text",
    ];
    let writing_apps = [
        "obsidian",
        "notion",
        "typora",
        "ia writer",
        "bear",
        "ulysses",
        "word",
        "pages",
        "logseq",
        "roam",
        "craft",
    ];
    let browser_apps = ["safari", "chrome", "chromium", "firefox", "arc", "edge", "brave", "opera", "vivaldi"];

    let code_types = [
        "rs", "py", "ts", "tsx", "js", "jsx", "go", "java", "c", "cpp", "h", "hpp", "rb", "swift", "kt", "scala",
        "zig", "lua", "sh", "bash", "zsh",
    ];
    let doc_types = ["md", "markdown", "txt", "org", "tex", "rst", "adoc", "docx", "rtf", "pdf"];
    let web_types = ["html", "css", "scss", "sass", "less", "vue", "svelte"];

    if code_apps.iter().any(|candidate| app.contains(candidate)) && code_types.contains(&file_type) {
        return 0.20;
    }

    if writing_apps.iter().any(|candidate| app.contains(candidate)) && doc_types.contains(&file_type) {
        return 0.20;
    }

    if browser_apps.iter().any(|candidate| app.contains(candidate)) && web_types.contains(&file_type) {
        return 0.15;
    }

    0.0
}

pub fn enrich_hits(
    conn: &Connection,
    hits: Vec<SearchHit>,
    is_context_boosted: bool,
) -> Result<Vec<SearchResult>, String> {
    let mut results = Vec::new();

    for hit in hits {
        if let Some(doc) = db::get_document_by_id(conn, hit.document_id).map_err(|e| format!("DB error: {}", e))? {
            let snippet = build_snippet(&hit.content);
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

    dedupe_by_document(results)
}

fn dedupe_by_document(results: Vec<SearchResult>) -> Result<Vec<SearchResult>, String> {
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

    deduped.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
    Ok(deduped)
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| (*x as f64) * (*y as f64)).sum();
    let norm_a: f64 = a.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| (*x as f64) * (*x as f64)).sum::<f64>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

fn build_snippet(content: &str) -> String {
    let normalized = content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if normalized.len() > 220 {
        format!("{}...", &normalized[..220])
    } else {
        normalized
    }
}
