//! Local persistent store for Refined settings, actions, and history.

use crate::actions::{default_actions, ActionItem};
use crate::groq::GroqConfig;
use crate::history::HistoryEntry;
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, File};
use std::io::{BufReader, BufWriter};
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
    pub autostart: bool,
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
            autostart: false,
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
            File::open(&settings_file)
                .ok()
                .and_then(|f| serde_json::from_reader(BufReader::new(f)).ok())
                .unwrap_or_default()
        } else {
            AppSettings::default()
        };

        let history_file = data_dir.join("history.json");
        let history = if history_file.exists() {
            File::open(&history_file)
                .ok()
                .and_then(|f| serde_json::from_reader(BufReader::new(f)).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        Self {
            settings,
            history,
            data_dir,
        }
    }

    pub fn save_settings(&self) {
        let file_path = self.data_dir.join("settings.json");
        if let Ok(file) = File::create(file_path) {
            let writer = BufWriter::new(file);
            let _ = serde_json::to_writer_pretty(writer, &self.settings);
        }
    }

    pub fn save_history(&self) {
        let file_path = self.data_dir.join("history.json");
        if let Ok(file) = File::create(file_path) {
            let writer = BufWriter::new(file);
            let _ = serde_json::to_writer_pretty(writer, &self.history);
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
}

fn dirs_fallback_app_data() -> PathBuf {
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        PathBuf::from(local_app_data).join("Refined")
    } else {
        PathBuf::from("Refined_Data")
    }
}
