//! Refined Studio: History, Statistics, Action Customization, and Groq Settings.

use crate::actions::{default_actions, ActionItem};
use crate::autostart;
use crate::groq::{available_reasoning_efforts, fetch_chat_models, GroqConfig, GroqModel};
use crate::history::HistoryEntry;
use crate::stats::calculate_stats;
use crate::store::{Store, ThemeMode};
use crate::theme::{render_diff_view, spectrum_badge, spectrum_button, SpectrumTheme};
use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Layout, RichText, ScrollArea, Stroke, Ui, Vec2,
};
use std::sync::mpsc::{channel, Receiver, Sender};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudioTab {
    History,
    Statistics,
    Actions,
    Settings,
}

pub struct Studio {
    pub current_tab: StudioTab,
    pub history_search: String,
    pub history_action_filter: String,
    pub expanded_history_id: Option<String>,
    pub fetched_models: Vec<GroqModel>,
    pub is_fetching_models: bool,
    pub models_status_msg: Option<(String, bool)>,
    pub models_rx: Option<Receiver<Result<Vec<GroqModel>, String>>>,
    pub models_tx: Sender<Result<Vec<GroqModel>, String>>,
    pub show_api_key: bool,
    pub new_action_dialog_open: bool,
    pub new_action_name: String,
    pub new_action_desc: String,
    pub new_action_icon: String,
    pub new_action_shortcut: String,
    pub new_action_prompt: String,
}

impl Studio {
    pub fn new() -> Self {
        let (models_tx, models_rx) = channel();
        Self {
            current_tab: StudioTab::History,
            history_search: String::new(),
            history_action_filter: "All".to_string(),
            expanded_history_id: None,
            fetched_models: Vec::new(),
            is_fetching_models: false,
            models_status_msg: None,
            models_rx: Some(models_rx),
            models_tx,
            show_api_key: false,
            new_action_dialog_open: false,
            new_action_name: String::new(),
            new_action_desc: String::new(),
            new_action_icon: "✦".to_string(),
            new_action_shortcut: String::new(),
            new_action_prompt: String::new(),
        }
    }

    pub fn fetch_models(&mut self, api_key: &str) {
        if api_key.trim().is_empty() {
            self.models_status_msg = Some(("Please enter your Groq API key first.".to_string(), false));
            return;
        }

        self.is_fetching_models = true;
        self.models_status_msg = Some(("Connecting to Groq API and retrieving chat models...".to_string(), true));
        let tx = self.models_tx.clone();
        let key = api_key.to_string();

        std::thread::spawn(move || {
            let res = fetch_chat_models(&key);
            let _ = tx.send(res);
        });
    }

    pub fn update(&mut self, ui: &mut Ui, store: &mut Store, theme: &SpectrumTheme) {
        if let Some(ref rx) = self.models_rx {
            if let Ok(res) = rx.try_recv() {
                self.is_fetching_models = false;
                match res {
                    Ok(models) => {
                        let count = models.len();
                        self.fetched_models = models;
                        self.models_status_msg = Some((
                            format!("Successfully fetched {count} chat models from Groq!"),
                            true,
                        ));
                    }
                    Err(err) => {
                        self.models_status_msg = Some((err, false));
                    }
                }
            }
        }

        egui::Frame::new()
            .fill(theme.surface)
            .stroke(Stroke::new(1.0, theme.border))
            .inner_margin(egui::Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("✦ REFINED STUDIO")
                            .font(FontId::proportional(15.0))
                            .color(theme.accent)
                            .strong(),
                    );
                    ui.add_space(20.0);

                    let tabs = [
                        (StudioTab::History, "History"),
                        (StudioTab::Statistics, "Statistics"),
                        (StudioTab::Actions, "Actions"),
                        (StudioTab::Settings, "Settings & Groq"),
                    ];

                    for (tab, label) in tabs {
                        let is_active = self.current_tab == tab;
                        let text = if is_active {
                            RichText::new(label).color(theme.accent).strong()
                        } else {
                            RichText::new(label).color(theme.text_secondary)
                        };

                        if ui.selectable_label(is_active, text).clicked() {
                            self.current_tab = tab;
                        }
                    }
                });
            });

        ui.add_space(8.0);

        match self.current_tab {
            StudioTab::History => self.render_history_tab(ui, store, theme),
            StudioTab::Statistics => self.render_statistics_tab(ui, store, theme),
            StudioTab::Actions => self.render_actions_tab(ui, store, theme),
            StudioTab::Settings => self.render_settings_tab(ui, store, theme),
        }
    }

    fn render_history_tab(&mut self, ui: &mut Ui, store: &mut Store, theme: &SpectrumTheme) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Study Refinements History")
                    .font(FontId::proportional(16.0))
                    .color(theme.text_primary)
                    .strong(),
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !store.history.is_empty() {
                    if spectrum_button(ui, "Clear All History", false, theme).clicked() {
                        store.clear_history();
                    }
                }

                egui::ComboBox::from_id_salt("history_action_filter")
                    .selected_text(RichText::new(&self.history_action_filter).font(FontId::proportional(12.0)))
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(self.history_action_filter == "All", "All Actions").clicked() {
                            self.history_action_filter = "All".to_string();
                        }
                        for action in &store.settings.actions {
                            if ui.selectable_label(self.history_action_filter == action.name, &action.name).clicked() {
                                self.history_action_filter = action.name.clone();
                            }
                        }
                    });

                ui.add(
                    egui::TextEdit::singleline(&mut self.history_search)
                        .hint_text("🔍 Search text or action...")
                        .desired_width(180.0),
                );
            });
        });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        if store.history.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.label(RichText::new("✦").font(FontId::proportional(36.0)).color(theme.text_tertiary));
                ui.label(
                    RichText::new("No refinements yet")
                        .font(FontId::proportional(16.0))
                        .color(theme.text_secondary)
                        .strong(),
                );
                ui.label(
                    RichText::new("Select any text in any app, press Alt + Q, and pick an action to start studying!")
                        .font(FontId::proportional(12.0))
                        .color(theme.text_tertiary),
                );
            });
            return;
        }

        let query = self.history_search.to_lowercase();
        let filter_action = self.history_action_filter.clone();

        let filtered_entries: Vec<(usize, &HistoryEntry)> = store
            .history
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                if filter_action != "All" && entry.action_name != filter_action {
                    return false;
                }
                if !query.is_empty() {
                    let matches_text = entry.original_text.to_lowercase().contains(&query)
                        || entry.refined_text.to_lowercase().contains(&query)
                        || entry.action_name.to_lowercase().contains(&query);
                    if !matches_text {
                        return false;
                    }
                }
                true
            })
            .collect();

        ui.label(
            RichText::new(format!("Showing {} entries", filtered_entries.len()))
                .font(FontId::proportional(11.0))
                .color(theme.text_secondary),
        );
        ui.add_space(4.0);

        let mut entry_to_delete: Option<usize> = None;

        ScrollArea::vertical().show(ui, |ui| {
            for (idx, entry) in filtered_entries {
                let is_expanded = self.expanded_history_id.as_deref() == Some(&entry.id);

                egui::Frame::new()
                    .fill(theme.surface)
                    .stroke(Stroke::new(1.0, theme.border))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            spectrum_badge(ui, &entry.action_name, theme.accent_subtle, theme.accent);
                            ui.label(
                                RichText::new(entry.timestamp.format("%b %d, %H:%M").to_string())
                                    .font(FontId::proportional(11.0))
                                    .color(theme.text_secondary),
                            );
                            ui.label(
                                RichText::new(format!("{}ms", entry.duration_ms))
                                    .font(FontId::proportional(11.0))
                                    .color(theme.text_tertiary),
                            );

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.add(egui::Button::new(RichText::new("🗑").color(theme.diff_del_text)).frame(false)).clicked() {
                                    entry_to_delete = Some(idx);
                                }

                                if spectrum_button(ui, "Copy Refined", false, theme).clicked() {
                                    crate::clipboard::set_clipboard_text(&entry.refined_text);
                                }

                                if spectrum_button(ui, if is_expanded { "Collapse Diff" } else { "View Diff" }, false, theme).clicked() {
                                    if is_expanded {
                                        self.expanded_history_id = None;
                                    } else {
                                        self.expanded_history_id = Some(entry.id.clone());
                                    }
                                }

                                let delta = entry.stats.word_delta();
                                let delta_str = if delta >= 0 {
                                    format!("+{}w", delta)
                                } else {
                                    format!("{}w", delta)
                                };
                                spectrum_badge(
                                    ui,
                                    &format!("{} â†’ {} words ({})", entry.stats.original_words, entry.stats.refined_words, delta_str),
                                    theme.surface_elevated,
                                    theme.text_secondary,
                                );
                            });
                        });

                        ui.add_space(6.0);

                        if !is_expanded {
                            let preview: String = entry.refined_text.chars().take(220).collect();
                            let preview_display = if entry.refined_text.chars().count() > 220 {
                                format!("{preview}...")
                            } else {
                                preview
                            };
                            ui.label(RichText::new(preview_display).color(theme.text_primary).font(FontId::proportional(13.0)));
                        } else {
                            ui.label(RichText::new("Before & After Diff:").font(FontId::proportional(12.0)).color(theme.text_secondary).strong());
                            ui.add_space(4.0);
                            egui::Frame::new()
                                .fill(theme.surface_elevated)
                                .corner_radius(CornerRadius::same(6))
                                .inner_margin(egui::Margin::same(10))
                                .show(ui, |ui| {
                                    render_diff_view(ui, &entry.diff_chunks, theme);
                                });
                        }
                    });
                ui.add_space(6.0);
            }
        });

        if let Some(idx) = entry_to_delete {
            store.history.remove(idx);
            store.save_history();
        }
    }

    fn render_statistics_tab(&mut self, ui: &mut Ui, store: &Store, theme: &SpectrumTheme) {
        let stats = calculate_stats(&store.history);

        ui.label(
            RichText::new("Study Session Insights & Metrics")
                .font(FontId::proportional(16.0))
                .color(theme.text_primary)
                .strong(),
        );
        ui.label(
            RichText::new("Quantifying your productivity, editing velocity, and study patterns.")
                .font(FontId::proportional(11.0))
                .color(theme.text_secondary),
        );

        ui.add_space(12.0);

        ui.horizontal(|ui| {
            render_metric_card(
                ui,
                "Words Refined",
                &stats.total_refined_words.to_string(),
                &format!("from {} original words", stats.total_original_words),
                theme.accent,
                theme,
            );
            render_metric_card(
                ui,
                "Refinements Made",
                &stats.total_refinements.to_string(),
                "total AI operations",
                theme.purple,
                theme,
            );
            render_metric_card(
                ui,
                "Time Saved",
                &format!("{:.1} min", stats.estimated_minutes_saved),
                "editing & drafting effort",
                theme.diff_add_text,
                theme,
            );
            render_metric_card(
                ui,
                "Avg Response Time",
                &format!("{} ms", stats.average_latency_ms),
                "Groq inference speed",
                theme.amber,
                theme,
            );
        });

        ui.add_space(16.0);

        egui::Frame::new()
            .fill(theme.surface)
            .stroke(Stroke::new(1.0, theme.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("Action Distribution")
                        .font(FontId::proportional(14.0))
                        .color(theme.text_primary)
                        .strong(),
                );
                ui.add_space(8.0);

                if stats.action_counts.is_empty() {
                    ui.label(RichText::new("No actions recorded yet.").color(theme.text_secondary));
                } else {
                    let total = stats.total_refinements.max(1) as f32;
                    for (action_name, count) in &stats.action_counts {
                        let pct = (*count as f32 / total) * 100.0;
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(action_name).strong().color(theme.text_primary));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new(format!("{count} ({pct:.1}%)")).color(theme.text_secondary));
                            });
                        });

                        ui.add(
                            egui::ProgressBar::new(pct / 100.0)
                                .fill(theme.accent)
                                .corner_radius(CornerRadius::same(4)),
                        );
                        ui.add_space(4.0);
                    }
                }
            });

        ui.add_space(12.0);

        egui::Frame::new()
            .fill(theme.surface)
            .stroke(Stroke::new(1.0, theme.border))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("Activity (Last 7 Days)")
                        .font(FontId::proportional(14.0))
                        .color(theme.text_primary)
                        .strong(),
                );
                ui.add_space(8.0);

                let max_count = stats.daily_counts.iter().map(|(_, c)| *c).max().unwrap_or(1).max(1) as f32;

                ui.horizontal(|ui| {
                    for (day, count) in &stats.daily_counts {
                        ui.vertical_centered(|ui| {
                            let ratio = (*count as f32 / max_count).clamp(0.05, 1.0);
                            let bar_height = 80.0 * ratio;

                            ui.allocate_space(Vec2::new(36.0, 80.0 - bar_height));
                            egui::Frame::new()
                                .fill(if *count > 0 { theme.accent } else { theme.surface_elevated })
                                .corner_radius(CornerRadius::same(4))
                                .show(ui, |ui| {
                                    ui.allocate_space(Vec2::new(36.0, bar_height));
                                });

                            ui.add_space(4.0);
                            ui.label(RichText::new(day).font(FontId::proportional(10.0)).color(theme.text_secondary));
                            ui.label(RichText::new(count.to_string()).font(FontId::proportional(11.0)).strong().color(theme.text_primary));
                        });
                        ui.add_space(10.0);
                    }
                });
            });
    }

    fn render_actions_tab(&mut self, ui: &mut Ui, store: &mut Store, theme: &SpectrumTheme) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Action Palette Customization")
                    .font(FontId::proportional(16.0))
                    .color(theme.text_primary)
                    .strong(),
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if spectrum_button(ui, "+ Add Custom Action", true, theme).clicked() {
                    self.new_action_dialog_open = true;
                    self.new_action_name = "Summarize".to_string();
                    self.new_action_desc = "Condense into 3 key bullet points".to_string();
                    self.new_action_icon = "✦".to_string();
                    self.new_action_shortcut = "7".to_string();
                    self.new_action_prompt = "Summarize the text into exactly 3 concise, impactful bullet points. Preserve the core insights.".to_string();
                }

                if spectrum_button(ui, "Reset All to Defaults", false, theme).clicked() {
                    store.settings.actions = default_actions();
                    store.save_settings();
                }
            });
        });

        ui.add_space(4.0);
        ui.label(
            RichText::new("Customize prompt instructions for built-in actions, or configure your own personalized actions.")
                .font(FontId::proportional(11.0))
                .color(theme.text_secondary),
        );

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        if self.new_action_dialog_open {
            egui::Frame::new()
                .fill(theme.surface_elevated)
                .stroke(Stroke::new(1.0, theme.accent))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.label(RichText::new("Create New Custom Action").font(FontId::proportional(14.0)).color(theme.accent).strong());
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label("Name:");
                            ui.text_edit_singleline(&mut self.new_action_name);
                        });
                        ui.vertical(|ui| {
                            ui.label("Icon:");
                            ui.text_edit_singleline(&mut self.new_action_icon);
                        });
                        ui.vertical(|ui| {
                            ui.label("Shortcut (e.g. 7):");
                            ui.text_edit_singleline(&mut self.new_action_shortcut);
                        });
                    });

                    ui.add_space(4.0);
                    ui.label("Description:");
                    ui.text_edit_singleline(&mut self.new_action_desc);

                    ui.add_space(4.0);
                    ui.label("Prompt Instruction:");
                    ui.add(egui::TextEdit::multiline(&mut self.new_action_prompt).desired_rows(3).desired_width(f32::INFINITY));

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if spectrum_button(ui, "Save Action", true, theme).clicked() {
                            let id = format!("custom_{}", chrono::Local::now().timestamp_millis());
                            store.settings.actions.push(ActionItem {
                                id,
                                name: self.new_action_name.clone(),
                                description: self.new_action_desc.clone(),
                                icon: self.new_action_icon.clone(),
                                shortcut: self.new_action_shortcut.clone(),
                                prompt_instruction: self.new_action_prompt.clone(),
                                default_instruction: self.new_action_prompt.clone(),
                                is_builtin: false,
                                is_rewrite_as: false,
                            });
                            store.save_settings();
                            self.new_action_dialog_open = false;
                        }
                        if spectrum_button(ui, "Cancel", false, theme).clicked() {
                            self.new_action_dialog_open = false;
                        }
                    });
                });
            ui.add_space(12.0);
        }

        let mut action_to_remove: Option<usize> = None;
        let actions_changed = std::cell::Cell::new(false);

        ScrollArea::vertical().show(ui, |ui| {
            for (idx, action) in store.settings.actions.iter_mut().enumerate() {
                egui::Frame::new()
                    .fill(theme.surface)
                    .stroke(Stroke::new(1.0, theme.border))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(egui::Margin::same(14))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&action.icon).font(FontId::proportional(16.0)).color(theme.accent));
                            ui.label(RichText::new(&action.name).font(FontId::proportional(14.0)).color(theme.text_primary).strong());

                            if action.is_modified() {
                                spectrum_badge(ui, "Customized", theme.amber, Color32::BLACK);
                            }
                            if !action.is_builtin {
                                spectrum_badge(ui, "Custom", theme.purple, Color32::WHITE);
                            }

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if !action.is_builtin {
                                    if ui.add(egui::Button::new(RichText::new("🗑 Delete").color(theme.diff_del_text)).frame(false)).clicked() {
                                        action_to_remove = Some(idx);
                                    }
                                }

                                if action.is_modified() {
                                    if spectrum_button(ui, "Reset to Default", false, theme).clicked() {
                                        action.reset_to_default();
                                    }
                                }

                                spectrum_badge(ui, &format!("Key: {}", action.shortcut), theme.surface_elevated, theme.text_secondary);
                            });
                        });

                        ui.add_space(4.0);
                        ui.label(RichText::new(&action.description).font(FontId::proportional(11.0)).color(theme.text_secondary));

                        ui.add_space(6.0);
                        ui.label(RichText::new("System Instruction / Prompt:").font(FontId::proportional(11.0)).color(theme.text_tertiary));
                        let changed = ui.add(
                            egui::TextEdit::multiline(&mut action.prompt_instruction)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY),
                        ).changed();

                        if changed { actions_changed.set(true); }
                    });
                ui.add_space(8.0);
            }
        });

        if let Some(idx) = action_to_remove {
            store.settings.actions.remove(idx);
            actions_changed.set(true);
        }
        if actions_changed.get() {
            store.save_settings();
        }
    }

    fn render_settings_tab(&mut self, ui: &mut Ui, store: &mut Store, theme: &SpectrumTheme) {
        ScrollArea::vertical().show(ui, |ui| {
            ui.label(
                RichText::new("AI Settings (Groq API)")
                    .font(FontId::proportional(16.0))
                    .color(theme.text_primary)
                    .strong(),
            );
            ui.label(
                RichText::new("Configure your Groq API credentials, chat models, and reasoning parameters.")
                    .font(FontId::proportional(11.0))
                    .color(theme.text_secondary),
            );

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            egui::Frame::new()
                .fill(theme.surface)
                .stroke(Stroke::new(1.0, theme.border))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.label(RichText::new("Groq API Key").font(FontId::proportional(13.0)).strong());
                    ui.label(RichText::new("Get your free key from https://console.groq.com/keys").font(FontId::proportional(11.0)).color(theme.text_secondary));
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        let edit = if self.show_api_key {
                            egui::TextEdit::singleline(&mut store.settings.groq.api_key)
                        } else {
                            egui::TextEdit::singleline(&mut store.settings.groq.api_key).password(true)
                        };
                        let changed = ui.add(edit.desired_width(320.0)).changed();
                        if changed { store.save_settings(); }

                        if spectrum_button(ui, if self.show_api_key { "Hide" } else { "Show" }, false, theme).clicked() {
                            self.show_api_key = !self.show_api_key;
                        }

                        if spectrum_button(ui, "Fetch Models from Groq", true, theme).clicked() {
                            let key = store.settings.groq.api_key.clone();
                            self.fetch_models(&key);
                        }
                    });

                    if let Some((ref msg, is_ok)) = self.models_status_msg {
                        ui.add_space(6.0);
                        let col = if is_ok { theme.diff_add_text } else { theme.diff_del_text };
                        ui.label(RichText::new(msg).color(col).font(FontId::proportional(11.0)));
                    }
                });

            ui.add_space(12.0);

            egui::Frame::new()
                .fill(theme.surface)
                .stroke(Stroke::new(1.0, theme.border))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.label(RichText::new("Chat Model Selection").font(FontId::proportional(13.0)).strong());
                    ui.label(RichText::new("Filtered automatically to chat/reasoning models").font(FontId::proportional(11.0)).color(theme.text_secondary));
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.label("Selected Model:");

                        let current_model = store.settings.groq.model.clone();
                        egui::ComboBox::from_id_salt("groq_model_selector")
                            .selected_text(RichText::new(&current_model).strong())
                            .width(280.0)
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(current_model == "openai/gpt-oss-120b", "openai/gpt-oss-120b (Recommended)").clicked() {
                                    store.settings.groq.model = "openai/gpt-oss-120b".to_string();
                                    store.save_settings();
                                }

                                for model in &self.fetched_models {
                                    if model.id == "openai/gpt-oss-120b" {
                                        continue;
                                    }
                                    if ui.selectable_label(current_model == model.id, &model.id).clicked() {
                                        store.settings.groq.model = model.id.clone();
                                        store.save_settings();
                                    }
                                }
                            });
                    });

                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.label("Reasoning Effort:");
                        let current_effort = store.settings.groq.reasoning_effort.clone();
                        egui::ComboBox::from_id_salt("reasoning_effort_selector")
                            .selected_text(RichText::new(&current_effort).strong())
                            .width(140.0)
                            .show_ui(ui, |ui| {
                                for effort in available_reasoning_efforts() {
                                    if ui.selectable_label(current_effort == effort, effort).clicked() {
                                        store.settings.groq.reasoning_effort = effort.to_string();
                                        store.save_settings();
                                    }
                                }
                            });
                    });

                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.label("Temperature:");
                        if ui.add(egui::Slider::new(&mut store.settings.groq.temperature, 0.0..=2.0)).changed() {
                            store.save_settings();
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Top P:");
                        if ui.add(egui::Slider::new(&mut store.settings.groq.top_p, 0.0..=1.0)).changed() {
                            store.save_settings();
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Max Completion Tokens:");
                        ui.label(RichText::new("65,536 (Highest Possible)").strong().color(theme.accent));
                    });
                });

            ui.add_space(12.0);

            egui::Frame::new()
                .fill(theme.surface)
                .stroke(Stroke::new(1.0, theme.border))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Global System Prompt").font(FontId::proportional(13.0)).strong());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if spectrum_button(ui, "Reset to Default", false, theme).clicked() {
                                store.settings.groq.global_system_prompt = GroqConfig::default().global_system_prompt;
                                store.save_settings();
                            }
                        });
                    });

                    ui.add_space(6.0);
                    let changed = ui.add(
                        egui::TextEdit::multiline(&mut store.settings.groq.global_system_prompt)
                            .desired_rows(4)
                            .desired_width(f32::INFINITY),
                    ).changed();
                    if changed {
                        store.save_settings();
                    }
                });

            ui.add_space(12.0);

            egui::Frame::new()
                .fill(theme.surface)
                .stroke(Stroke::new(1.0, theme.border))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.label(RichText::new("App Appearance & Windows Integration").font(FontId::proportional(13.0)).strong());
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.label("Adobe Spectrum 2 Theme:");
                        let is_dark = store.settings.theme_mode == ThemeMode::Dark;
                        if ui.selectable_label(is_dark, "Dark Mode").clicked() {
                            store.settings.theme_mode = ThemeMode::Dark;
                            store.save_settings();
                        }
                        if ui.selectable_label(!is_dark, "Light Mode").clicked() {
                            store.settings.theme_mode = ThemeMode::Light;
                            store.save_settings();
                        }
                    });

                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        let mut autostart_val = autostart::enabled();
                        if crate::theme::spectrum_switch(ui, &mut autostart_val, theme).changed() {
                            autostart::set(autostart_val);
                            store.settings.autostart = autostart_val;
                            store.save_settings();
                        }
                        ui.add_space(crate::theme::SPACE_8);
                        ui.label(RichText::new("Start Refined automatically with Windows").color(theme.text_primary));
                    });

                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Global Shortcut: Alt + Q (triggers only when text is actively selected).")
                            .font(FontId::proportional(11.0))
                            .color(theme.text_secondary),
                    );
                });

            ui.add_space(20.0);
        });
    }
}

fn render_metric_card(
    ui: &mut Ui,
    title: &str,
    value: &str,
    subtitle: &str,
    accent_color: Color32,
    theme: &SpectrumTheme,
) {
    egui::Frame::new()
        .fill(theme.surface)
        .stroke(Stroke::new(1.0, theme.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(140.0);
            ui.label(RichText::new(title).font(FontId::proportional(11.0)).color(theme.text_secondary));
            ui.add_space(2.0);
            ui.label(RichText::new(value).font(FontId::proportional(22.0)).color(accent_color).strong());
            ui.add_space(2.0);
            ui.label(RichText::new(subtitle).font(FontId::proportional(10.0)).color(theme.text_tertiary));
        });
}





