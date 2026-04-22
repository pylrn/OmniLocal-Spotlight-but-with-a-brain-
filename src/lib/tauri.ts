// SmartSearch — Typed IPC Wrappers
import { invoke } from '@tauri-apps/api/core';

// ═══════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════

export interface Collection {
  id: number;
  name: string;
  path: string;
  glob_pattern: string | null;
  context: string | null;
  created_at: string | null;
  doc_count: number;
}

export interface DocumentRow {
  id: number;
  collection_id: number;
  path: string;
  abs_path: string;
  title: string | null;
  file_type: string;
  file_size: number;
  collection_name: string;
}

export interface SearchResult {
  document_id: number;
  chunk_id: number;
  title: string | null;
  path: string;
  abs_path: string;
  collection_name: string;
  collection_context: string | null;
  file_type: string;
  snippet: string;
  score: number;
  chunk_index: number;
  start_line: number | null;
  end_line: number | null;
  context_boosted: boolean;
}

export interface ScanResult {
  collection_name: string;
  files_found: number;
  files_indexed: number;
  files_unchanged: number;
  files_removed: number;
  errors: string[];
}

export interface IndexStats {
  total_collections: number;
  total_documents: number;
  total_chunks: number;
  embedded_chunks: number;
}

export interface ProviderStatus {
  connected: boolean;
  provider: 'Ollama' | 'LMStudio';
  model_available: boolean;
  model_name: string;
  error: string | null;
}

// ═══════════════════════════════════════════════════════════════
// IPC Functions
// ═══════════════════════════════════════════════════════════════

export async function addCollection(
  name: string,
  path: string,
  globPattern?: string,
  context?: string,
): Promise<number> {
  return invoke('add_collection', {
    name,
    path,
    globPattern: globPattern || null,
    context: context || null,
  });
}

export async function listCollections(): Promise<Collection[]> {
  return invoke('list_collections');
}

export async function removeCollection(name: string): Promise<boolean> {
  return invoke('remove_collection', { name });
}

export async function addManualFiles(paths: string[]): Promise<void> {
  return invoke('add_manual_files', { paths });
}

export async function listIndexedFiles(): Promise<DocumentRow[]> {
  return invoke('list_indexed_files');
}

export async function removeManualFile(documentId: number): Promise<void> {
  return invoke('remove_manual_file', { documentId });
}

export async function scanCollections(): Promise<ScanResult[]> {
  return invoke('scan_collections');
}

export async function searchKeyword(
  query: string,
  limit?: number,
): Promise<SearchResult[]> {
  return invoke('search_keyword', { query, limit: limit || null });
}

export async function getIndexStats(): Promise<IndexStats> {
  return invoke('get_index_stats');
}

export async function checkAiStatus(): Promise<ProviderStatus> {
  return invoke('check_ai_status');
}

export async function getSetting(key: string): Promise<string | null> {
  return invoke('get_setting', { key });
}

export async function setSetting(key: string, value: string): Promise<void> {
  return invoke('set_setting', { key, value });
}

export async function getForegroundApp(): Promise<string | null> {
  return invoke('get_foreground_app');
}

// ═══════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════

export function getFileTypeColor(fileType: string): string {
  const colors: Record<string, string> = {
    md: 'var(--filetype-md)',
    markdown: 'var(--filetype-md)',
    ts: 'var(--filetype-ts)',
    tsx: 'var(--filetype-ts)',
    js: 'var(--filetype-js)',
    jsx: 'var(--filetype-js)',
    py: 'var(--filetype-py)',
    rs: 'var(--filetype-rs)',
    go: 'var(--filetype-go)',
  };
  return colors[fileType] || 'var(--filetype-default)';
}

export function getFileTypeIcon(fileType: string): string {
  const icons: Record<string, string> = {
    md: '📄',
    markdown: '📄',
    ts: '🔧',
    tsx: '🔧',
    js: '🔧',
    jsx: '🔧',
    py: '🐍',
    rs: '⚙️',
    go: '🔵',
    txt: '📝',
    json: '{ }',
    yaml: '📋',
    yml: '📋',
    html: '🌐',
    css: '🎨',
  };
  return icons[fileType] || '📄';
}

export function formatScore(score: number): string {
  return `${Math.round(score * 100)}%`;
}

export function getScoreColor(score: number): string {
  if (score >= 0.7) return 'var(--score-high)';
  if (score >= 0.4) return 'var(--score-medium)';
  return 'var(--score-low)';
}
