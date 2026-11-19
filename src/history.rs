//! Refinement history records and persistence models.

use crate::diff::{compute_diff, DiffChunk, DiffKind, DiffStats};
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    inserted_words: Option<usize>,
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
        diff_chunks: &[DiffChunk],
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
            inserted_words: Some(count_inserted_words(diff_chunks)),
            stats,
            model,
            duration_ms,
            language,
        }
    }

    pub fn inserted_words(&mut self) -> usize {
        *self.inserted_words.get_or_insert_with(|| {
            let (chunks, _) = compute_diff(&self.original_text, &self.refined_text);
            count_inserted_words(&chunks)
        })
    }
}

fn count_inserted_words(chunks: &[DiffChunk]) -> usize {
    chunks
        .iter()
        .filter(|chunk| chunk.kind == DiffKind::Insert)
        .map(|chunk| chunk.text.split_whitespace().count())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_history_without_inserted_word_count_loads_and_migrates() {
        let (chunks, stats) = compute_diff("Water flows", "Cold water flows");
        let entry = HistoryEntry::new(
            "polish".into(),
            "Polish".into(),
            "Water flows".into(),
            "Cold water flows".into(),
            &chunks,
            stats,
            "model".into(),
            15,
            "auto".into(),
        );
        let mut old = serde_json::to_value(&entry).unwrap();
        let expected = old
            .as_object_mut()
            .unwrap()
            .remove("inserted_words")
            .unwrap();
        let mut decoded: HistoryEntry = serde_json::from_value(old).unwrap();
        assert_eq!(serde_json::json!(decoded.inserted_words()), expected);
        assert_eq!(
            serde_json::to_value(decoded).unwrap()["inserted_words"],
            expected
        );
    }
}
