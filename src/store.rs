//! Local persistent store for Refined settings, actions, and history.

use crate::actions::{default_actions, ActionItem};
use crate::groq::GroqConfig;
use crate::history::HistoryEntry;
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, File};
use std::io::BufWriter;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

impl Default for ThemeMode {
    fn default() -> Self {
        Self::Dark
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub groq: GroqConfig,
    pub actions: Vec<ActionItem>,
    pub selected_language: String,
    pub custom_language: String,
    pub theme_mode: ThemeMode,
    pub history_limit: usize,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            groq: GroqConfig::default(),
            actions: default_actions(),
            selected_language: "auto".to_string(),
            custom_language: String::new(),
            theme_mode: ThemeMode::Dark,
            history_limit: 500,
        }
    }
}

pub struct Store {
    pub settings: AppSettings,
    pub history: Vec<HistoryEntry>,
    data_dir: PathBuf,
}


impl Store {
    pub fn open() -> Self {
        let data_dir = dirs_fallback_app_data();
        let _ = create_dir_all(&data_dir);

        let settings_file = data_dir.join("settings.json");
        let settings = if settings_file.exists() {
            read_json_bom_tolerant(&settings_file).unwrap_or_default()
        } else {
            AppSettings::default()
        };

        let history_file = data_dir.join("history.json");
        let history = if history_file.exists() {
            read_json_bom_tolerant(&history_file).unwrap_or_default()
        } else {
            Vec::new()
        };

        Self {
            settings,
            history,
            data_dir,
        }
    }

    fn write_json<T: serde::Serialize>(&self, file_name: &str, value: &T) -> Result<(), String> {
        let file = File::create(self.data_dir.join(file_name)).map_err(|e| e.to_string())?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, value).map_err(|e| e.to_string())
    }

    pub fn save_settings(&self) {
        if let Err(e) = self.write_json("settings.json", &self.settings) {
            // Disk-full or permission failures must not disappear silently
            // while the UI implies everything was saved.
            eprintln!("[refined] could not save settings: {e}");
        }
    }

    pub fn save_history(&self) {
        if let Err(e) = self.write_json("history.json", &self.history) {
            eprintln!("[refined] could not save history: {e}");
        }
    }

    pub fn add_history(&mut self, entry: HistoryEntry) {
        self.history.insert(0, entry);
        if self.history.len() > self.settings.history_limit {
            self.history.truncate(self.settings.history_limit);
        }
        self.save_history();
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        self.save_history();
    }

    pub fn delete_history_entry(&mut self, id: &str) {
        self.history.retain(|e| e.id != id);
        self.save_history();
    }

    pub fn reset_all_actions(&mut self) {
        self.settings.actions = default_actions();
        self.save_settings();
    }

    pub fn add_custom_action(&mut self, action: ActionItem) {
        self.settings.actions.push(action);
        self.renumber_action_keys();
        self.save_settings();
    }

    pub fn delete_custom_action(&mut self, id: &str) {
        self.settings.actions.retain(|a| a.id != id);
        self.renumber_action_keys();
        self.save_settings();
    }

    /// Action order is user-defined (Studio), so the number shortcuts are
    /// positional: row 1 answers to key 1, whatever the action is.
    pub fn move_action(&mut self, from: usize, to: usize) {
        if from == to || from >= self.settings.actions.len() || to >= self.settings.actions.len()
        {
            return;
        }
        self.settings.actions.swap(from, to);
        self.renumber_action_keys();
        self.save_settings();
    }

    fn renumber_action_keys(&mut self) {
        for (i, action) in self.settings.actions.iter_mut().enumerate() {
            action.shortcut = if i < 9 {
                (i + 1).to_string()
            } else {
                String::new()
            };
        }
    }

    pub fn reset_action_to_default(&mut self, id: &str) {
        if let Some(action) = self.settings.actions.iter_mut().find(|a| a.id == id) {
            action.reset_to_default();
        }
        self.save_settings();
    }
}

fn dirs_fallback_app_data() -> PathBuf {
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        PathBuf::from(local_app_data).join("Refined")
    } else {
        PathBuf::from("Refined_Data")
    }
}

/// Reads a JSON file, tolerating a UTF-8 BOM prefix (serde_json rejects BOMs,
/// and several Windows editors/scripts write them) so user data is never
/// silently discarded over a 3-byte prefix.
fn read_json_bom_tolerant<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    let trimmed = text.trim_start_matches('\u{feff}');
    serde_json::from_str(trimmed).ok()
}
