//! Refinement history records and persistence models.

use crate::diff::{compute_diff, DiffChunk, DiffKind, DiffStats};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

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

    /// The diff as the HUD shows it, with sub- and superscript tags rendered.
    pub fn diff(&self) -> (Vec<DiffChunk>, DiffStats) {
        compute_diff(
            &crate::rich_text::readable(&self.original_text),
            &crate::rich_text::readable(&self.refined_text),
        )
    }

    pub fn inserted_words(&mut self) -> usize {
        if self.inserted_words.is_none() {
            self.inserted_words = Some(count_inserted_words(&self.diff().0));
        }
        self.inserted_words.unwrap_or_default()
    }
}

/// History lives in a JSON Lines file, oldest entry first. Saving an edit
/// appends one compact line, so it never reads or rewrites earlier edits.
pub fn append(path: &Path, entry: &HistoryEntry) -> std::io::Result<()> {
    let mut file = OpenOptions::new().create(true).read(true).append(true).open(path)?;
    // A line cut short by a crash must not swallow the new entry.
    let mut line = Vec::new();
    if file.seek(SeekFrom::End(0))? > 0 {
        file.seek(SeekFrom::End(-1))?;
        let mut last = [0u8];
        file.read_exact(&mut last)?;
        if last[0] != b'\n' {
            line.push(b'\n');
        }
    }
    serde_json::to_writer(&mut line, entry)?;
    line.push(b'\n');
    file.write_all(&line)
}

/// Reads the history newest first. Unreadable lines, such as a line cut
/// short by a crash, are skipped. Returns whether lines were skipped or
/// dropped beyond `limit`, so the caller can rewrite the file.
pub fn load(path: &Path, limit: usize) -> std::io::Result<(Vec<HistoryEntry>, bool)> {
    let text = std::fs::read_to_string(path)?;
    let lines = text.lines().filter(|line| !line.trim().is_empty());
    let mut entries: Vec<HistoryEntry> = Vec::new();
    let mut lossy = false;
    for line in lines {
        match serde_json::from_str(line.trim_start_matches('\u{feff}')) {
            Ok(entry) => entries.push(entry),
            Err(_) => lossy = true,
        }
    }
    entries.reverse();
    if entries.len() > limit {
        entries.truncate(limit);
        lossy = true;
    }
    Ok((entries, lossy))
}

/// Replaces the file with `entries` (newest first) through a temporary file.
pub fn rewrite(path: &Path, entries: &[HistoryEntry]) -> std::io::Result<()> {
    let tmp = path.with_extension("jsonl.tmp");
    let mut writer = BufWriter::new(File::create(&tmp)?);
    for entry in entries.iter().rev() {
        serde_json::to_writer(&mut writer, entry)?;
        writer.write_all(b"\n")?;
    }
    writer.into_inner().map_err(|error| error.into_error())?.sync_all()?;
    std::fs::rename(tmp, path)
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

    fn entry(id: &str) -> HistoryEntry {
        let (chunks, stats) = compute_diff("a", "b");
        let mut entry = HistoryEntry::new(
            "polish".into(), "Polish".into(), "a".into(), "b".into(),
            &chunks, stats, "model".into(), 1, "auto".into(),
        );
        entry.id = id.into();
        entry
    }

    #[test]
    fn appends_loads_newest_first_and_recovers_from_a_cut_line() {
        let dir = std::env::temp_dir().join(format!("refined-history-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.jsonl");
        let _ = std::fs::remove_file(&path);
        for id in ["1", "2", "3"] {
            append(&path, &entry(id)).unwrap();
        }
        let ids = |entries: &[HistoryEntry]| entries.iter().map(|e| e.id.clone()).collect::<Vec<_>>();
        let (entries, lossy) = load(&path, 10).unwrap();
        assert_eq!((ids(&entries), lossy), (vec!["3".into(), "2".into(), "1".into()], false));

        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"id\":\"4\",\"tim").unwrap();
        drop(file);
        append(&path, &entry("4")).unwrap();
        let (entries, lossy) = load(&path, 3).unwrap();
        assert_eq!((ids(&entries), lossy), (vec!["4".into(), "3".into(), "2".into()], true));

        rewrite(&path, &entries).unwrap();
        append(&path, &entry("5")).unwrap();
        let (entries, lossy) = load(&path, 10).unwrap();
        assert_eq!(
            (ids(&entries), lossy),
            (vec!["5".into(), "4".into(), "3".into(), "2".into()], false)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

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
