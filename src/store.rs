//! Local persistent store for Refined settings, actions, and history.

use crate::actions::{default_actions, order_actions, refresh_builtins, ActionItem};
use crate::groq::GroqConfig;
use crate::history::HistoryEntry;
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, File};
use std::io::{BufWriter, Write};
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

/// Hard cap on stored refinements; the history file must stay tiny enough
/// that saving it never stalls the UI thread.
const HISTORY_LIMIT: usize = 500;
const TRIM_INTERVAL: usize = 100;

/// Moves an unreadable store file aside so it can be recovered later.
/// Timestamped: a second corruption must not overwrite the first backup.
fn quarantine_corrupt(path: &std::path::Path) {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let mut name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    name.push_str(&format!(".corrupt-{stamp}"));
    let _ = std::fs::rename(path, path.with_file_name(name));
}

/// The user-editable global shortcut. `modifier` is one of
/// [`HOTKEY_MODIFIERS`], `key` one of [`HOTKEY_KEY_CHOICES`]. Stored as
/// plain strings so settings.json stays hand-readable and old files
/// forward-upgrade via `#[serde(default)]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub modifier: String,
    pub key: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            modifier: "Ctrl+Shift".to_string(),
            key: "X".to_string(),
        }
    }
}

// No Ctrl+Alt combos here: on German keyboards Ctrl+Alt is AltGr, and a
// hotkey on it breaks every special character the user types.
pub const HOTKEY_MODIFIERS: &[&str] = &["Ctrl+Shift", "Alt", "Alt+Shift"];

pub const HOTKEY_KEY_CHOICES: &[&str] = &[
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", //
    "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z", //
    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", //
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub groq: GroqConfig,
    pub actions: Vec<ActionItem>,
    pub selected_language: String,
    pub custom_language: String,
    pub theme_mode: ThemeMode,
    #[serde(default)]
    pub hotkey: HotkeyConfig,
    #[serde(default)]
    hotkey_default_migrated: bool,
}

impl AppSettings {
    fn migrate_hotkey_default(&mut self) {
        if !self.hotkey_default_migrated {
            if self.hotkey.modifier == "Alt" && self.hotkey.key.eq_ignore_ascii_case("V") {
                self.hotkey = HotkeyConfig::default();
            }
            self.hotkey_default_migrated = true;
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            groq: GroqConfig::default(),
            actions: default_actions(),
            selected_language: "auto".to_string(),
            custom_language: String::new(),
            theme_mode: ThemeMode::Dark,
            hotkey: HotkeyConfig::default(),
            hotkey_default_migrated: true,
        }
    }
}

pub struct Store {
    pub settings: AppSettings,
    pub history: Vec<HistoryEntry>,
    data_dir: PathBuf,
    history_loaded: bool,
    appends_since_load: usize,
}

impl Store {
    pub fn open() -> Self {
        let data_dir = data_dir();
        let _ = create_dir_all(&data_dir);

        let settings_file = data_dir.join("settings.json");
        let mut settings = if settings_file.exists() {
            match read_json_bom_tolerant::<AppSettings>(&settings_file) {
                Some(s) => s,
                // A silent default would wipe the stored API key and every
                // customization without a trace; say it happened.
                None => {
                    eprintln!(
                        "[refined] settings.json unreadable, using defaults (backup will be created)"
                    );
                    quarantine_corrupt(&settings_file);
                    AppSettings::default()
                }
            }
        } else {
            AppSettings::default()
        };

        settings.migrate_hotkey_default();
        settings.groq.migrate_reasoning_default();

        // Earlier releases always used this fixed output limit.
        if !settings.groq.output_limit_edited && settings.groq.max_completion_tokens == 32768 {
            settings.groq.max_completion_tokens = 0;
        }

        if settings.groq.global_system_prompt == crate::groq::PREVIOUS_SYSTEM_PROMPT {
            settings.groq.global_system_prompt = crate::groq::STUDY_SYSTEM_PROMPT.into();
        }
        refresh_builtins(&mut settings.actions);

        Self { settings, history: Vec::new(), data_dir, history_loaded: false, appends_since_load: 0 }
    }

    fn history_path(&self) -> PathBuf {
        self.data_dir.join("history.jsonl")
    }

    pub fn load_history(&mut self) {
        if self.history_loaded {
            return;
        }
        self.history_loaded = true;
        let path = self.history_path();
        match crate::history::load(&path, HISTORY_LIMIT) {
            Ok((history, lossy)) => {
                self.history = history;
                if lossy {
                    self.save_history();
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // Releases before 0.4.0 kept one pretty-printed JSON array.
                let legacy = self.data_dir.join("history.json");
                if !legacy.exists() {
                    return;
                }
                match read_json_bom_tolerant::<Vec<HistoryEntry>>(&legacy) {
                    Some(mut history) => {
                        history.truncate(HISTORY_LIMIT);
                        self.history = history;
                        if crate::history::rewrite(&path, &self.history).is_ok() {
                            let _ = std::fs::remove_file(legacy);
                        }
                    }
                    None => {
                        eprintln!("[refined] history.json unreadable, starting with an empty history");
                        quarantine_corrupt(&legacy);
                    }
                }
            }
            Err(error) => eprintln!("[refined] could not read history: {error}"),
        }
    }

    fn write_json<T: serde::Serialize>(&self, file_name: &str, value: &T) -> Result<(), String> {
        // Atomic replace: serialize into a temp file next to the target,
        // flush it, and only then rename over the real file. A crash or
        // disk-full mid-write can no longer truncate settings (including
        // the API key) or the history the next start would then wipe via
        // `unwrap_or_default()`.
        let target = self.data_dir.join(file_name);
        let tmp = self.data_dir.join(format!("{file_name}.tmp"));
        let file = File::create(&tmp).map_err(|e| e.to_string())?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, value).map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
        drop(writer);
        std::fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn save_settings(&self) {
        if let Err(e) = self.write_json("settings.json", &self.settings) {
            // Disk-full or permission failures must not disappear silently
            // while the UI implies everything was saved.
            eprintln!("[refined] could not save settings: {e}");
        }
    }

    pub fn save_history(&self) {
        if let Err(e) = crate::history::rewrite(&self.history_path(), &self.history) {
            eprintln!("[refined] could not save history: {e}");
        }
    }

    /// Appends without loading earlier edits. Every [`TRIM_INTERVAL`]
    /// appends, the file is read once and trimmed to [`HISTORY_LIMIT`].
    pub fn add_history(&mut self, entry: HistoryEntry) {
        if !self.history_loaded
            && self.appends_since_load < TRIM_INTERVAL
            && !self.data_dir.join("history.json").exists()
        {
            self.appends_since_load += 1;
            if let Err(e) = crate::history::append(&self.history_path(), &entry) {
                eprintln!("[refined] could not save history: {e}");
            }
            return;
        }
        self.load_history();
        if let Err(e) = crate::history::append(&self.history_path(), &entry) {
            eprintln!("[refined] could not save history: {e}");
        }
        self.history.insert(0, entry);
        if self.history.len() > HISTORY_LIMIT {
            self.history.truncate(HISTORY_LIMIT);
            self.save_history();
        }
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
        order_actions(&mut self.settings.actions);
        self.save_settings();
    }

    pub fn delete_custom_action(&mut self, id: &str) {
        self.settings.actions.retain(|a| a.id != id);
        order_actions(&mut self.settings.actions);
        self.save_settings();
    }

    /// Action order is user-defined (Studio), so the number shortcuts are
    /// positional: row 1 answers to key 1, whatever the action is.
    pub fn move_action(&mut self, from: usize, to: usize) {
        let actions = &mut self.settings.actions;
        if from == to || from >= actions.len() || to >= actions.len()
            || actions[from].is_pinned() || actions[to].is_pinned()
        {
            return;
        }
        actions.swap(from, to);
        order_actions(actions);
        self.save_settings();
    }

    pub fn reset_action_to_default(&mut self, id: &str) {
        if let Some(action) = self.settings.actions.iter_mut().find(|a| a.id == id) {
            action.reset_to_default();
        }
        self.save_settings();
    }
}

/// `%LOCALAPPDATA%\\Refined` on Windows, `$XDG_DATA_HOME/refined` or
/// `~/.local/share/refined` on Linux.
pub fn data_dir() -> PathBuf {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("Refined"));
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .map(|dir| dir.join("refined"));
    base.unwrap_or_else(|| PathBuf::from("Refined_Data"))
}

/// Reads a JSON file, tolerating a UTF-8 BOM prefix (serde_json rejects BOMs,
/// and several Windows editors/scripts write them) so user data is never
/// silently discarded over a 3-byte prefix.
fn read_json_bom_tolerant<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    let trimmed = text.trim_start_matches('\u{feff}');
    serde_json::from_str(trimmed).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_only_the_legacy_default_once() {
        let legacy = |key: &str| {
            let mut value = serde_json::to_value(AppSettings::default()).unwrap();
            value.as_object_mut().unwrap().remove("hotkey_default_migrated");
            value["hotkey"] = serde_json::json!({"modifier": "Alt", "key": key});
            serde_json::from_value::<AppSettings>(value).unwrap()
        };
        let mut settings = legacy("V");
        settings.migrate_hotkey_default();
        assert_eq!(settings.hotkey, HotkeyConfig::default());

        settings.hotkey = legacy("V").hotkey;
        settings.migrate_hotkey_default();
        assert_eq!(settings.hotkey.key, "V");

        let mut custom = legacy("Q");
        custom.migrate_hotkey_default();
        assert_eq!(custom.hotkey.modifier, "Alt");
        assert_eq!(custom.hotkey.key, "Q");
    }

    #[test]
    fn update_restart_requires_a_closed_palette_and_a_fresh_click() {
        let mut store = Store {
            settings: AppSettings::default(),
            history: Vec::new(),
            data_dir: PathBuf::new(),
            history_loaded: false,
            appends_since_load: 0,
        };
        let mut studio = crate::studio::Studio::new();
        studio.update_state = Some(crate::updater::UpdateState::Ready {
            version: "0.3.2".into(),
            checksum: String::new(),
        });
        studio.pending_restart = true;
        assert!(!studio.restart_for_update(&mut store, true));
        assert!(!studio.restart_for_update(&mut store, false));
        assert!(matches!(studio.update_state, Some(crate::updater::UpdateState::Ready { .. })));
    }
}
