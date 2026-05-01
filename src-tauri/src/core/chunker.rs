// SmartSearch — Semantic Boundary Chunker
// Splits documents into coherent chunks based on topic transitions,
// not arbitrary token limits.

use unicode_segmentation::UnicodeSegmentation;

/// Configuration for the semantic chunker
#[derive(Debug, Clone)]
pub struct ChunkerConfig {
    /// Maximum tokens per chunk (fallback split if exceeded)
    pub max_chunk_tokens: usize,
    /// Minimum tokens per chunk (merge with adjacent if below)
    pub min_chunk_tokens: usize,
    /// Percentile threshold for boundary detection (0.0 - 1.0)
    /// Higher = more aggressive splitting
    pub boundary_percentile: f64,
    /// Approximate tokens-per-word ratio for rough estimation
    pub tokens_per_word: f64,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            max_chunk_tokens: 1500,
            min_chunk_tokens: 50,
            boundary_percentile: 0.90,
            tokens_per_word: 1.3,
        }
    }
}

/// A text chunk with metadata
#[derive(Debug, Clone, serde::Serialize)]
pub struct TextChunk {
    pub content: String,
    pub chunk_index: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub estimated_tokens: usize,
}

/// Segment text into sentences using Unicode-aware segmentation
pub fn segment_sentences(text: &str) -> Vec<SentenceSpan> {
    let mut sentences = Vec::new();
    let mut current_line = 1;
    let mut pos = 0;

    // Use unicode_segmentation for sentence boundaries
    for sentence in text.split_sentence_bounds() {
        let trimmed = sentence.trim();
        if trimmed.is_empty() {
            // Count newlines in whitespace
            current_line += sentence.matches('\n').count();
            pos += sentence.len();
            continue;
        }

        let start_line = current_line;
        current_line += sentence.matches('\n').count();
        let end_line = current_line;

        sentences.push(SentenceSpan {
            text: trimmed.to_string(),
            start_line,
            end_line,
            start_offset: pos,
            end_offset: pos + sentence.len(),
        });

        pos += sentence.len();
    }

    sentences
}

/// Fallback chunking: fixed-size with overlap (used when no embeddings available)
pub fn fixed_size_chunk(text: &str, config: &ChunkerConfig) -> Vec<TextChunk> {
    let sentences = segment_sentences(text);
    if sentences.is_empty() {
        return vec![];
    }

    let mut chunks = Vec::new();
    let mut current_chunk_sentences: Vec<&SentenceSpan> = Vec::new();
    let mut current_tokens = 0;

    for sentence in &sentences {
        let sentence_tokens = estimate_tokens(&sentence.text, config.tokens_per_word);

        if current_tokens + sentence_tokens > config.max_chunk_tokens && !current_chunk_sentences.is_empty() {
            // Emit current chunk
            let content: String = current_chunk_sentences
                .iter()
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");

            chunks.push(TextChunk {
                content,
                chunk_index: chunks.len(),
                start_line: current_chunk_sentences.first().unwrap().start_line,
                end_line: current_chunk_sentences.last().unwrap().end_line,
                estimated_tokens: current_tokens,
            });

            current_chunk_sentences.clear();
            current_tokens = 0;
        }

        current_chunk_sentences.push(sentence);
        current_tokens += sentence_tokens;
    }

    // Emit remaining
    if !current_chunk_sentences.is_empty() {
        let content: String = current_chunk_sentences
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        chunks.push(TextChunk {
            content,
            chunk_index: chunks.len(),
            start_line: current_chunk_sentences.first().unwrap().start_line,
            end_line: current_chunk_sentences.last().unwrap().end_line,
            estimated_tokens: current_tokens,
        });
    }

    chunks
}

// ═══════════════════════════════════════════════════════════════════════
// Internal helpers
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct SentenceSpan {
    pub text: String,
    pub start_line: usize,
    pub end_line: usize,
    pub start_offset: usize,
    pub end_offset: usize,
}

/// Rough token count estimation (word count × tokens-per-word ratio)
fn estimate_tokens(text: &str, tokens_per_word: f64) -> usize {
    let word_count = text.split_whitespace().count();
    (word_count as f64 * tokens_per_word) as usize
}

// ═══════════════════════════════════════════════════════════════════════
// Extract title from document content
// ═══════════════════════════════════════════════════════════════════════

/// Extract a title from markdown content (first # heading)
pub fn extract_title(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") {
            return Some(trimmed.trim_start_matches("# ").to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sentence_segmentation() {
        let text = "Hello world. This is a test. Another sentence here.";
        let sentences = segment_sentences(text);
        assert!(sentences.len() >= 3);
    }


    #[test]
    fn test_fixed_size_chunk() {
        let text = "First sentence. Second sentence. Third sentence. Fourth sentence. Fifth sentence.";
        let config = ChunkerConfig {
            max_chunk_tokens: 10,
            min_chunk_tokens: 2,
            ..Default::default()
        };
        let chunks = fixed_size_chunk(text, &config);
        assert!(!chunks.is_empty());
        // All text should be covered
        let reconstructed: String = chunks.iter().map(|c| c.content.as_str()).collect::<Vec<_>>().join(" ");
        assert!(reconstructed.contains("First") && reconstructed.contains("Fifth"));
    }

    #[test]
    fn test_extract_title() {
        let content = "Some preamble\n# My Document Title\n\nBody text here.";
        assert_eq!(extract_title(content), Some("My Document Title".to_string()));
    }

    #[test]
    fn test_extract_title_first_line() {
        let content = "# Quick Start Guide\n\nThis is the body.";
        assert_eq!(extract_title(content), Some("Quick Start Guide".to_string()));
    }
}
