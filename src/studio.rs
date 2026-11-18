//! Refined Studio: History, Statistics, Action Customization, and Groq Settings.

use crate::actions::ActionItem;
use crate::autostart;
use crate::groq::{available_reasoning_efforts, fetch_chat_models, GroqConfig, GroqModel};
use crate::history::HistoryEntry;
use crate::stats::calculate_stats;
use crate::store::{Store, ThemeMode};
use crate::theme::{card_frame, icons, render_diff_view, render_rf_logo, spectrum_badge, spectrum_button, spectrum_button_compact, spectrum_slider, spectrum_switch, SpectrumTheme};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Key, Layout, Pos2, Rect, RichText,
    ScrollArea, Sense, Stroke, Ui, Vec2,
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
    pub new_action_prompt: String,
    /// Set from the hotkey thread each frame: Alt+Q failed to register.
    pub hotkey_failed: bool,
    /// Pending settings save: UI edits mark it, and the next update flushes
    /// it at most ~half a second later. A write per keystroke or slider tick
    /// was a full create/flush/rename on the UI thread for nothing.
    pub settings_save_at: Option<std::time::Instant>,
    /// Cached aggregate stats keyed by (history length, newest entry id,
    /// today). Plain recalculation is O(n) with chunk scans and used to run
    /// every frame while the Statistics tab was open.
    pub stats_cache: Option<(usize, String, String, crate::stats::AggregatedStats)>,
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
            new_action_prompt: String::new(),
            hotkey_failed: false,
            settings_save_at: None,
            stats_cache: None,
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

    /// Marks settings as edited; the next update flushes them after the
    /// debounce window so a burst of keystrokes or slider ticks becomes
    /// one atomic write instead of dozens.
    fn mark_settings_dirty(&mut self) {
        self.settings_save_at = Some(std::time::Instant::now());
    }

    /// Flushes a pending settings save if its debounce window has passed.
    /// Returns whether anything was written.
    pub fn flush_settings(&mut self, store: &mut Store) -> bool {
        if let Some(at) = self.settings_save_at {
            if at.elapsed() >= std::time::Duration::from_millis(400) {
                store.save_settings();
                self.settings_save_at = None;
                return true;
            }
        }
        false
    }

    /// Takes a pending mark, if any; the caller then persists. Used when the
    /// Studio hides or the app exits, before the debounce window would have
    /// elapsed in `update`.
    pub fn take_pending_save(&mut self) -> bool {
        self.settings_save_at.take().is_some()
    }

    pub fn update(&mut self, ui: &mut Ui, store: &mut Store, theme: &SpectrumTheme) {
        self.flush_settings(store);

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
            if pressed(Key::Num2) && self.current_tab != StudioTab::Statistics {
                if self.settings_save_at.is_some() {
                    store.save_settings();
                    self.settings_save_at = None;
                }
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
                // Custom-drawn pill: egui's stock Button centers the font's
                // em-box, and Segoe's ink sits 1px low in it, so the text
                // read as pushed down inside the pill. We center the ink.
                let galley = ui.painter().layout(
                    label.to_string(),
                    FontId::proportional(13.0),
                    if is_active { theme.text_primary } else { theme.text_secondary },
                    f32::INFINITY,
                );
                let size = Vec2::new(galley.size().x + 24.0, 24.0);
                let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                let hovered = resp.hovered();
                let bg = if is_active || hovered {
                    theme.surface_hover
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(4), bg);
                ui.painter().galley(
                    Pos2::new(
                        rect.left() + 12.0,
                        rect.center().y - galley.size().y / 2.0 - 1.0,
                    ),
                    galley,
                    Color32::WHITE,
                );
                let resp = resp.on_hover_text(shortcut);
                if resp.clicked() && self.current_tab != tab {
                    // Leaving an edit-heavy tab should persist edits at once,
                    // not after the debounce window.
                    if self.settings_save_at.is_some() {
                        store.save_settings();
                        self.settings_save_at = None;
                    }
                    self.current_tab = tab;
                }
            }
        });

        // Balance the nav band: the panel margin puts 12px above the tab
        // row, so the separator below gets matching tight spacing instead
        // of a 32px dead zone before the content heading.
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);

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
                    && spectrum_button(ui, Some(icons::TRASH), "Clear All History", false, theme)
                        .clicked()
                {
                    store.clear_history();
                }

                // Filter by action: built from the live action list plus any
                // names that only exist in history, so custom and deleted
                // actions remain filterable.
                let mut all_actions = vec!["All".to_string()];
                for a in store.settings.actions.iter().map(|a| a.name.clone()) {
                    if !all_actions.contains(&a) {
                        all_actions.push(a);
                    }
                }
                for e in store.history.iter().map(|e| e.action_name.clone()) {
                    if !all_actions.contains(&e) {
                        all_actions.push(e);
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

        // store.history is newest-first; display it in that order.
        let filtered: Vec<&HistoryEntry> = store
            .history
            .iter()
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
                // Controls inside cards inherit this spacing; x=0 rendered
                // badges and buttons flush against each other.
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);

                for entry in filtered {
                    let is_expanded = self.expanded_history_id.as_deref() == Some(&entry.id);

                    card_frame(theme, egui::Margin::same(12))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());

                            // Item Header
                            ui.horizontal(|ui| {
                                spectrum_badge(
                                    ui,
                                    &entry.action_name,
                                    theme.accent_badge_bg,
                                    theme.accent_badge_text,
                                );
                                ui.label(
                                    RichText::new(entry.timestamp.format("%b %d, %H:%M").to_string())
                                        .font(FontId::proportional(12.0))
                                        .color(theme.text_tertiary),
                                );

                                let delta = entry.stats.word_delta();
                                // Zero-delta entries read cleaner without
                                // an empty "(+0)" badge.
                                let delta_str = match delta {
                                    0 => String::new(),
                                    d if d > 0 => format!(" (+{d})"),
                                    d => format!(" ({d})"),
                                };
                                spectrum_badge(
                                    ui,
                                    &format!("{} → {} words{}", entry.stats.original_words, entry.stats.refined_words, delta_str),
                                    theme.surface_subtle,
                                    theme.text_primary,
                                );

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.add_sized([26.0, 22.0], egui::Button::new(RichText::new(icons::TRASH).color(theme.diff_del_text)).fill(Color32::TRANSPARENT).stroke(Stroke::NONE)).clicked() {
                                        delete_id = Some(entry.id.clone());
                                    }

                                    if spectrum_button_compact(
                                        ui,
                                        Some(icons::EYE),
                                        if is_expanded { "Hide changes" } else { "Show changes" },
                                        false,
                                        theme,
                                    )
                                    .clicked()
                                    {
                                        toggle_diff_id = Some(entry.id.clone());
                                    }

                                    if spectrum_button_compact(ui, Some(icons::COPY), "Copy", false, theme)
                                        .clicked()
                                    {
                                        crate::clipboard::set_clipboard_text(&entry.refined_text);
                                    }
                                });
                            });

                            ui.add_space(4.0);

                            // Content preview
                            if is_expanded {
                                ui.label(RichText::new("Original text").font(FontId::proportional(12.0)).color(theme.text_secondary));
                                ui.label(RichText::new(&entry.original_text).font(FontId::proportional(13.0)).color(theme.text_primary));
                                ui.add_space(6.0);

                                ui.label(RichText::new("Changes").font(FontId::proportional(12.0)).color(theme.accent).strong());
                                egui::Frame::new()
                                    .fill(theme.surface_elevated)
                                    .corner_radius(CornerRadius::same(4))
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
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let key = (
            store.history.len(),
            store.history.first().map(|e| e.id.clone()).unwrap_or_default(),
            today,
        );
        let stats = match &self.stats_cache {
            Some((len, id, day, cached)) if *len == key.0 && *id == key.1 && *day == key.2 => {
                cached.clone()
            }
            _ => {
                let stats = calculate_stats(&store.history);
                self.stats_cache = Some((key.0, key.1, key.2, stats.clone()));
                stats
            }
        };

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
                self.render_statistics_content(ui, theme, &stats, &store.settings.actions);
            });
    }

    fn render_statistics_content(
        &mut self,
        ui: &mut Ui,
        theme: &SpectrumTheme,
        stats: &crate::stats::AggregatedStats,
        actions: &[crate::actions::ActionItem],
    ) {
        ui.add_space(12.0);

        // 1. Metric cards. Widths come from the true available width minus
        // gaps AND the frames' painted horizontal margins, so the row can
        // never clip off the window edge. Wide windows get 4 across,
        // narrow windows 2. Values are quiet neutral numbers - Adobe
        // dashboards reserve color for actions, not digits.
        let available_w = ui.available_width();
        let gap = 12.0;
        // inner_margin(14) painted on both sides PLUS the 1px frame stroke
        // on each side: 30 px per card. Subtracting only 28 made this row
        // overshoot the dashboard cards below by ~8px, whose own row came
        // up 4px short - the right edges visibly failed to stack.
        let side_margin = 30.0;
        let per_row = if available_w >= 182.0 * 4.0 + gap * 3.0 + side_margin * 4.0 {
            4
        } else {
            2
        };
        let card_w = ((available_w - gap * (per_row as f32 - 1.0) - side_margin * per_row as f32)
            / per_row as f32)
            .floor()
            .max(96.0);

        let metrics: Vec<(&str, String, String)> = vec![
            (
                "Words refined",
                fmt_thousands(stats.total_refined_words),
                format!("from {} original", fmt_thousands(stats.total_original_words)),
            ),
            (
                "Refinements",
                fmt_thousands(stats.total_refinements),
                "total AI operations".to_string(),
            ),
            (
                "Edit time saved",
                format!("{:.1} min", stats.estimated_minutes_saved),
                "manual rework avoided".to_string(),
            ),
            (
                "Avg latency",
                format!("{} ms", fmt_thousands(stats.average_latency_ms as usize)),
                "Groq inference speed".to_string(),
            ),
        ];

        for row in metrics.chunks(per_row) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(gap, 0.0);
                for (title, value, subtitle) in row {
                    render_metric_card(ui, title, value.as_str(), subtitle.as_str(), card_w, theme);
                }
            });
            ui.add_space(gap);
        }

        // 2. Two-Column Dashboard: Real Activity Bar Chart (Left) + Action Breakdown (Right)
        let col_gap = 16.0;
        // Each card paints inner_margin(16) on both sides (=32) PLUS its
        // 1px stroke on both sides: 34 px per card, 68 across both. With
        // that subtracted, this row's right edge stacks exactly with the
        // metric-card row above it.
        let frame_margins = 68.0;
        let left_w = (available_w - col_gap - frame_margins) * 0.58;
        let right_w = (available_w - col_gap - frame_margins) * 0.42;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(col_gap, 0.0);

            // Left Column: Interactive 7-Day Activity Chart
            card_frame(theme, egui::Margin::same(16))
                .show(ui, |ui| {
                    // A Frame inside a horizontal parent inherits that layout,
                    // which previously sandwiched header, chart, and labels
                    // into one row. The body must be vertical.
                    ui.vertical(|ui| {
                        ui.set_width(left_w);
                        ui.set_min_height(250.0);

                        ui.label(
                            RichText::new("Refinements per day")
                                .font(FontId::proportional(14.0))
                                .color(theme.text_primary)
                                .strong(),
                        );
                        ui.label(
                            RichText::new("How many refinements you ran each day")
                                .font(FontId::proportional(12.0))
                                .color(theme.text_secondary),
                        );

                        ui.add_space(16.0);

                        if stats.total_refinements == 0 {
                            ui.add_space(36.0);
                            ui.label(
                                RichText::new("Refinements will appear here once you use Alt+Q.")
                                    .font(FontId::proportional(12.0))
                                    .color(theme.text_secondary),
                            );
                            return;
                        }

                        // Bars grow from a shared baseline; the count rides
                        // above the bar, the day label below it. No hover
                        // tooltip at all: the counts are printed inline, and a
                        // popup near the baseline sat right on top of the day
                        // labels and flickered them.
                        let max_count = stats
                            .daily_counts
                            .iter()
                            .map(|(_, c)| *c)
                            .max()
                            .unwrap_or(1)
                            .max(1) as f32;
                        let plot_h = 120.0;
                        let label_band = 22.0;
                        let (plot_rect, _) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), plot_h + label_band),
                            Sense::hover(),
                        );
                        let plot_w = plot_rect.width();
                        let slot = plot_w / 7.0;
                        let baseline = plot_rect.bottom() - label_band;

                        for (i, (day, count)) in stats.daily_counts.iter().enumerate() {
                            let ratio = (*count as f32 / max_count).clamp(0.04, 1.0);
                            let bar_h = plot_h * ratio;
                            let bar_w = 26.0_f32.min(slot - 8.0);
                            let cx = plot_rect.left() + slot * (i as f32 + 0.5);
                            let bar_rect = Rect::from_min_max(
                                Pos2::new(cx - bar_w / 2.0, baseline - bar_h),
                                Pos2::new(cx + bar_w / 2.0, baseline),
                            );
                            let bar_color = if *count > 0 {
                                theme.accent
                            } else {
                                theme.surface_active
                            };
                            ui.painter()
                                .rect_filled(bar_rect, CornerRadius::same(4), bar_color);

                            let count_galley = ui.painter().layout(
                                format!("{count}"),
                                FontId::proportional(12.0),
                                if *count > 0 { theme.text_primary } else { theme.text_tertiary },
                                f32::INFINITY,
                            );
                            ui.painter().galley(
                                Pos2::new(
                                    cx - count_galley.size().x / 2.0,
                                    bar_rect.top() - count_galley.size().y - 3.0,
                                ),
                                count_galley,
                                Color32::WHITE,
                            );

                            let day_galley = ui.painter().layout(
                                day.clone(),
                                FontId::proportional(12.0),
                                theme.text_secondary,
                                f32::INFINITY,
                            );
                            ui.painter().galley(
                                Pos2::new(cx - day_galley.size().x / 2.0, baseline + 4.0),
                                day_galley,
                                Color32::WHITE,
                            );
                        }
                    });
                });

            // Right Column: Action Usage Distribution
            card_frame(theme, egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(right_w);
                        ui.set_min_height(250.0);

                        ui.label(
                            RichText::new("Action distribution")
                                .font(FontId::proportional(14.0))
                                .color(theme.text_primary)
                                .strong(),
                        );
                        ui.label(
                            RichText::new("How often each action was used")
                                .font(FontId::proportional(12.0))
                                .color(theme.text_secondary),
                        );

                        ui.add_space(12.0);

                        if stats.action_counts.is_empty() {
                            ui.add_space(40.0);
                            ui.label(
                                RichText::new("No actions recorded yet.")
                                    .font(FontId::proportional(12.0))
                                    .color(theme.text_secondary),
                            );
                            return;
                        }

                        let total = stats.total_refinements.max(1) as f32;
                        // Every action appears - unused ones at 0 - so the
                        // panel keeps its full shape instead of collapsing
                        // to the single action that happens to have counts.
                        let mut rows: Vec<(String, usize)> = actions
                            .iter()
                            .map(|a| {
                                (
                                    a.name.clone(),
                                    *stats.action_counts.get(&a.name).unwrap_or(&0),
                                )
                            })
                            .collect();
                        for (k, v) in &stats.action_counts {
                            if !rows.iter().any(|(n, _)| n == k) {
                                rows.push((k.clone(), *v));
                            }
                        }
                        rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

                        for (action_name, count) in rows.iter().take(8) {
                            let pct = (*count as f32 / total) * 100.0;
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(action_name).font(FontId::proportional(12.0)).color(theme.text_primary));
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
                    });
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
                if spectrum_button(ui, Some(icons::ADD), "Add Custom Action", true, theme)
                    .clicked()
                {
                    self.new_action_dialog_open = true;
                    self.new_action_name.clear();
                    self.new_action_desc.clear();
                    self.new_action_icon = "E70F".to_string();
                    self.new_action_prompt.clear();
                }

                if spectrum_button(ui, Some(icons::REFRESH), "Reset All to Defaults", false, theme)
                    .clicked()
                {
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
                        ui.add_sized([60.0, 24.0], egui::TextEdit::singleline(&mut self.new_action_icon).align(Align2::LEFT_CENTER).hint_text("e.g. E70F"))
                            .on_hover_text("A Segoe MDL2 icon codepoint, e.g. E70F");
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
                        if spectrum_button(ui, Some(icons::SAVE), "Save Action", true, theme)
                            .clicked()
                        {
                            let name = self.new_action_name.trim().to_string();
                            // Custom icons must be real Segoe MDL2 codepoints,
                            // or the palette falls back to tofu/emoji glyphs.
                            let icon_valid =
                                mdl2_codepoint(self.new_action_icon.trim()).is_some();
                            if !name.is_empty() && !icon_valid {
                                self.models_status_msg = Some((
                                    "Pick a Segoe MDL2 icon code, e.g. E70F (see the tooltip)."
                                        .to_string(),
                                    false,
                                ));
                            } else if !name.is_empty() {
                                let id = name.to_lowercase().replace(' ', "_");
                                let action = ActionItem {
                                    id,
                                    name,
                                    description: self.new_action_desc.trim().to_string(),
                                    icon: mdl2_codepoint(self.new_action_icon.trim())
                                        .map(|cp| char::from_u32(cp).unwrap().to_string())
                                        .unwrap_or_else(|| self.new_action_icon.trim().to_string()),
                                    // Positional: assigned by list order.
                                    shortcut: String::new(),
                                    prompt_instruction: self.new_action_prompt.trim().to_string(),
                                    default_instruction: self.new_action_prompt.trim().to_string(),
                                    is_builtin: false,
                                    is_rewrite_as: false,
                                };
                                store.add_custom_action(action);
                                self.new_action_dialog_open = false;
                            }
                        }

                        if spectrum_button(ui, Some(icons::CLOSE), "Cancel", false, theme)
                            .clicked()
                        {
                            self.new_action_dialog_open = false;
                        }
                    });
                });

            ui.add_space(10.0);
        }

        let mut reset_action_id: Option<String> = None;
        let mut delete_action_id: Option<String> = None;
        let mut move_action: Option<(usize, usize)> = None;
        let mut save_needed = false;

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 10.0);

                let actions = &mut store.settings.actions;
                let actions_total = actions.len();
                for (idx, action) in actions.iter_mut().enumerate() {
                    card_frame(theme, egui::Margin::same(14))
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
                                    spectrum_badge(ui, "Custom", theme.accent_badge_bg, theme.accent_badge_text);
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
                                        if spectrum_button(ui, Some(icons::REFRESH), "Reset to Default", false, theme).clicked() {
                                            reset_action_id = Some(action.id.clone());
                                        }
                                    }

                                    // Reorder controls: the list order is the
                                    // user's, and number keys follow position.
                                    if ui.add_enabled(
                                        idx > 0,
                                        egui::Button::new(
                                            RichText::new(icons::UP)
                                                .font(FontId::proportional(12.0))
                                                .color(theme.text_primary),
                                        )
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(Stroke::NONE),
                                    ).on_disabled_hover_text("Already at the top")
                                    .clicked()
                                    {
                                        move_action = Some((idx, idx - 1));
                                    }
                                    if ui.add_enabled(
                                        idx + 1 < actions_total,
                                        egui::Button::new(
                                            RichText::new(icons::DOWN)
                                                .font(FontId::proportional(12.0))
                                                .color(theme.text_primary),
                                        )
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(Stroke::NONE),
                                    ).on_disabled_hover_text("Already at the bottom")
                                    .clicked()
                                    {
                                        move_action = Some((idx, idx + 1));
                                    }

                                    if !action.shortcut.is_empty() {
                                        spectrum_badge(ui, &format!("Key: {}", action.shortcut), theme.surface_subtle, theme.text_primary);
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
        if let Some((from, to)) = move_action {
            store.move_action(from, to);
        }
        if save_needed {
            self.mark_settings_dirty();
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

            // The hotkey thread reports register failure here: a GUI app's
            // stderr is invisible, and a dead Alt+Q with no signal is
            // unfixable from the user's side.
            if self.hotkey_failed {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(icons::WARNING)
                            .font(FontId::proportional(14.0))
                            .color(theme.diff_del_text),
                    );
                    ui.label(
                        RichText::new("Alt+Q could not be registered - another app may be using it. Refined can still run from the tray.")
                            .font(FontId::proportional(12.0))
                            .color(theme.diff_del_text),
                    );
                });
            }

            ui.add_space(14.0);

            // Groq API Key Card
            card_frame(theme, egui::Margin::same(16))
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
                            self.mark_settings_dirty();
                        }

                        // Fixed 64px width for Show/Hide button to eliminate layout shifts
                        let toggle_text = if self.show_api_key {
                            format!("{} Hide", icons::EYE)
                        } else {
                            format!("{} Show", icons::EYE)
                        };
                        if ui.add_sized([64.0, 28.0], egui::Button::new(RichText::new(toggle_text).font(FontId::proportional(12.0)).color(theme.text_primary)).fill(theme.surface_elevated).stroke(Stroke::new(1.0, theme.border)).corner_radius(CornerRadius::same(4))).clicked() {
                            self.show_api_key = !self.show_api_key;
                        }

                        // Fixed 170px width for Fetch Models button
                        if ui.add_sized([170.0, 28.0], egui::Button::new(RichText::new(format!("{}  Fetch Models from Groq", icons::DOWNLOAD)).font(FontId::proportional(12.0)).color(Color32::WHITE).strong()).fill(theme.accent).stroke(Stroke::NONE).corner_radius(CornerRadius::same(4))).clicked() {
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
            card_frame(theme, egui::Margin::same(16))
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
                        let mut model_picked = false;

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
                                            model_picked = true;
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
                                            model_picked = true;
                                        }
                                    }
                                }
                            });
                        if model_picked {
                            self.mark_settings_dirty();
                        }
                    });

                    ui.add_space(8.0);

                    // Reasoning effort
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Reasoning Effort:").font(FontId::proportional(12.0)).color(theme.text_primary));
                        let current_effort = store.settings.groq.reasoning_effort.clone();
                        let mut effort_picked = false;

                        egui::ComboBox::from_id_salt("groq_reasoning_effort")
                            .selected_text(&current_effort)
                            .width(140.0)
                            .show_ui(ui, |ui| {
                                for effort in available_reasoning_efforts() {
                                    if ui.selectable_value(&mut store.settings.groq.reasoning_effort, effort.to_string(), effort).clicked() {
                                        effort_picked = true;
                                    }
                                }
                            });
                        if effort_picked {
                            self.mark_settings_dirty();
                        }

                        ui.add_space(10.0);
                        ui.label(
                            RichText::new("(Default: high; automatically falls back if model does not support reasoning)")
                                .font(FontId::proportional(12.0))
                                .color(theme.text_secondary),
                        );
                    });

                    ui.add_space(12.0);

                    // Sliders on a shared label column so they align at the
                    // same x; the value is shown next to each slider.
                    egui::Grid::new("groq_numeric_settings")
                        .num_columns(3)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new("Temperature:").font(FontId::proportional(12.0)).color(theme.text_primary));
                            let slider = spectrum_slider(ui, &mut store.settings.groq.temperature, 0.0..=2.0, 0.05, theme);
                            if slider.changed() {
                                self.mark_settings_dirty();
                            }
                            ui.label(
                                RichText::new(format!("{:.2}", store.settings.groq.temperature))
                                    .font(FontId::proportional(12.0))
                                    .color(theme.text_secondary),
                            );
                            ui.end_row();

                            ui.label(RichText::new("Top-P:").font(FontId::proportional(12.0)).color(theme.text_primary));
                            let slider = spectrum_slider(ui, &mut store.settings.groq.top_p, 0.0..=1.0, 0.05, theme);
                            if slider.changed() {
                                self.mark_settings_dirty();
                            }
                            ui.label(
                                RichText::new(format!("{:.2}", store.settings.groq.top_p))
                                    .font(FontId::proportional(12.0))
                                    .color(theme.text_secondary),
                            );
                            ui.end_row();
                        });

                    ui.add_space(8.0);

                    // Max Tokens (aligned to 32,768)
                    ui.label(
                        RichText::new(
                            "Max completion tokens: 32,768 (the optimal context limit)",
                        )
                        .font(FontId::proportional(12.0))
                        .color(theme.text_secondary),
                    );
                });

            ui.add_space(14.0);

            // Global System Prompt Card
            card_frame(theme, egui::Margin::same(16))
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
                            if spectrum_button(ui, Some(icons::REFRESH), "Reset to Default", false, theme)
                                .clicked()
                            {
                                store.settings.groq.global_system_prompt = GroqConfig::default().global_system_prompt;
                                self.mark_settings_dirty();
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
                        self.mark_settings_dirty();
                    }
                });

            ui.add_space(14.0);

            // Appearance & Platform Card
            card_frame(theme, egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    ui.label(
                        RichText::new("Appearance & Windows Integration")
                            .font(FontId::proportional(14.0))
                            .color(theme.text_primary)
                            .strong(),
                    );
                    ui.add_space(10.0);

                    // Theme selector: Spectrum segmented control - a quiet pill
                    // track with a filled active segment (no raw accent fill).
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Theme:").font(FontId::proportional(12.0)).color(theme.text_primary));
                        let is_dark = theme.is_dark;

                        let seg_w = 220.0;
                        let seg_h = 30.0;
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(seg_w, seg_h), Sense::hover());

                        // Track
                        ui.painter().rect_filled(
                            rect,
                            CornerRadius::same((seg_h / 2.0) as u8),
                            theme.surface_subtle,
                        );

                        let half = rect.split_left_right_at_x(rect.center().x);
                        let segments = [("Dark Mode", true), ("Light Mode", false)];

                        let mut picked: Option<ThemeMode> = None;
                        for (i, (label, is_dark_mode)) in segments.iter().enumerate() {
                            let seg_rect = if i == 0 { half.0 } else { half.1 };
                            let active = is_dark == *is_dark_mode;
                            let resp = ui
                                .interact(seg_rect, ui.scope_id().with(("theme_seg", i)), Sense::click())
                                .on_hover_text(if *is_dark_mode { "Dark theme" } else { "Light theme" });
                            if resp.clicked() {
                                picked = Some(if *is_dark_mode { ThemeMode::Dark } else { ThemeMode::Light });
                            }

                            if active {
                                // Filled segment: white in dark mode, white in
                                // light mode - macOS-style knob.
                                let inset = 2.0;
                                let fill_rect = Rect::from_min_max(
                                    Pos2::new(seg_rect.left() + inset, seg_rect.top() + inset),
                                    Pos2::new(seg_rect.right() - inset, seg_rect.bottom() - inset),
                                );
                                ui.painter().rect_filled(
                                    fill_rect,
                                    CornerRadius::same(((seg_h - inset * 2.0) / 2.0) as u8),
                                    Color32::WHITE,
                                );
                            }

                            let color = if active {
                                Color32::from_rgb(0x13, 0x13, 0x13)
                            } else {
                                theme.text_secondary
                            };
                            let galley = ui.painter().layout(
                                label.to_string(),
                                FontId::proportional(12.0),
                                color,
                                f32::INFINITY,
                            );
                            ui.painter().galley(
                                Pos2::new(
                                    seg_rect.center().x - galley.size().x / 2.0,
                                    seg_rect.center().y - galley.size().y / 2.0,
                                ),
                                galley,
                                Color32::WHITE,
                            );
                        }

                        if let Some(mode) = picked {
                            store.settings.theme_mode = mode;
                            self.mark_settings_dirty();
                        }
                    });

                    ui.add_space(10.0);

                    // Autostart sliding switch
                    ui.horizontal(|ui| {
                        let mut autostart_val = autostart::enabled();
                        if spectrum_switch(ui, &mut autostart_val, theme).changed() {
                            autostart::set(autostart_val);
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

/// Parses a Segoe MDL2 icon input (e.g. "E70F" or "\u{E70F}") into a u32
/// codepoint within the MDL2 private-use range; None when invalid.
fn mdl2_codepoint(input: &str) -> Option<u32> {
    let t = input
        .trim()
        .trim_start_matches("\\u{")
        .trim_start_matches("U+")
        .trim_start_matches("u+")
        .trim_end_matches('}');
    let cp = u32::from_str_radix(t, 16).ok()?;
    (0xE000..=0xF8FF).contains(&cp).then_some(cp)
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
    card_width: f32,
    theme: &SpectrumTheme,
) {
    card_frame(theme, egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            // Vertical body: this Frame can be shown inside a horizontal
            // parent, and egui frames inherit the parent layout.
            ui.vertical(|ui| {
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
                        .color(theme.text_primary)
                        .strong(),
                );
                ui.label(
                    RichText::new(subtitle)
                        .font(FontId::proportional(12.0))
                        .color(theme.text_tertiary),
                );
            });
        });
}
