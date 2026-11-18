//! Refinement history records and persistence models.

use crate::diff::{DiffChunk, DiffStats};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub timestamp: DateTime<Local>,
    pub action_id: String,
    pub action_name: String,
    pub original_text: String,
    pub refined_text: String,
    pub diff_chunks: Vec<DiffChunk>,
    pub stats: DiffStats,
    pub model: String,
    pub duration_ms: u128,
    pub language: String,
}

impl HistoryEntry {
    pub fn new(
        action_id: String,
        action_name: String,
        original_text: String,
        refined_text: String,
        diff_chunks: Vec<DiffChunk>,
        stats: DiffStats,
        model: String,
        duration_ms: u128,
        language: String,
    ) -> Self {
        let now = Local::now();
        let id = format!("{}_{}", now.timestamp_millis(), action_id);
        Self {
            id,
            timestamp: now,
            action_id,
            action_name,
            original_text,
            refined_text,
            diff_chunks,
            stats,
            model,
            duration_ms,
            language,
        }
    }
}
