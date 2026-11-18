//! Statistics aggregation and insights for Refined study sessions.

use crate::history::HistoryEntry;
use chrono::{Datelike, Local};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct AggregatedStats {
    pub total_refinements: usize,
    pub total_original_words: usize,
    pub total_refined_words: usize,
    #[allow(dead_code)]
pub net_word_change: isize,
    pub estimated_minutes_saved: f32,
    pub action_counts: HashMap<String, usize>,
    #[allow(dead_code)]
pub model_counts: HashMap<String, usize>,
    pub average_latency_ms: u128,
    pub daily_counts: Vec<(String, usize)>, // last 7 days (e.g. "Mon", count)
}

pub fn calculate_stats(history: &[HistoryEntry]) -> AggregatedStats {
    if history.is_empty() {
        return AggregatedStats::default();
    }

    let mut total_orig_words = 0;
    let mut total_ref_words = 0;
    let mut action_counts: HashMap<String, usize> = HashMap::new();
    let mut model_counts: HashMap<String, usize> = HashMap::new();
    let mut total_duration = 0u128;

    for entry in history {
        total_orig_words += entry.stats.original_words;
        total_ref_words += entry.stats.refined_words;
        *action_counts.entry(entry.action_name.clone()).or_insert(0) += 1;
        *model_counts.entry(entry.model.clone()).or_insert(0) += 1;
        total_duration += entry.duration_ms;
    }

    let net_word_change = total_ref_words as isize - total_orig_words as isize;
    // Human rewriting/polishing average: ~30 words per minute
    let estimated_minutes_saved = (total_orig_words as f32 / 30.0).max(0.1);
    let avg_latency = total_duration / history.len() as u128;

    // Daily counts for the last 7 days
    let today = Local::now().date_naive();
    let mut daily_counts = Vec::new();
    for day_offset in (0..7).rev() {
        let date = today - chrono::Duration::days(day_offset);
        let count = history
            .iter()
            .filter(|e| e.timestamp.date_naive() == date)
            .count();
        let label = format!("{:02}.{:02}", date.day(), date.month());
        daily_counts.push((label, count));
    }

    AggregatedStats {
        total_refinements: history.len(),
        total_original_words: total_orig_words,
        total_refined_words: total_ref_words,
        net_word_change,
        estimated_minutes_saved,
        action_counts,
        model_counts,
        average_latency_ms: avg_latency,
        daily_counts,
    }
}

