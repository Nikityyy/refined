//! Refined Studio: History, Statistics, Action Customization, and Groq Settings.

use crate::actions::ActionItem;
use crate::autostart;
use crate::groq::{fetch_chat_models, GroqConfig, GroqModel, DEFAULT_MODEL};
use crate::history::HistoryEntry;
use crate::stats::calculate_stats;
use crate::store::{Store, ThemeMode};
use crate::theme::{card_frame, icons, render_diff_view, render_rf_logo, spectrum_badge, spectrum_button, spectrum_button_compact, spectrum_slider, spectrum_switch, SpectrumTheme};
use crate::updater::{self, UpdateState};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Key, Layout, Pos2, Rect, RichText,
    ScrollArea, Sense, Stroke, Ui, Vec2,
};

/// True when Ctrl is held and the given key was pressed this frame.
fn ctx_key(ui: &Ui, key: Key) -> bool {
    ui.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, key))
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
    expanded_history: Option<(String, Vec<crate::diff::DiffChunk>)>,
    catalog_key: String,
    catalog_error: Option<String>,
    models_rx: Option<Receiver<Result<Vec<GroqModel>, String>>>,
    pub show_api_key: bool,
    pub new_action_dialog_open: bool,
    pub new_action_name: String,
    pub new_action_desc: String,
    pub new_action_icon: &'static str,
    pub new_action_prompt: String,
    /// Set from the hotkey thread when the configured shortcut fails to register.
    pub hotkey_failed: bool,
    /// Pending settings save: UI edits mark it, and the next update flushes
    /// it at most ~half a second later. A write per keystroke or slider tick
    /// was a full create/flush/rename on the UI thread for nothing.
    pub settings_save_at: Option<std::time::Instant>,
    /// Cached aggregate stats keyed by (history length, newest entry id,
    /// today). Plain recalculation is O(n) with chunk scans and used to run
    /// every frame while the Statistics tab was open.
    pub stats_cache: Option<(usize, String, String, crate::stats::AggregatedStats)>,
    /// Update-check worker channel; None until the first drain.
    pub update_rx: Option<Receiver<UpdateState>>,
    pub update_tx: Sender<UpdateState>,
    /// Latest known update state (None = the check has not reported back).
    pub update_state: Option<UpdateState>,
    /// Set by the banner's "Restart now" button; the next update applies it.
    pub pending_restart: bool,
}

impl Studio {
    pub fn new() -> Self {
        let (update_tx, update_rx) = channel();
        Self {
            current_tab: StudioTab::History,
            history_search: String::new(),
            history_action_filter: "All".to_string(),
            expanded_history: None,
            catalog_key: String::new(),
            catalog_error: None,
            models_rx: None,
            show_api_key: false,
            new_action_dialog_open: false,
            new_action_name: String::new(),
            new_action_desc: String::new(),
            new_action_icon: icons::EDIT,
            new_action_prompt: String::new(),
            hotkey_failed: false,
            settings_save_at: None,
            stats_cache: None,
            update_rx: Some(update_rx),
            update_tx,
            update_state: None,
            pending_restart: false,
        }
    }

    pub fn check_for_updates(&mut self, ctx: &egui::Context) {
        if matches!(self.update_state, Some(UpdateState::Checking | UpdateState::Ready { .. })) {
            return;
        }
        self.update_state = Some(UpdateState::Checking);
        let tx = self.update_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let state = match updater::check_latest() {
                Ok(Some(info)) => match updater::download(&info) {
                    Ok(checksum) => UpdateState::Ready { version: info.version, checksum },
                    Err(error) => UpdateState::Failed(error),
                },
                Ok(None) => UpdateState::UpToDate,
                Err(error) => UpdateState::Failed(error),
            };
            if tx.send(state).is_ok() {
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            }
        });
    }

    pub fn sync_updates(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.update_rx {
            while let Ok(state) = rx.try_recv() {
                self.update_state = Some(state);
            }
        }
        if self.update_state.is_none() {
            self.check_for_updates(ctx);
        }
    }

    pub fn restart_for_update(&mut self, store: &mut Store, hud_open: bool) -> bool {
        if !std::mem::take(&mut self.pending_restart) || hud_open {
            return false;
        }
        self.flush_pending_now(store);
        let Some(UpdateState::Ready { checksum, .. }) = &self.update_state else { return false };
        match updater::apply_staged(checksum) {
            Ok(()) => true,
            Err(error) => {
                self.update_state = Some(UpdateState::Failed(error));
                false
            }
        }
    }

    fn fetch_models(&mut self, ctx: &egui::Context, key: &str) {
        if key.is_empty() || self.models_rx.is_some() {
            return;
        }
        self.catalog_key = key.to_owned();
        self.catalog_error = None;
        let (tx, rx) = channel();
        self.models_rx = Some(rx);
        let ctx = ctx.clone();
        let key = key.to_owned();
        std::thread::spawn(move || {
            if tx.send(fetch_chat_models(&key)).is_ok() {
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            }
        });
    }

    pub fn sync_models(&mut self, ctx: &egui::Context, config: &mut GroqConfig) {
        let key = config.api_key.trim();
        if self.catalog_key != key {
            config.models = Default::default();
            self.models_rx = None;
            self.catalog_error = None;
            if key.is_empty() {
                self.catalog_key.clear();
            } else if let Some(delay) = self.settings_save_at.and_then(|at| {
                std::time::Duration::from_millis(450).checked_sub(at.elapsed())
            }) {
                ctx.request_repaint_after(delay);
            } else {
                self.fetch_models(ctx, key);
            }
        }

        let Some(rx) = &self.models_rx else { return };
        match rx.try_recv() {
            Ok(Ok(models)) => {
                if models.is_empty() {
                    self.catalog_error = Some("Groq returned no chat models for this key.".into());
                }
                config.models = models.into();
                self.models_rx = None;
            }
            Ok(Err(error)) => {
                self.catalog_error = Some(error);
                self.models_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.catalog_error = Some("Model lookup ended unexpectedly.".into());
                self.models_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
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

    /// Persists a pending settings save immediately (tab switches, app
    /// shutdown) - the debounce only exists to coalesce rapid edit bursts.
    pub fn flush_pending_now(&mut self, store: &mut Store) {
        if self.take_pending_save() {
            store.save_settings();
        }
    }

    /// Takes a pending mark, if any; the caller then persists. Used when the
    /// Studio hides or the app exits, before the debounce window would have
    /// elapsed in `update`.
    pub fn take_pending_save(&mut self) -> bool {
        self.settings_save_at.take().is_some()
    }

    /// The update banner body. Callers wrap it in the quiet alert style.
    fn render_update_banner(&mut self, ui: &mut Ui, theme: &SpectrumTheme, msg: &str, hud_open: bool) {
        egui::Frame::new()
            .fill(theme.accent_badge_bg)
            .stroke(Stroke::new(1.0, theme.border))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(icons::DOWNLOAD)
                            .font(FontId::proportional(13.0))
                            .color(theme.accent_badge_text),
                    );
                    ui.label(
                        RichText::new(msg)
                            .font(FontId::proportional(12.0))
                            .color(theme.accent_badge_text),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let restart = ui.add_enabled_ui(!hud_open, |ui| {
                            spectrum_button(ui, Some(icons::REFRESH), "Restart now", true, theme)
                        }).inner.on_disabled_hover_text("Close the edit palette to restart.");
                        if restart.clicked() {
                            self.pending_restart = true;
                        }
                    });
                });
            });
        ui.add_space(8.0);
    }

    /// Update status row for the Settings "Updates" card.
    fn render_update_status(&mut self, ui: &mut Ui, theme: &SpectrumTheme) {
        ui.horizontal(|ui| {
            let (line, color) = match &self.update_state {
                None | Some(UpdateState::Checking) => {
                    ("Checking for updates...".to_string(), theme.text_secondary)
                }
                Some(UpdateState::UpToDate) => (
                    format!("You are on the latest version ({}).", env!("CARGO_PKG_VERSION")),
                    theme.diff_add_text,
                ),
                Some(UpdateState::Ready { version, .. }) => (
                    format!("Version {version} is ready to install."),
                    theme.text_primary,
                ),
                Some(UpdateState::Failed(e)) => (e.clone(), theme.diff_del_text),
            };
            ui.label(
                RichText::new(line)
                    .font(FontId::proportional(12.0))
                    .color(color),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let busy = matches!(self.update_state, Some(UpdateState::Checking | UpdateState::Ready { .. }));
                if ui.add_enabled(!busy, egui::Button::new(RichText::new("Check again").font(FontId::proportional(12.0)).color(Color32::WHITE)).fill(theme.accent).stroke(Stroke::NONE).corner_radius(CornerRadius::same(4))).clicked() {
                    self.check_for_updates(ui.ctx());
                }
            });
        });
    }

    pub fn update(&mut self, ui: &mut Ui, store: &mut Store, theme: &SpectrumTheme, hud_open: bool) {
        self.flush_settings(store);
        store.load_history();

        ui.set_width(ui.available_width());

        // Keyboard navigation: Ctrl+1..4 jump between tabs. Leaving an
        // edit-heavy tab persists edits at once, not after the debounce.
        {
            let pressed = |key: Key| ctx_key(ui, key);
            let mut picked: Option<StudioTab> = None;
            if pressed(Key::Num1) && self.current_tab != StudioTab::History {
                picked = Some(StudioTab::History);
            }
            if pressed(Key::Num2) && self.current_tab != StudioTab::Statistics {
                picked = Some(StudioTab::Statistics);
            }
            if pressed(Key::Num3) && self.current_tab != StudioTab::Actions {
                picked = Some(StudioTab::Actions);
            }
            if pressed(Key::Num4) && self.current_tab != StudioTab::Settings {
                picked = Some(StudioTab::Settings);
            }
            if let Some(tab) = picked {
                self.flush_pending_now(store);
                self.current_tab = tab;
            }
        }

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
            render_rf_logo(ui, 26.0);
            ui.label(RichText::new("Refined").size(16.0).strong().color(theme.text_primary));
            ui.add_space(16.0);

            let tabs = [
                (StudioTab::History, "History", "Ctrl+1"),
                (StudioTab::Statistics, "Statistics", "Ctrl+2"),
                (StudioTab::Actions, "Actions", "Ctrl+3"),
                (StudioTab::Settings, "Settings", "Ctrl+4"),
            ];

            for (tab, label, shortcut) in tabs {
                let active = self.current_tab == tab;
                let response = ui.add(
                    egui::Button::new(RichText::new(label).size(13.0).color(
                        if active { theme.text_primary } else { theme.text_secondary }
                    ))
                    .frame_when_inactive(false)
                    .min_size(Vec2::new(68.0, 32.0)),
                ).on_hover_text(shortcut);

                if active {
                    let bottom = response.rect.bottom() - 1.0;
                    ui.painter().line_segment(
                        [Pos2::new(response.rect.left() + 8.0, bottom),
                         Pos2::new(response.rect.right() - 8.0, bottom)],
                        Stroke::new(2.0, theme.text_primary),
                    );
                }
                if response.clicked() && !active {
                    self.flush_pending_now(store);
                    self.current_tab = tab;
                }
            }
        });
        ui.add_space(3.0);
        ui.separator();
        ui.add_space(9.0);

        if let Some(UpdateState::Ready { version, .. }) = &self.update_state {
            self.render_update_banner(ui, theme, &format!("Refined {version} is ready to install."), hud_open);
        }

        if self.current_tab != StudioTab::History {
            self.expanded_history = None;
        }
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

                // Search box with fixed size, a clear button, and a leading
                // search icon so the field is findable at a glance.
                if !self.history_search.is_empty() {
                    if ui.add_sized([24.0, 26.0], egui::Button::new(RichText::new(icons::CLEAR).font(FontId::proportional(11.0)).color(theme.text_secondary)).fill(Color32::TRANSPARENT).stroke(Stroke::NONE)).on_hover_text("Clear search").clicked() {
                        self.history_search.clear();
                    }
                }
                ui.add_sized(
                    [220.0, 26.0],
                    egui::TextEdit::singleline(&mut self.history_search)
                        .align(Align2::LEFT_CENTER)
                        .hint_text("Search refinements..."),
                );
                ui.label(
                    RichText::new(icons::SEARCH)
                        .font(FontId::proportional(12.0))
                        .color(theme.text_secondary),
                );
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
                    RichText::new(format!(
                        "Select text, press {}+{}, and pick an action.",
                        store.settings.hotkey.modifier, store.settings.hotkey.key
                    ))
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
                    let diff = self.expanded_history.as_ref()
                        .filter(|(id, _)| id == &entry.id).map(|(_, chunks)| chunks);
                    let is_expanded = diff.is_some();

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
                                // Include the year only once entries leave it:
                                // "%b %d" is ambiguous after a few months.
                                let now_year = chrono::Local::now().format("%Y").to_string();
                                let ts_fmt = if entry.timestamp.format("%Y").to_string() == now_year {
                                    "%b %d, %H:%M"
                                } else {
                                    "%b %d, %Y, %H:%M"
                                };
                                ui.label(
                                    RichText::new(entry.timestamp.format(ts_fmt).to_string())
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
                                    if ui.add_sized([26.0, 22.0], egui::Button::new(RichText::new(icons::TRASH).color(theme.diff_del_text)).fill(Color32::TRANSPARENT).stroke(Stroke::NONE)).on_hover_text("Delete this entry").clicked() {
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
                                        crate::clipboard::copy_refinement(&entry.refined_text);
                                    }
                                });
                            });

                            ui.add_space(4.0);

                            // Content preview
                            if let Some(chunks) = diff {
                                ui.label(RichText::new("Original text").font(FontId::proportional(12.0)).color(theme.text_secondary));
                                ui.label(RichText::new(crate::rich_text::readable(&entry.original_text)).font(FontId::proportional(13.0)).color(theme.text_primary));
                                ui.add_space(6.0);

                                ui.label(RichText::new("Changes").font(FontId::proportional(12.0)).color(theme.accent).strong());
                                egui::Frame::new()
                                    .fill(theme.surface_elevated)
                                    .stroke(Stroke::new(1.0, theme.border))
                                    .corner_radius(CornerRadius::same(4))
                                    .inner_margin(egui::Margin::same(8))
                                    .show(ui, |ui| {
                                        render_diff_view(ui, chunks, theme);
                                    });
                            } else {
                                // Char-boundary-safe preview: raw byte slicing
                                // would panic on multi-byte UTF-8 (umlauts, CJK).
                                let preview = truncate_chars(&crate::rich_text::readable(&entry.refined_text), 140);
                                ui.label(RichText::new(preview).font(FontId::proportional(13.0)).color(theme.text_primary));
                            }
                        });
                }
            });

        if let Some(id) = delete_id {
            store.delete_history_entry(&id);
            if self.expanded_history.as_ref().is_some_and(|(shown, _)| shown == &id) {
                self.expanded_history = None;
            }
        }
        if let Some(id) = toggle_diff_id {
            if self.expanded_history.as_ref().is_some_and(|(shown, _)| shown == &id) {
                self.expanded_history = None;
            } else if let Some(entry) = store.history.iter().find(|entry| entry.id == id) {
                self.expanded_history = Some((id, entry.diff().0));
            }
        }
    }

    fn render_statistics_tab(&mut self, ui: &mut Ui, store: &mut Store, theme: &SpectrumTheme) {
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
                let stats = calculate_stats(&mut store.history);
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
        // inner_margin(16) painted on both sides PLUS the 1px frame stroke
        // on each side: 34 px per card. This must track the metric card's
        // own frame padding - when the two drift apart the metric row
        // overflows to the window edge while the dashboard row below sits
        // inside it, and the right edges visibly fail to stack.
        let side_margin = 34.0;
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
                "Est. editing time saved",
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
                        ui.set_min_height(200.0);

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
                                RichText::new("Refinements will appear here after you run an action.")
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
                        ui.set_min_height(200.0);

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

                        // Beyond 7 rows the tail rolls into "Other" so the
                        // percentages still sum to 100 instead of silently
                        // dropping actions.
                        if rows.len() > 8 {
                            let other: usize = rows.iter().skip(7).map(|(_, v)| v).sum();
                            rows.truncate(7);
                            rows.push(("Other".to_string(), other));
                        }

                        for (action_name, count) in rows.iter() {
                            let pct = (*count as f32 / total) * 100.0;
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(action_name).font(FontId::proportional(12.0)).color(theme.text_primary));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(RichText::new(format!("{count} ({pct:.0}%)")).font(FontId::proportional(12.0)).color(theme.text_secondary));
                                });
                            });

                            // At 0% ProgressBar still paints a minimum-width
                            // fill; filling with the track color keeps an
                            // empty row truly empty.
                            let track = ui.style().visuals.extreme_bg_color;
                            ui.add(
                                egui::ProgressBar::new(pct / 100.0)
                                    .fill(if pct > 0.0 { theme.accent } else { track })
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
                    self.new_action_icon = icons::EDIT;
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
                .inner_margin(egui::Margin::same(16))
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
                    });

                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Icon:").font(FontId::proportional(12.0)).color(theme.text_secondary));
                        for icon in crate::theme::ACTION_ICONS {
                            let label = RichText::new(icon).font(FontId::proportional(14.0)).color(theme.text_primary);
                            if ui.selectable_label(self.new_action_icon == icon, label).clicked() {
                                self.new_action_icon = icon;
                            }
                        }
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

                    let name = self.new_action_name.trim();
                    let id = name.to_lowercase().replace(' ', "_");
                    let duplicate = store.settings.actions.iter().any(|action| action.id == id);
                    let prompt = self.new_action_prompt.trim();
                    let error = if duplicate {
                        Some("An action with this name already exists.")
                    } else {
                        None
                    };
                    if let Some(error) = error {
                        ui.label(RichText::new(error).size(12.0).color(theme.diff_del_text));
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let save = ui.add_enabled_ui(error.is_none() && !name.is_empty() && !prompt.is_empty(), |ui| {
                            spectrum_button(ui, Some(icons::SAVE), "Save action", true, theme)
                        }).inner;
                        if save.clicked() {
                            store.add_custom_action(ActionItem {
                                id,
                                name: name.into(),
                                description: self.new_action_desc.trim().into(),
                                icon: self.new_action_icon.into(),
                                shortcut: String::new(),
                                prompt_instruction: prompt.into(),
                                default_instruction: prompt.into(),
                                is_builtin: false,
                                is_rewrite_as: false,
                            });
                            self.new_action_dialog_open = false;
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
                let has_pinned_action = actions.last().is_some_and(ActionItem::is_pinned);
                let movable_count = actions.len() - usize::from(has_pinned_action);
                for (idx, action) in actions.iter_mut().enumerate() {
                    card_frame(theme, egui::Margin::same(12))
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
                                        if ui.add_sized([64.0, 24.0], egui::Button::new(RichText::new(format!("{} Delete", icons::TRASH)).color(theme.diff_del_text)).fill(Color32::TRANSPARENT).stroke(Stroke::NONE)).on_hover_text("Delete this custom action").clicked() {
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
                                        idx > 0 && idx < movable_count,
                                        egui::Button::new(
                                            RichText::new(icons::UP)
                                                .font(FontId::proportional(12.0))
                                                .color(theme.text_primary),
                                        )
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(Stroke::NONE),
                                    ).on_hover_text("Move up")
                                    .on_disabled_hover_text(if action.is_pinned() { "Rewrite as always stays last" } else { "Already at the top" })
                                    .clicked()
                                    {
                                        move_action = Some((idx, idx - 1));
                                    }
                                    if ui.add_enabled(
                                        idx + 1 < movable_count,
                                        egui::Button::new(
                                            RichText::new(icons::DOWN)
                                                .font(FontId::proportional(12.0))
                                                .color(theme.text_primary),
                                        )
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(Stroke::NONE),
                                    ).on_hover_text("Move down")
                                    .on_disabled_hover_text(if has_pinned_action { "Rewrite as always stays last" } else { "Already at the bottom" })
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
                RichText::new("Choose a model for your study notes and customize Refined.")
                    .font(FontId::proportional(12.0))
                    .color(theme.text_secondary),
            );

            // The hotkey thread reports register failure here: a GUI app's
            // stderr is invisible, and a dead hotkey with no signal is
            // unfixable from the user's side.
            if self.hotkey_failed {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(icons::WARNING)
                            .font(FontId::proportional(14.0))
                            .color(theme.diff_del_text),
                    );
                    let combo = format!(
                        "{}+{}",
                        store.settings.hotkey.modifier, store.settings.hotkey.key
                    );
                    ui.label(
                        RichText::new(format!(
                            "{combo} could not be registered - another app may be using it. Pick a different shortcut below, or run Refined from the tray."
                        ))
                        .font(FontId::proportional(12.0))
                        .color(theme.diff_del_text),
                    );
                });
            }

            // Wayland compositors do not deliver global key grabs to apps.
            // A desktop shortcut that runs `--trigger` opens the palette.
            #[cfg(target_os = "linux")]
            if std::env::var_os("WAYLAND_DISPLAY").is_some() {
                let command = std::env::current_exe()
                    .map(|exe| format!("\"{}\" --trigger", exe.display()))
                    .unwrap_or_else(|_| "refined --trigger".into());
                ui.add_space(8.0);
                ui.label(
                    RichText::new("On Wayland, add a keyboard shortcut in your desktop settings that runs this command:")
                        .font(FontId::proportional(12.0))
                        .color(theme.text_secondary),
                );
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&command).font(FontId::monospace(12.0)).color(theme.text_primary));
                    if spectrum_button_compact(ui, Some(icons::COPY), "Copy", false, theme).clicked() {
                        crate::clipboard::set_clipboard_text(&command);
                    }
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
                        RichText::new("Paste a Groq API key to load the chat models available to your account.")
                            .font(FontId::proportional(12.0))
                            .color(theme.text_secondary),
                    );
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        // Fixed width buttons so Show/Hide NEVER jitters container width!
                        let input_w = (ui.available_width() - 214.0).max(200.0);
                        let edit = if self.show_api_key {
                            ui.add_sized([input_w, 28.0], egui::TextEdit::singleline(&mut store.settings.groq.api_key).align(Align2::LEFT_CENTER))
                        } else {
                            ui.add_sized([input_w, 28.0], egui::TextEdit::singleline(&mut store.settings.groq.api_key).align(Align2::LEFT_CENTER).password(true))
                        };

                        // Right-click menu on the key field: long opaque keys
                        // are pasted, never typed, so paste/copy/clear operate
                        // on the whole value.
                        let mut key_edited = false;
                        edit.context_menu(|ui| {
                            let clipboard = crate::clipboard::get_clipboard_text().unwrap_or_default();
                            if ui
                                .add_enabled(
                                    !clipboard.trim().is_empty(),
                                    egui::Button::new("Paste from clipboard"),
                                )
                                .clicked()
                            {
                                store.settings.groq.api_key = clipboard.trim().to_string();
                                key_edited = true;
                                ui.close();
                            }
                            if ui
                                .add_enabled(
                                    !store.settings.groq.api_key.is_empty(),
                                    egui::Button::new("Copy key"),
                                )
                                .clicked()
                            {
                                crate::clipboard::set_clipboard_text(&store.settings.groq.api_key);
                                ui.close();
                            }
                            if ui
                                .add_enabled(
                                    !store.settings.groq.api_key.is_empty(),
                                    egui::Button::new("Clear"),
                                )
                                .clicked()
                            {
                                store.settings.groq.api_key.clear();
                                key_edited = true;
                                ui.close();
                            }
                        });
                        if key_edited || edit.changed() {
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

                        ui.add_enabled_ui(
                            self.models_rx.is_none() && !store.settings.groq.api_key.trim().is_empty(),
                            |ui| {
                                if spectrum_button_compact(ui, Some(icons::REFRESH), "Refresh models", false, theme).clicked() {
                                    self.fetch_models(ui.ctx(), store.settings.groq.api_key.trim());
                                }
                            },
                        );
                    });

                    ui.add_space(6.0);
                    if let Some(error) = &self.catalog_error {
                        ui.label(RichText::new(error).size(12.0).color(theme.diff_del_text));
                    } else {
                        let status = if self.models_rx.is_some() {
                            "Checking Groq for available models".to_owned()
                        } else if store.settings.groq.models.is_empty() {
                            "Enter your API key to load models.".to_owned()
                        } else {
                            format!("{} chat models available", store.settings.groq.models.len())
                        };
                        ui.label(RichText::new(status).size(12.0).color(theme.text_secondary));
                    }
                });

            ui.add_space(14.0);

            card_frame(theme, egui::Margin::same(16)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let config = &mut store.settings.groq;
                let models = config.models.clone();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Model behavior").size(15.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        for (label, model, detail) in [
                            ("Study preset", "openai/gpt-oss-120b", "GPT-OSS 120B with low reasoning. Best balanced result in the study benchmark."),
                            ("Quick edits", DEFAULT_MODEL, "GPT-OSS 20B with low reasoning. A faster option for short edits."),
                        ] {
                            ui.add_enabled_ui(models.iter().any(|entry| entry.id == model), |ui| {
                                if spectrum_button_compact(ui, None, label, false, theme).on_hover_text(detail).clicked() {
                                    config.model = model.into();
                                    config.reasoning_effort = "low".into();
                                    self.mark_settings_dirty();
                                }
                            });
                        }
                    });
                });
                ui.add_space(10.0);

                egui::Grid::new("groq_model_settings").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    ui.label("Model");
                    let model_label = config.selected_model().map_or(config.model.as_str(), GroqModel::label);
                    let mut picked = None;
                    ui.add_enabled_ui(!models.is_empty(), |ui| {
                        egui::ComboBox::from_id_salt("groq_model_select").selected_text(model_label)
                            .width(240.0).show_ui(ui, |ui| {
                                for model in models.iter() {
                                    if ui.selectable_label(config.model == model.id, model.label())
                                        .on_hover_text(&model.id).clicked()
                                    {
                                        picked = Some(model.id.clone());
                                    }
                                }
                            });
                    });
                    if let Some(model) = picked {
                        config.model = model;
                        // An effort the new model does not offer falls back
                        // to the model default, which is Medium for GPT-OSS.
                        let options = config.reasoning_options();
                        if !options.contains(&config.reasoning_effort.as_str()) && options.contains(&"low") {
                            config.reasoning_effort = "low".into();
                        }
                        self.mark_settings_dirty();
                    }
                    ui.end_row();

                    ui.label("Reasoning");
                    let options = config.reasoning_options();
                    let active = options.contains(&config.reasoning_effort.as_str());
                    if options.is_empty() {
                        ui.label(RichText::new("Managed by model").color(theme.text_secondary));
                    } else {
                        let effort_label = |effort: &str| match effort {
                            "none" => "Off", "default" => "Model default", "low" => "Low",
                            "medium" => "Medium", "high" => "High", "xhigh" => "Extra high",
                            "minimal" => "Minimal", "max" => "Maximum", _ => "Custom",
                        };
                        let mut picked = None;
                        egui::ComboBox::from_id_salt("groq_reasoning_effort")
                            .selected_text(if active { effort_label(&config.reasoning_effort) } else { "Model default" })
                            .width(240.0).show_ui(ui, |ui| {
                                for effort in options {
                                    if ui.selectable_label(effort == config.reasoning_effort, effort_label(effort)).clicked() {
                                        picked = Some(effort.to_owned());
                                    }
                                }
                            });
                        if let Some(effort) = picked {
                            config.reasoning_effort = effort;
                            self.mark_settings_dirty();
                        }
                    }
                    ui.end_row();

                    ui.label("Output limit");
                    let limit = config.selected_model().and_then(GroqModel::completion_limit);
                    ui.horizontal(|ui| {
                        let mut auto = config.max_completion_tokens == 0;
                        let auto_label = limit.map_or("Model maximum".into(), |value| format!("Model maximum ({} tokens)", fmt_thousands(value as usize)));
                        if ui.checkbox(&mut auto, auto_label).changed() {
                            config.max_completion_tokens = if auto { 0 } else { limit.unwrap_or(4096).min(4096) };
                            config.output_limit_edited = true;
                            self.mark_settings_dirty();
                        }
                        if !auto {
                            let mut cap = config.request_cap().unwrap_or(4096);
                            if ui.add(egui::DragValue::new(&mut cap).range(1..=limit.unwrap_or(131_072u32)).suffix(" tokens")).changed() {
                                config.max_completion_tokens = cap;
                                config.output_limit_edited = true;
                                self.mark_settings_dirty();
                            }
                        }
                    });
                    ui.end_row();
                });

                if !models.is_empty() && config.selected_model().is_none() {
                    ui.label(RichText::new("Your saved model is unavailable. Choose one from the list.")
                        .size(12.0).color(theme.diff_del_text));
                }
                ui.add_space(8.0);
                ui.collapsing("Sampling settings", |ui| {
                    egui::Grid::new("groq_numeric_settings").num_columns(3).spacing([12.0, 8.0]).show(ui, |ui| {
                        for (label, value, max) in [
                            ("Temperature", &mut config.temperature, 2.0),
                            ("Top P", &mut config.top_p, 1.0),
                        ] {
                            ui.label(label);
                            if spectrum_slider(ui, value, 0.0..=max, 0.05, theme).changed() {
                                self.mark_settings_dirty();
                            }
                            ui.label(RichText::new(format!("{value:.2}")).color(theme.text_secondary));
                            ui.end_row();
                        }
                    });
                });
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
                        RichText::new(format!("Appearance & {OS_NAME} Integration"))
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
                                // Filled segment: white with a hairline
                                // border - borderless white vanished on the
                                // light mode track.
                                let inset = 2.0;
                                let fill_rect = Rect::from_min_max(
                                    Pos2::new(seg_rect.left() + inset, seg_rect.top() + inset),
                                    Pos2::new(seg_rect.right() - inset, seg_rect.bottom() - inset),
                                );
                                let fill_radius = CornerRadius::same(((seg_h - inset * 2.0) / 2.0) as u8);
                                ui.painter().rect_filled(fill_rect, fill_radius, Color32::WHITE);
                                ui.painter().rect_stroke(
                                    fill_rect,
                                    fill_radius,
                                    Stroke::new(1.0, theme.border),
                                    egui::StrokeKind::Inside,
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
                        ui.label(RichText::new(format!("Start Refined automatically with {OS_NAME}")).color(theme.text_primary));
                    });

                    ui.add_space(10.0);

                    // Global shortcut picker. Changes apply instantly: the
                    // hotkey thread re-registers when it sees the new combo.
                    ui.label(
                        RichText::new("Global Shortcut")
                            .font(FontId::proportional(13.0))
                            .color(theme.text_primary)
                            .strong(),
                    );
                    ui.horizontal(|ui| {
                        let cur_mod = store.settings.hotkey.modifier.clone();
                        let cur_key = store.settings.hotkey.key.clone();

                        egui::ComboBox::from_id_salt("hotkey_modifier")
                            .selected_text(cur_mod.clone())
                            .width(110.0)
                            .show_ui(ui, |ui| {
                                let mut sel: Option<&'static str> = None;
                                for m in crate::store::HOTKEY_MODIFIERS {
                                    ui.selectable_value(&mut sel, Some(*m), *m);
                                }
                                if let Some(m) = sel {
                                    if m != cur_mod {
                                        store.settings.hotkey.modifier = m.to_string();
                                        self.mark_settings_dirty();
                                    }
                                }
                            });

                        ui.label(
                            RichText::new("+")
                                .color(theme.text_secondary)
                                .strong(),
                        );

                        egui::ComboBox::from_id_salt("hotkey_key")
                            .selected_text(cur_key.clone())
                            .width(90.0)
                            .show_ui(ui, |ui| {
                                let mut sel: Option<&'static str> = None;
                                for k in crate::store::HOTKEY_KEY_CHOICES {
                                    ui.selectable_value(&mut sel, Some(*k), *k);
                                }
                                if let Some(k) = sel {
                                    if k != cur_key {
                                        store.settings.hotkey.key = k.to_string();
                                        self.mark_settings_dirty();
                                    }
                                }
                            });
                    });
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Triggers only when text is actively selected. Applies instantly; no restart needed.")
                            .font(FontId::proportional(12.0))
                            .color(theme.text_secondary),
                    );
                });

            ui.add_space(14.0);

            // Updates Card: current version and the self-updater state.
            // Updating never runs an installer - see the banner at the top.
            card_frame(theme, egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    ui.label(
                        RichText::new("Updates")
                            .font(FontId::proportional(14.0))
                            .color(theme.text_primary)
                            .strong(),
                    );
                    ui.label(
                        RichText::new("Checks GitHub automatically on launch. Updates download in the background and swap in without an installer; settings and history are untouched.")
                            .font(FontId::proportional(12.0))
                            .color(theme.text_secondary),
                    );
                    ui.add_space(8.0);
                    self.render_update_status(ui, theme);
                });

            ui.add_space(20.0);
        });
    }
}

/// codepoint within the MDL2 private-use range; None when invalid.
const OS_NAME: &str = if cfg!(windows) { "Windows" } else { "Linux" };

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

/// Truncates to at most `max_chars` characters, cutting back to the last
/// whitespace so the ellipsis never glues onto a half-word, and never
/// splitting a multi-byte character.
fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let cut: String = text.chars().take(max_chars).collect();
    match cut.rfind(char::is_whitespace) {
        Some(i) => format!("{}...", &cut[..i]),
        None => format!("{}...", cut.trim_end()),
    }
}

fn render_metric_card(
    ui: &mut Ui,
    title: &str,
    value: &str,
    subtitle: &str,
    card_width: f32,
    theme: &SpectrumTheme,
) {
    // Compact height: at 96px tall the stack filled only ~2/3 with the
    // leftover pooled at the bottom, so the cards read as oversized blocks.
    card_frame(theme, egui::Margin::symmetric(16, 10))
        .show(ui, |ui| {
            // Vertical body: this Frame can be shown inside a horizontal
            // parent, and egui frames inherit the parent layout.
            ui.vertical(|ui| {
                ui.set_width(card_width);
                ui.set_min_height(58.0);
                ui.spacing_mut().item_spacing.y = 4.0;

                ui.label(
                    RichText::new(title)
                        .font(FontId::proportional(12.0))
                        .color(theme.text_secondary)
                        .strong(),
                );
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

#[cfg(test)]
mod tests {
    use super::{fmt_thousands, truncate_chars};

    #[test]
    fn thousands_separators() {
        assert_eq!(fmt_thousands(0), "0");
        assert_eq!(fmt_thousands(999), "999");
        assert_eq!(fmt_thousands(1000), "1,000");
        assert_eq!(fmt_thousands(1234567), "1,234,567");
    }

    #[test]
    fn truncation_cuts_at_word_boundaries() {
        let text = "The replacement of human editors is unlikely.";
        let out = truncate_chars(text, 12);
        assert!(out.ends_with("..."));
        assert!(!out.trim_end_matches('.').contains("replacemen"));
        // Short input passes through untouched.
        assert_eq!(truncate_chars("short", 20), "short");
    }
}
