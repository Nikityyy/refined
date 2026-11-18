//! Refined Studio: History, Statistics, Action Customization, and Groq Settings.

use crate::actions::{default_actions, ActionItem};
use crate::autostart;
use crate::groq::{available_reasoning_efforts, fetch_chat_models, GroqConfig, GroqModel};
use crate::history::HistoryEntry;
use crate::stats::calculate_stats;
use crate::store::{Store, ThemeMode};
use crate::theme::{icons, render_diff_view, render_rf_logo, spectrum_badge, spectrum_button, spectrum_switch, SpectrumTheme};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Key, Layout, RichText, ScrollArea, Sense, Stroke, Ui, Vec2,
};

/// True when Ctrl is held and the given key was pressed this frame.
fn ctx_key(ui: &Ui, key: Key) -> bool {
    ui.input(|i| i.modifiers.ctrl && i.key_pressed(key) && !i.modifiers.shift && !i.modifiers.alt)
}
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
            new_action_icon: "✨".to_string(),
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

        ui.set_width(ui.available_width());

        // Keyboard navigation: Ctrl+1..4 jump between tabs
        {
            let pressed = |key: Key| ctx_key(ui, key);
            if pressed(Key::Num1) {
                self.current_tab = StudioTab::History;
            }
            if pressed(Key::Num2) {
                self.current_tab = StudioTab::Statistics;
            }
            if pressed(Key::Num3) {
                self.current_tab = StudioTab::Actions;
            }
            if pressed(Key::Num4) {
                self.current_tab = StudioTab::Settings;
            }
        }

        // Authentic Adobe Creative Cloud Top Navigation Bar
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(10.0, 0.0);

            // Glowing Adobe 'Rf' squircle mnemonic badge
            render_rf_logo(ui, 26.0);

            // Refined Brand
            ui.label(
                RichText::new("Refined")
                    .font(FontId::proportional(16.0))
                    .color(theme.text_primary)
                    .strong(),
            );

            spectrum_badge(ui, "v0.2.0", theme.surface_elevated, theme.text_secondary);

            ui.add_space(20.0);

            // Segmented pill navigation tabs
            let tabs = [
                (StudioTab::History, "History", "Ctrl+1"),
                (StudioTab::Statistics, "Statistics", "Ctrl+2"),
                (StudioTab::Actions, "Actions", "Ctrl+3"),
                (StudioTab::Settings, "Settings", "Ctrl+4"),
            ];

            for (tab, label, shortcut) in tabs {
                let is_active = self.current_tab == tab;
                let btn = if is_active {
                    egui::Button::new(RichText::new(label).color(Color32::WHITE).strong())
                        .fill(theme.accent)
                        .corner_radius(CornerRadius::same(6))
                        .stroke(Stroke::NONE)
                } else {
                    egui::Button::new(RichText::new(label).color(theme.text_secondary))
                        .fill(Color32::TRANSPARENT)
                        .corner_radius(CornerRadius::same(6))
                        .stroke(Stroke::NONE)
                };

                let resp = ui.add(btn).on_hover_text(shortcut);
                if resp.clicked() {
                    self.current_tab = tab;
                }
            }
        });

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

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
                RichText::new("History")
                    .font(FontId::proportional(16.0))
                    .color(theme.text_primary)
                    .strong(),
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !store.history.is_empty()
                    && spectrum_button(ui, "Clear All History", false, theme).clicked()
                {
                    store.clear_history();
                }

                // Filter by action
                let mut all_actions = vec!["All".to_string()];
                for a in default_actions() {
                    if !all_actions.contains(&a.name) {
                        all_actions.push(a.name);
                    }
                }
                egui::ComboBox::from_id_salt("history_action_filter")
                    .selected_text(&self.history_action_filter)
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        for a in all_actions {
                            ui.selectable_value(&mut self.history_action_filter, a.clone(), a);
                        }
                    });

                // Search box with fixed size
                ui.add_sized(
                    [220.0, 26.0],
                    egui::TextEdit::singleline(&mut self.history_search)
                        .align(Align2::LEFT_CENTER)
                        .hint_text("Search refinements..."),                );
            });
        });

        ui.add_space(10.0);

        if store.history.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                render_rf_logo(ui, 52.0);
                ui.add_space(14.0);
                ui.label(
                    RichText::new("No refinements yet")
                        .font(FontId::proportional(16.0))
                        .color(theme.text_primary)
                        .strong(),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Select text in any app, press Alt+Q, and pick an action.")
                        .font(FontId::proportional(12.0))
                        .color(theme.text_secondary),
                );
            });
            return;
        }

        let search = self.history_search.to_lowercase();
        let action_filter = self.history_action_filter.clone();

        let filtered: Vec<&HistoryEntry> = store
            .history
            .iter()
            .rev()
            .filter(|e| {
                if action_filter != "All" && e.action_name != action_filter {
                    return false;
                }
                if search.is_empty() {
                    return true;
                }
                e.action_name.to_lowercase().contains(&search)
                    || e.original_text.to_lowercase().contains(&search)
                    || e.refined_text.to_lowercase().contains(&search)
            })
            .collect();

        if filtered.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(RichText::new("No matching refinements found.").color(theme.text_secondary));
            });
            return;
        }

        let mut delete_id: Option<String> = None;
        let mut toggle_diff_id: Option<String> = None;

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 8.0);

                for entry in filtered {
                    let is_expanded = self.expanded_history_id.as_deref() == Some(&entry.id);

                    egui::Frame::new()
                        .fill(theme.surface)
                        .stroke(Stroke::new(1.0, theme.border))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(egui::Margin::same(12))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());

                            // Item Header
                            ui.horizontal(|ui| {
                                spectrum_badge(ui, &entry.action_name, theme.accent_subtle, theme.accent);
                                ui.label(
                                    RichText::new(entry.timestamp.format("%b %d, %H:%M").to_string())
                                        .font(FontId::proportional(12.0))
                                        .color(theme.text_tertiary),
                                );

                                let delta = entry.stats.word_delta();
                                let delta_str = if delta >= 0 {
                                    format!("+{delta}")
                                } else {
                                    format!("{delta}")
                                };
                                spectrum_badge(
                                    ui,
                                    // Clean right arrow → (no mojibake)
                                    &format!("{} → {} words ({})", entry.stats.original_words, entry.stats.refined_words, delta_str),
                                    theme.surface_elevated,
                                    theme.text_secondary,
                                );

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.add_sized([26.0, 22.0], egui::Button::new(RichText::new(icons::TRASH).color(theme.diff_del_text)).fill(Color32::TRANSPARENT).stroke(Stroke::NONE)).clicked() {
                                        delete_id = Some(entry.id.clone());
                                    }

                                    if spectrum_button(ui, if is_expanded { "Hide changes" } else { "Show changes" }, false, theme).clicked() {
                                        toggle_diff_id = Some(entry.id.clone());
                                    }

                                    if spectrum_button(ui, &format!("{} Copy", icons::COPY), false, theme).clicked() {
                                        crate::clipboard::set_clipboard_text(&entry.refined_text);
                                    }
                                });
                            });

                            ui.add_space(6.0);

                            // Content preview
                            if is_expanded {
                                ui.label(RichText::new("Original text").font(FontId::proportional(12.0)).color(theme.text_secondary));
                                ui.label(RichText::new(&entry.original_text).font(FontId::proportional(13.0)).color(theme.text_primary));
                                ui.add_space(6.0);

                                ui.label(RichText::new("Changes").font(FontId::proportional(12.0)).color(theme.accent).strong());
                                egui::Frame::new()
                                    .fill(theme.surface_elevated)
                                    .corner_radius(CornerRadius::same(6))
                                    .inner_margin(egui::Margin::same(8))
                                    .show(ui, |ui| {
                                        render_diff_view(ui, &entry.diff_chunks, theme);
                                    });
                            } else {
                                // Char-boundary-safe preview: raw byte slicing
                                // would panic on multi-byte UTF-8 (umlauts, CJK).
                                let preview = truncate_chars(&entry.refined_text, 140);
                                ui.label(RichText::new(preview).font(FontId::proportional(13.0)).color(theme.text_primary));
                            }
                        });
                }
            });

        if let Some(id) = delete_id {
            store.delete_history_entry(&id);
        }
        if let Some(id) = toggle_diff_id {
            if self.expanded_history_id.as_deref() == Some(&id) {
                self.expanded_history_id = None;
            } else {
                self.expanded_history_id = Some(id);
            }
        }
    }

    fn render_statistics_tab(&mut self, ui: &mut Ui, store: &Store, theme: &SpectrumTheme) {
        let stats = calculate_stats(&store.history);

        ui.label(
            RichText::new("Statistics")
                .font(FontId::proportional(16.0))
                .color(theme.text_primary)
                .strong(),
        );
        ui.label(
            RichText::new("Your refining activity at a glance.")
                .font(FontId::proportional(12.0))
                .color(theme.text_secondary),
        );

        // Scrollable so the charts are always reachable, even on small windows.
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.render_statistics_content(ui, theme, &stats);
            });
    }

    fn render_statistics_content(
        &mut self,
        ui: &mut Ui,
        theme: &SpectrumTheme,
        stats: &crate::stats::AggregatedStats,
    ) {
        ui.add_space(12.0);

        // 1. Responsive metric grid: 4 across on wide windows, 2x2 when narrow.
        // Card widths are computed once from the panel width so cards never
        // overflow, and every card in the grid shares the exact same size.
        let available_w = ui.available_width();
        let gap = 12.0;
        let min_card_w = 182.0;
        let per_row = if available_w >= min_card_w * 4.0 + gap * 3.0 { 4 } else { 2 };
        let card_w = ((available_w - gap * (per_row as f32 - 1.0)) / per_row as f32)
            .floor()
            .max(min_card_w);

        let metrics: Vec<(&str, String, String, Color32)> = vec![
            (
                "WORDS REFINED",
                fmt_thousands(stats.total_refined_words),
                format!("from {} original", fmt_thousands(stats.total_original_words)),
                theme.accent,
            ),
            (
                "REFINEMENTS",
                fmt_thousands(stats.total_refinements),
                "total AI operations".to_string(),
                theme.purple,
            ),
            (
                "TIME SAVED",
                format!("{:.1} min", stats.estimated_minutes_saved),
                "editing effort avoided".to_string(),
                Color32::from_rgb(0x22, 0xC5, 0x5E),
            ),
            (
                "AVG LATENCY",
                format!("{} ms", fmt_thousands(stats.average_latency_ms as usize)),
                "Groq inference speed".to_string(),
                theme.amber,
            ),
        ];

        for row in metrics.chunks(per_row) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(gap, 0.0);
                for (title, value, subtitle, accent_color) in row {
                    render_metric_card(
                        ui,
                        title,
                        value.as_str(),
                        subtitle.as_str(),
                        *accent_color,
                        card_w,
                        theme,
                    );
                }
            });
            ui.add_space(gap);
        }

        // 2. Two-Column Dashboard: Real Activity Bar Chart (Left) + Action Breakdown (Right)
        let col_gap = 16.0;
        let left_w = (available_w - col_gap) * 0.58;
        let right_w = (available_w - col_gap) * 0.42;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(col_gap, 0.0);

            // Left Column: Interactive 7-Day Activity Chart
            egui::Frame::new()
                .fill(theme.surface)
                .stroke(Stroke::new(1.0, theme.border))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(left_w);
                    ui.set_height(240.0);

                    ui.label(
                        RichText::new("Weekly Activity Velocity")
                            .font(FontId::proportional(14.0))
                            .color(theme.text_primary)
                            .strong(),
                    );
                    ui.label(
                        RichText::new("Daily operations and word volume processed")
                            .font(FontId::proportional(12.0))
                            .color(theme.text_secondary),
                    );

                    ui.add_space(16.0);

                    // Real interactive 7-day Bar Chart
                    if stats.total_refinements == 0 {
                        ui.vertical(|ui| {
                            ui.add_space(26.0);
                            ui.label(
                                RichText::new("Refinements will appear here once you use Alt+Q.")
                                    .font(FontId::proportional(12.0))
                                    .color(theme.text_secondary),
                            );
                        });
                    } else {
                        let max_count = stats.daily_counts.iter().map(|(_, c)| *c).max().unwrap_or(1).max(1) as f32;

                        ui.horizontal(|ui| {
                            let bar_slot_w = (left_w - 32.0) / 7.0;
                            ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);

                            for (day, count) in &stats.daily_counts {
                                ui.allocate_ui_with_layout(
                                    Vec2::new(bar_slot_w, 150.0),
                                    Layout::top_down(Align::Center),
                                    |ui| {
                                        let ratio = (*count as f32 / max_count).clamp(0.04, 1.0);
                                        let bar_h = 92.0 * ratio;

                                    ui.add_space(100.0 - bar_h);

                                    // Bar container
                                    let (bar_rect, bar_resp) = ui.allocate_exact_size(
                                        Vec2::new(26.0, bar_h),
                                        Sense::hover(),
                                    );

                                    let bar_color = if *count > 0 {
                                        if bar_resp.hovered() { theme.accent_hover } else { theme.accent }
                                    } else {
                                        theme.surface_elevated
                                    };

                                    ui.painter().rect_filled(
                                        bar_rect,
                                        CornerRadius::same(4),
                                        bar_color,
                                    );

                                    bar_resp.on_hover_ui(|ui| {
                                        ui.label(RichText::new(format!("{day}: {count} refinements")).strong());
                                    });

                                    ui.add_space(6.0);
                                    ui.label(
                                        RichText::new(format!("{count}"))
                                            .font(FontId::proportional(12.0))
                                            .color(if *count > 0 { theme.text_primary } else { theme.text_tertiary })
                                            .strong(),
                                    );
                                    ui.label(
                                        RichText::new(day)
                                            .font(FontId::proportional(12.0))
                                            .color(theme.text_secondary),
                                    );
                                },
                            );
                        }
                        });
                    }
                });

            // Right Column: Action Usage Distribution
            egui::Frame::new()
                .fill(theme.surface)
                .stroke(Stroke::new(1.0, theme.border))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(right_w);
                    ui.set_height(240.0);

                    ui.label(
                        RichText::new("Action Distribution")
                            .font(FontId::proportional(14.0))
                            .color(theme.text_primary)
                            .strong(),
                    );
                    ui.label(
                        RichText::new("Breakdown of study and writing tools used")
                            .font(FontId::proportional(12.0))
                            .color(theme.text_secondary),
                    );

                    ui.add_space(12.0);

                    if stats.action_counts.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(40.0);
                            ui.label(RichText::new("No actions recorded yet.").color(theme.text_secondary));
                        });
                    } else {
                        let total = stats.total_refinements.max(1) as f32;
                        let mut sorted_actions: Vec<(String, usize)> =
                            stats.action_counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
                        sorted_actions.sort_by(|a, b| b.1.cmp(&a.1));

                        for (action_name, count) in sorted_actions.iter().take(5) {
                            let pct = (*count as f32 / total) * 100.0;
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(action_name).font(FontId::proportional(12.0)).strong().color(theme.text_primary));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(RichText::new(format!("{count} ({pct:.0}%)")).font(FontId::proportional(12.0)).color(theme.text_secondary));
                                });
                            });

                            ui.add(
                                egui::ProgressBar::new(pct / 100.0)
                                    .fill(theme.accent)
                                    .corner_radius(CornerRadius::same(3))
                                    .animate(false),
                            );
                            ui.add_space(4.0);
                        }
                    }
                });
        });
    }

    fn render_actions_tab(&mut self, ui: &mut Ui, store: &mut Store, theme: &SpectrumTheme) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Actions")
                    .font(FontId::proportional(16.0))
                    .color(theme.text_primary)
                    .strong(),
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if spectrum_button(ui, "+ Add Custom Action", true, theme).clicked() {
                    self.new_action_dialog_open = true;
                    self.new_action_name.clear();
                    self.new_action_desc.clear();
                    self.new_action_icon = "✨".to_string();
                    self.new_action_shortcut.clear();
                    self.new_action_prompt.clear();
                }

                if spectrum_button(ui, "Reset All to Defaults", false, theme).clicked() {
                    store.reset_all_actions();
                }
            });
        });

        ui.label(
            RichText::new("Edit the prompt behind any action, or add your own.")
                .font(FontId::proportional(12.0))
                .color(theme.text_secondary),
        );

        ui.add_space(12.0);

        // Add Custom Action Dialog
        if self.new_action_dialog_open {
            egui::Frame::new()
                .fill(theme.surface_elevated)
                .stroke(Stroke::new(1.0, theme.accent))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("Create New Action")
                            .font(FontId::proportional(14.0))
                            .color(theme.accent)
                            .strong(),
                    );
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Name:").font(FontId::proportional(12.0)).color(theme.text_secondary));
                        ui.add_sized([160.0, 24.0], egui::TextEdit::singleline(&mut self.new_action_name).align(Align2::LEFT_CENTER).hint_text("e.g. Formalize"));

                        ui.add_space(10.0);
                        ui.label(RichText::new("Icon:").font(FontId::proportional(12.0)).color(theme.text_secondary));
                        ui.add_sized([40.0, 24.0], egui::TextEdit::singleline(&mut self.new_action_icon).align(Align2::LEFT_CENTER));

                        ui.add_space(10.0);
                        ui.label(RichText::new("Shortcut:").font(FontId::proportional(12.0)).color(theme.text_secondary));
                        ui.add_sized([40.0, 24.0], egui::TextEdit::singleline(&mut self.new_action_shortcut).align(Align2::LEFT_CENTER).hint_text("7"));
                    });

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Description:").font(FontId::proportional(12.0)).color(theme.text_secondary));
                        ui.add_sized([ui.available_width() - 80.0, 24.0], egui::TextEdit::singleline(&mut self.new_action_desc).align(Align2::LEFT_CENTER).hint_text("Brief summary of what this action does"));
                    });

                    ui.add_space(6.0);
                    ui.label(RichText::new("Prompt Instruction:").font(FontId::proportional(12.0)).color(theme.text_secondary));
                    ui.add(
                        egui::TextEdit::multiline(&mut self.new_action_prompt)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    );

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if spectrum_button(ui, "Save Action", true, theme).clicked() {
                            let name = self.new_action_name.trim().to_string();
                            if !name.is_empty() {
                                let id = name.to_lowercase().replace(' ', "_");
                                let action = ActionItem {
                                    id,
                                    name,
                                    description: self.new_action_desc.trim().to_string(),
                                    icon: if self.new_action_icon.trim().is_empty() { "✨".to_string() } else { self.new_action_icon.trim().to_string() },
                                    shortcut: self.new_action_shortcut.trim().to_string(),
                                    prompt_instruction: self.new_action_prompt.trim().to_string(),
                                    default_instruction: self.new_action_prompt.trim().to_string(),
                                    is_builtin: false,
                                    is_rewrite_as: false,
                                };
                                store.add_custom_action(action);
                                self.new_action_dialog_open = false;
                            }
                        }

                        if spectrum_button(ui, "Cancel", false, theme).clicked() {
                            self.new_action_dialog_open = false;
                        }
                    });
                });

            ui.add_space(10.0);
        }

        let mut reset_action_id: Option<String> = None;
        let mut delete_action_id: Option<String> = None;
        let mut save_needed = false;

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 10.0);

                let actions = &mut store.settings.actions;
                for action in actions.iter_mut() {
                    egui::Frame::new()
                        .fill(theme.surface)
                        .stroke(Stroke::new(1.0, theme.border))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(egui::Margin::same(14))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());

                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&action.icon)
                                        .font(FontId::proportional(16.0))
                                        .color(theme.accent),
                                );
                                ui.label(
                                    RichText::new(&action.name)
                                        .font(FontId::proportional(14.0))
                                        .color(theme.text_primary)
                                        .strong(),
                                );

                                if !action.is_builtin {
                                    spectrum_badge(ui, "Custom", theme.accent_subtle, theme.accent);
                                }
                                if action.is_modified() {
                                    spectrum_badge(ui, "Modified", theme.diff_add_bg, theme.diff_add_text);
                                }

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if !action.is_builtin {
                                        if ui.add_sized([64.0, 24.0], egui::Button::new(RichText::new(format!("{} Delete", icons::TRASH)).color(theme.diff_del_text)).fill(Color32::TRANSPARENT).stroke(Stroke::NONE)).clicked() {
                                            delete_action_id = Some(action.id.clone());
                                        }
                                    }

                                    if action.is_modified() {
                                        if spectrum_button(ui, "Reset to Default", false, theme).clicked() {
                                            reset_action_id = Some(action.id.clone());
                                        }
                                    }

                                    if !action.shortcut.is_empty() {
                                        spectrum_badge(ui, &format!("Key: {}", action.shortcut), theme.surface_elevated, theme.text_secondary);
                                    }
                                });
                            });

                            ui.label(
                                RichText::new(&action.description)
                                    .font(FontId::proportional(12.0))
                                    .color(theme.text_secondary),
                            );

                            ui.add_space(6.0);
                            ui.label(
                                RichText::new("System Instruction / Prompt:")
                                    .font(FontId::proportional(12.0))
                                    .color(theme.text_tertiary),
                            );

                            let edit = ui.add(
                                egui::TextEdit::multiline(&mut action.prompt_instruction)
                                    .desired_rows(2)
                                    .desired_width(f32::INFINITY),
                            );
                            if edit.changed() {
                                save_needed = true;
                            }
                        });
                }
            });

        if let Some(id) = reset_action_id {
            store.reset_action_to_default(&id);
        }
        if let Some(id) = delete_action_id {
            store.delete_custom_action(&id);
        }
        if save_needed {
            store.save_settings();
        }
    }

    fn render_settings_tab(&mut self, ui: &mut Ui, store: &mut Store, theme: &SpectrumTheme) {
        ScrollArea::vertical().show(ui, |ui| {
            ui.set_width(ui.available_width());

            ui.label(
                RichText::new("Settings")
                    .font(FontId::proportional(16.0))
                    .color(theme.text_primary)
                    .strong(),
            );
            ui.label(
                RichText::new("Groq access, model behavior, and appearance.")
                    .font(FontId::proportional(12.0))
                    .color(theme.text_secondary),
            );

            ui.add_space(14.0);

            // Groq API Key Card
            egui::Frame::new()
                .fill(theme.surface)
                .stroke(Stroke::new(1.0, theme.border))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    ui.label(
                        RichText::new("Groq API Key")
                            .font(FontId::proportional(14.0))
                            .color(theme.text_primary)
                            .strong(),
                    );
                    ui.label(
                        RichText::new("Get your free key from https://console.groq.com/keys")
                            .font(FontId::proportional(12.0))
                            .color(theme.text_secondary),
                    );
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        // Fixed width buttons so Show/Hide NEVER jitters container width!
                        let input_w = (ui.available_width() - 250.0).max(200.0);
                        let edit = if self.show_api_key {
                            ui.add_sized([input_w, 28.0], egui::TextEdit::singleline(&mut store.settings.groq.api_key).align(Align2::LEFT_CENTER))
                        } else {
                            ui.add_sized([input_w, 28.0], egui::TextEdit::singleline(&mut store.settings.groq.api_key).align(Align2::LEFT_CENTER).password(true))
                        };
                        if edit.changed() {
                            store.save_settings();
                        }

                        // Fixed 64px width for Show/Hide button to eliminate layout shifts
                        let toggle_text = if self.show_api_key { "Hide" } else { "Show" };
                        if ui.add_sized([64.0, 28.0], egui::Button::new(RichText::new(toggle_text).color(theme.text_primary)).fill(theme.surface_elevated).stroke(Stroke::new(1.0, theme.border)).corner_radius(CornerRadius::same(6))).clicked() {
                            self.show_api_key = !self.show_api_key;
                        }

                        // Fixed 170px width for Fetch Models button
                        if ui.add_sized([170.0, 28.0], egui::Button::new(RichText::new("Fetch Models from Groq").color(Color32::WHITE).strong()).fill(theme.accent).stroke(Stroke::NONE).corner_radius(CornerRadius::same(6))).clicked() {
                            let key = store.settings.groq.api_key.clone();
                            self.fetch_models(&key);
                        }
                    });

                    if let Some((ref msg, ok)) = self.models_status_msg {
                        ui.add_space(6.0);
                        let color = if ok { theme.diff_add_text } else { theme.diff_del_text };
                        ui.label(RichText::new(msg).font(FontId::proportional(12.0)).color(color));
                    }
                });

            ui.add_space(14.0);

            // Model & Reasoning Settings Card
            egui::Frame::new()
                .fill(theme.surface)
                .stroke(Stroke::new(1.0, theme.border))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    ui.label(
                        RichText::new("AI Model & Inference Configuration")
                            .font(FontId::proportional(14.0))
                            .color(theme.text_primary)
                            .strong(),
                    );
                    ui.add_space(10.0);

                    // Model selection
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Active Model:").font(FontId::proportional(12.0)).color(theme.text_primary));
                        let current_model = store.settings.groq.model.clone();

                        egui::ComboBox::from_id_salt("groq_model_selector")
                            .selected_text(&current_model)
                            .width(260.0)
                            .show_ui(ui, |ui| {
                                if !self.fetched_models.is_empty() {
                                    for m in &self.fetched_models {
                                        let label = if m.id == "openai/gpt-oss-120b" {
                                            format!("{} (Recommended)", m.id)
                                        } else {
                                            m.id.clone()
                                        };
                                        if ui.selectable_value(&mut store.settings.groq.model, m.id.clone(), label).clicked() {
                                            store.save_settings();
                                        }
                                    }
                                } else {
                                    // Shown only until the user fetches the
                                    // live list; does not include models that
                                    // Groq has retired.
                                    let defaults = [
                                        "openai/gpt-oss-120b",
                                        "openai/gpt-oss-20b",
                                        "qwen/qwen3.8-27b",
                                    ];
                                    for m in defaults {
                                        if ui.selectable_value(&mut store.settings.groq.model, m.to_string(), m).clicked() {
                                            store.save_settings();
                                        }
                                    }
                                }
                            });
                    });

                    ui.add_space(8.0);

                    // Reasoning effort
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Reasoning Effort:").font(FontId::proportional(12.0)).color(theme.text_primary));
                        let current_effort = store.settings.groq.reasoning_effort.clone();

                        egui::ComboBox::from_id_salt("groq_reasoning_effort")
                            .selected_text(&current_effort)
                            .width(140.0)
                            .show_ui(ui, |ui| {
                                for effort in available_reasoning_efforts() {
                                    if ui.selectable_value(&mut store.settings.groq.reasoning_effort, effort.to_string(), effort).clicked() {
                                        store.save_settings();
                                    }
                                }
                            });

                        ui.add_space(10.0);
                        ui.label(
                            RichText::new("(Default: high; automatically falls back if model does not support reasoning)")
                                .font(FontId::proportional(12.0))
                                .color(theme.text_secondary),
                        );
                    });

                    ui.add_space(12.0);

                    // Temperature slider
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Temperature:").font(FontId::proportional(12.0)).color(theme.text_primary));
                        let slider = ui.add(egui::Slider::new(&mut store.settings.groq.temperature, 0.0..=2.0).step_by(0.05));
                        if slider.changed() {
                            store.save_settings();
                        }
                    });

                    ui.add_space(8.0);

                    // Top-P slider
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Top-P:").font(FontId::proportional(12.0)).color(theme.text_primary));
                        let slider = ui.add(egui::Slider::new(&mut store.settings.groq.top_p, 0.0..=1.0).step_by(0.05));
                        if slider.changed() {
                            store.save_settings();
                        }
                    });

                    ui.add_space(8.0);

                    // Max Tokens (aligned to 32,768)
                    ui.label(
                        RichText::new("Max Completion Tokens: 32,768 (Optimal context limit)")
                            .font(FontId::proportional(12.0))
                            .color(theme.text_secondary),
                    );
                });

            ui.add_space(14.0);

            // Global System Prompt Card
            egui::Frame::new()
                .fill(theme.surface)
                .stroke(Stroke::new(1.0, theme.border))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Global System Prompt")
                                .font(FontId::proportional(14.0))
                                .color(theme.text_primary)
                                .strong(),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if spectrum_button(ui, "Reset to Default", false, theme).clicked() {
                                store.settings.groq.global_system_prompt = GroqConfig::default().global_system_prompt;
                                store.save_settings();
                            }
                        });
                    });

                    ui.add_space(8.0);

                    let edit = ui.add(
                        egui::TextEdit::multiline(&mut store.settings.groq.global_system_prompt)
                            .desired_rows(4)
                            .desired_width(f32::INFINITY),
                    );
                    if edit.changed() {
                        store.save_settings();
                    }
                });

            ui.add_space(14.0);

            // Appearance & Platform Card
            egui::Frame::new()
                .fill(theme.surface)
                .stroke(Stroke::new(1.0, theme.border))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    ui.label(
                        RichText::new("Appearance & Windows Integration")
                            .font(FontId::proportional(14.0))
                            .color(theme.text_primary)
                            .strong(),
                    );
                    ui.add_space(10.0);

                    // Theme selector
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Theme:").font(FontId::proportional(12.0)).color(theme.text_primary));
                        let is_dark = theme.is_dark;

                        if ui.selectable_label(is_dark, "Dark Mode").clicked() {
                            store.settings.theme_mode = ThemeMode::Dark;
                            store.save_settings();
                        }
                        if ui.selectable_label(!is_dark, "Light Mode").clicked() {
                            store.settings.theme_mode = ThemeMode::Light;
                            store.save_settings();
                        }
                    });

                    ui.add_space(10.0);

                    // Autostart sliding switch
                    ui.horizontal(|ui| {
                        let mut autostart_val = autostart::enabled();
                        if spectrum_switch(ui, &mut autostart_val, theme).changed() {
                            autostart::set(autostart_val);
                            store.settings.autostart = autostart_val;
                            store.save_settings();
                        }
                        ui.add_space(8.0);
                        ui.label(RichText::new("Start Refined automatically with Windows").color(theme.text_primary));
                    });

                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Global Shortcut: Alt + Q (triggers only when text is actively selected).")
                            .font(FontId::proportional(12.0))
                            .color(theme.text_secondary),
                    );
                });

            ui.add_space(20.0);
        });
    }
}

/// Formats a number with thousands separators (12345 -> "12,345").
fn fmt_thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Truncates to at most `max_chars` characters, appending an ellipsis when cut,
/// and never splitting a multi-byte character.
fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let cut: String = text.chars().take(max_chars).collect();
    format!("{}...", cut.trim_end())
}

fn render_metric_card(
    ui: &mut Ui,
    title: &str,
    value: &str,
    subtitle: &str,
    accent_color: Color32,
    card_width: f32,
    theme: &SpectrumTheme,
) {
    egui::Frame::new()
        .fill(theme.surface)
        .stroke(Stroke::new(1.0, theme.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(card_width);
            ui.set_min_height(72.0);

            ui.label(
                RichText::new(title)
                    .font(FontId::proportional(12.0))
                    .color(theme.text_secondary)
                    .strong(),
            );
            ui.add_space(2.0);
            ui.label(
                RichText::new(value)
                    .font(FontId::proportional(22.0))
                    .color(accent_color)
                    .strong(),
            );
            ui.label(
                RichText::new(subtitle)
                    .font(FontId::proportional(12.0))
                    .color(theme.text_tertiary),
            );
        });
}
