//! Alt+Q Contextual HUD: Action palette, inline custom prompt, processing state, and diff viewer.

use crate::actions::{standard_languages, ActionItem};
use crate::diff::{compute_diff, DiffChunk, DiffStats};
use crate::groq::{complete_action, GroqConfig};
use crate::history::HistoryEntry;
use crate::theme::{icons, kbd_button, render_diff_view, spectrum_badge, spectrum_button, SpectrumTheme};
use eframe::egui::{self, Color32, CornerRadius, FontId, Key, Pos2, RichText, Sense, Stroke, Ui};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};

#[derive(Debug, Clone, PartialEq)]
pub enum HudState {
    SelectingAction,
    RewriteAsInput { action_idx: usize },
    Processing { action_name: String },
    DiffView {
        original_text: String,
        refined_text: String,
        diff_chunks: Vec<DiffChunk>,
        stats: DiffStats,
        action_name: String,
        model: String,
        duration_ms: u128,
    },
}

pub enum HudWorkerMsg {
    Success {
        refined_text: String,
        diff_chunks: Vec<DiffChunk>,
        stats: DiffStats,
        action_name: String,
        action_id: String,
        model: String,
        duration_ms: u128,
        language: String,
    },
    Error(String),
}

pub struct Hud {
    pub is_visible: bool,
    pub state: HudState,
    pub original_text: String,
    pub target_hwnd: isize,
    pub selected_index: usize,
    pub custom_rewrite_input: String,
    pub error_message: Option<String>,
    pub position: Pos2,
    pub worker_rx: Option<Receiver<HudWorkerMsg>>,
    pub worker_tx: Sender<HudWorkerMsg>,
    pub just_opened: bool,
    /// Demo mode: do not auto-dismiss on focus loss (so the UI can be inspected).
    pub demo_sticky: bool,
    /// Generation token: bumped whenever an old run must be invalidated
    /// (HUD re-opened, closed, or a new action triggered). A worker whose
    /// generation is stale must never paste into the user's window.
    generation: Arc<AtomicU64>,
}

/// The effective output language for a run: the custom language whenever the
/// "Custom..." entry is selected, otherwise the selected standard language.
fn resolved_language<'a>(selected: &'a str, custom: &'a str) -> &'a str {
    if selected == "custom" {
        custom
    } else {
        selected
    }
}

/// Computes multi-monitor clamped HUD position using Win32 monitor coordinates.
pub fn compute_clamped_hud_pos(cursor_x: i32, cursor_y: i32, width: f32, height: f32) -> Pos2 {
    let pt = POINT { x: cursor_x, y: cursor_y };
    let mut min_x = 0;
    let mut max_x = 1920;
    let mut min_y = 0;
    let mut max_y = 1080;

    unsafe {
        let hmon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        if !hmon.0.is_null() {
            let mut mi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(hmon, &mut mi).as_bool() {
                min_x = mi.rcWork.left;
                max_x = mi.rcWork.right;
                min_y = mi.rcWork.top;
                max_y = mi.rcWork.bottom;
            }
        }
    }

    let desired_x = (cursor_x - 20) as f32;
    let desired_y = (cursor_y + 15) as f32;

    let clamped_x = desired_x.clamp((min_x + 12) as f32, (max_x - 12) as f32 - width);
    let clamped_y = desired_y.clamp((min_y + 12) as f32, (max_y - 12) as f32 - height);

    Pos2::new(clamped_x, clamped_y)
}

impl Hud {
    pub fn new() -> Self {
        let (worker_tx, worker_rx) = channel();
        Self {
            is_visible: false,
            state: HudState::SelectingAction,
            original_text: String::new(),
            target_hwnd: 0,
            selected_index: 0,
            custom_rewrite_input: String::new(),
            error_message: None,
            position: Pos2::new(200.0, 200.0),
            worker_rx: Some(worker_rx),
            worker_tx,
            just_opened: false,
            demo_sticky: false,
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn open(&mut self, text: String, x: i32, y: i32, target_hwnd: isize) {
        // Invalidate any in-flight run from a previous HUD session.
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.original_text = text;
        self.target_hwnd = target_hwnd;
        self.is_visible = true;
        self.state = HudState::SelectingAction;
        self.selected_index = 0;
        self.custom_rewrite_input.clear();
        self.error_message = None;
        self.just_opened = true;
        // Clamp against the tallest state the HUD can grow into (the diff
        // view is 430 px); a stale 360 px floor let the last rows of the
        // diff sit below the work area.
        self.position = compute_clamped_hud_pos(x, y, 480.0, 430.0);
    }

    pub fn close(&mut self) {
        self.is_visible = false;
        self.state = HudState::SelectingAction;
        self.error_message = None;
        // Invalidate any in-flight worker so its completion is dropped.
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    pub fn desired_height(&self, action_count: usize) -> f32 {
        match &self.state {
            HudState::SelectingAction => {
                let items_h = action_count as f32 * 46.0;
                (96.0 + items_h).clamp(220.0, 460.0)
            }
            HudState::RewriteAsInput { .. } => 210.0,
            HudState::Processing { .. } => 150.0,
            HudState::DiffView { .. } => 430.0,
        }
    }

    /// Activates the action at `idx`: rewrite-as actions open the custom
    /// instruction input; everything else is triggered immediately.
    pub fn activate(
        &mut self,
        actions: &[ActionItem],
        idx: usize,
        groq_config: &GroqConfig,
        language: &str,
    ) {
        self.selected_index = idx;
        if actions[idx].is_rewrite_as {
            self.state = HudState::RewriteAsInput { action_idx: idx };
        } else {
            self.trigger_action(&actions[idx], None, groq_config, language);
        }
    }

    pub fn trigger_action(
        &mut self,
        action: &ActionItem,
        user_override: Option<&str>,
        groq_config: &GroqConfig,
        language: &str,
    ) {
        let original = self.original_text.clone();
        let target_hwnd = self.target_hwnd;
        let config = groq_config.clone();
        let lang = language.to_string();
        let tx = self.worker_tx.clone();
        let generation = self.generation.clone();

        let mut instruction = action.prompt_instruction.clone();
        if action.is_rewrite_as {
            // An empty instruction makes the model guess a style and drift;
            // treat blank input as the documented default style.
            let custom = user_override
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("clear and professional");
            instruction = instruction.replace("{user_instruction}", custom);
        }

        let action_name = action.name.clone();
        let action_id = action.id.clone();
        self.state = HudState::Processing {
            action_name: action_name.clone(),
        };
        // Invalidate every previously in-flight run; this one owns the HUD.
        let my_gen = self.generation.fetch_add(1, Ordering::Relaxed) + 1;

        std::thread::spawn(move || {
            match complete_action(&config, &instruction, &original, &lang) {
                Ok(resp) => {
                    // A newer generation means the user closed the HUD or
                    // started another action while this request was in
                    // flight: never paste, never record history.
                    if generation.load(Ordering::Relaxed) != my_gen {
                        return;
                    }

                    // 1. Immediately replace selected text in the active application window
                    let hwnd = HWND(target_hwnd as *mut std::ffi::c_void);
                    crate::clipboard::replace_selection_in_target(hwnd, &resp.text);

                    // 2. Compute visual diff
                    let (chunks, stats) = compute_diff(&original, &resp.text);

                    let _ = tx.send(HudWorkerMsg::Success {
                        refined_text: resp.text,
                        diff_chunks: chunks,
                        stats,
                        action_name,
                        action_id,
                        model: resp.model,
                        duration_ms: resp.duration_ms,
                        language: lang,
                    });
                }
                Err(err) => {
                    if generation.load(Ordering::Relaxed) != my_gen {
                        return;
                    }
                    let _ = tx.send(HudWorkerMsg::Error(err));
                }
            }
        });
    }

    pub fn poll_worker(
        &mut self,
        ctx: &egui::Context,
        on_save_history: &mut dyn FnMut(HistoryEntry),
    ) {
        if let Some(ref rx) = self.worker_rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    HudWorkerMsg::Success {
                        refined_text,
                        diff_chunks,
                        stats,
                        action_name,
                        action_id,
                        model,
                        duration_ms,
                        language,
                    } => {
                        let entry = HistoryEntry::new(
                            action_id,
                            action_name.clone(),
                            self.original_text.clone(),
                            refined_text.clone(),
                            diff_chunks.clone(),
                            stats.clone(),
                            model.clone(),
                            duration_ms,
                            language,
                        );
                        on_save_history(entry);

                        self.state = HudState::DiffView {
                            original_text: self.original_text.clone(),
                            refined_text,
                            diff_chunks,
                            stats,
                            action_name,
                            model,
                            duration_ms,
                        };
                        ctx.request_repaint();
                    }
                    HudWorkerMsg::Error(err) => {
                        // Mirrors the on-screen error to stderr so failures are
                        // diagnosable when launching from a console.
                        eprintln!("[refined] action failed: {err}");
                        self.error_message = Some(err);
                        self.state = HudState::SelectingAction;
                        ctx.request_repaint();
                    }
                }
            }
        }
    }

    /// Renders the action list rows; returns the index of a row that was
    /// clicked so the caller can activate it.
    pub fn render_action_list(
        &mut self,
        ui: &mut Ui,
        actions: &[ActionItem],
        theme: &SpectrumTheme,
    ) -> Option<usize> {
        let mut clicked_idx = None;
        for (idx, action) in actions.iter().enumerate() {
            let is_selected = idx == self.selected_index;
            let bg_color = if is_selected {
                theme.surface_hover
            } else {
                Color32::TRANSPARENT
            };

            let frame = egui::Frame::new()
                .fill(bg_color)
                .corner_radius(CornerRadius::same(6))
                .stroke(if is_selected {
                    Stroke::new(1.0, theme.accent)
                } else {
                    Stroke::NONE
                })
                .inner_margin(egui::Margin::symmetric(10, 6));

            let response = frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(&action.icon)
                            .font(FontId::proportional(15.0))
                            .color(if is_selected { theme.accent } else { theme.text_secondary }),
                    );
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(&action.name)
                                .font(FontId::proportional(13.0))
                                .color(theme.text_primary)
                                .strong(),
                        );
                        ui.label(
                            RichText::new(&action.description)
                                .font(FontId::proportional(12.0))
                                .color(theme.text_secondary),
                        );
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !action.shortcut.is_empty() {
                            spectrum_badge(ui, &action.shortcut, theme.surface_elevated, theme.text_secondary);
                        }
                    });
                });
            });

            let rect = response.response.rect;
            let interact = ui.interact(rect, ui.scope_id().with(idx), Sense::click());
            if interact.clicked() {
                clicked_idx = Some(idx);
            }
        }
        clicked_idx
    }

    pub fn render_ui(
        &mut self,
        ui: &mut Ui,
        actions: &[ActionItem],
        groq_config: &GroqConfig,
        selected_language: &mut String,
        custom_language: &mut String,
        theme: &SpectrumTheme,
        on_open_studio: &mut dyn FnMut(),
    ) {
        let ctx = ui.ctx().clone();

        // Global Esc to close HUD
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.close();
            return;
        }

        // Header
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("✦ Refined")
                    .font(FontId::proportional(14.0))
                    .color(theme.accent)
                    .strong(),
            );
            ui.label(
                RichText::new(format!("({} words)", self.original_text.split_whitespace().count()))
                    .font(FontId::proportional(12.0))
                    .color(theme.text_secondary),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(RichText::new(icons::CLOSE).color(theme.text_secondary)).frame(false)).clicked() {
                    self.close();
                }

                if ui.add(egui::Button::new(RichText::new(format!("{} Studio", icons::SETTINGS)).font(FontId::proportional(12.0)).color(theme.text_secondary)).frame(false)).clicked() {
                    on_open_studio();
                    self.close();
                }

                // Language dropdown
                egui::ComboBox::from_id_salt("hud_language_select")
                    .selected_text(RichText::new(format!("{} {}", icons::WORLD, selected_language)).font(FontId::proportional(12.0)))
                    .width(118.0)
                    .show_ui(ui, |ui| {
                        for lang in standard_languages() {
                            if ui.selectable_label(*selected_language == lang.code, &lang.label).clicked() {
                                *selected_language = lang.code.clone();
                            }
                        }
                    });
            });
        });

        if *selected_language == "custom" {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Language:").font(FontId::proportional(12.0)).color(theme.text_secondary));
                ui.text_edit_singleline(custom_language);
            });
        }

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);

        // Error message alert if present
        if let Some(ref err) = self.error_message {
            ui.scope(|ui| {
                egui::Frame::new()
                    .fill(theme.diff_del_bg)
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.label(RichText::new(format!("{} {}", icons::WARNING, err)).color(theme.diff_del_text).font(FontId::proportional(12.0)));
                    });
            });
            ui.add_space(6.0);
        }

        let should_close = std::cell::Cell::new(false);

        match &self.state {
            HudState::SelectingAction => {
                let action_count = actions.len();
                if ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
                    self.selected_index = (self.selected_index + 1) % action_count;
                }
                if ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
                    self.selected_index = (self.selected_index + action_count - 1) % action_count;
                }

                // Number shortcuts (1-9)
                for (idx, action) in actions.iter().enumerate() {
                    let key_opt = match action.shortcut.as_str() {
                        "1" => Some(Key::Num1),
                        "2" => Some(Key::Num2),
                        "3" => Some(Key::Num3),
                        "4" => Some(Key::Num4),
                        "5" => Some(Key::Num5),
                        "6" => Some(Key::Num6),
                        "7" => Some(Key::Num7),
                        "8" => Some(Key::Num8),
                        "9" => Some(Key::Num9),
                        _ => None,
                    };
                    if let Some(k) = key_opt {
                        if ctx.input(|i| i.key_pressed(k)) {
                            let lang = resolved_language(selected_language, custom_language);
                            self.activate(actions, idx, groq_config, lang);
                            return;
                        }
                    }
                }

                if ctx.input(|i| i.key_pressed(Key::Enter)) {
                    let lang = resolved_language(selected_language, custom_language);
                    self.activate(actions, self.selected_index, groq_config, lang);
                    return;
                }

                // Render action items list in a scrollable container so
                // every row stays reachable even with many custom actions.
                egui::ScrollArea::vertical()
                    .max_height(340.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if let Some(idx) = self.render_action_list(ui, actions, theme) {
                            let lang = resolved_language(selected_language, custom_language);
                            self.activate(actions, idx, groq_config, lang);
                        }
                    });
            }
            HudState::RewriteAsInput { action_idx } => {
                let action = &actions[*action_idx];
                let lang = resolved_language(selected_language, custom_language);
                ui.label(
                    RichText::new(format!("{} {}", icons::EDIT, action.name))
                        .font(FontId::proportional(13.0))
                        .color(theme.accent)
                        .strong(),
                );
                ui.label(
                    RichText::new("Enter instructions for how to rewrite the selected text:")
                        .font(FontId::proportional(12.0))
                        .color(theme.text_secondary),
                );
                ui.add_space(4.0);

                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.custom_rewrite_input)
                        .hint_text("e.g. executive summary, academic style, bullet points...")
                        .desired_width(f32::INFINITY),
                );
                edit.request_focus();

                let mut submitted =
                    edit.lost_focus() && ctx.input(|i| i.key_pressed(Key::Enter));

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if spectrum_button(ui, "Rewrite [Enter]", true, theme).clicked() {
                        submitted = true;
                    }
                    if spectrum_button(ui, "Cancel [Esc]", false, theme).clicked() {
                        self.state = HudState::SelectingAction;
                    }
                });

                if submitted {
                    let input = self.custom_rewrite_input.clone();
                    self.trigger_action(action, Some(&input), groq_config, lang);
                }
            }
            HudState::Processing { action_name } => {
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.spinner();
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!("Refining with {}...", groq_config.model))
                            .font(FontId::proportional(13.0))
                            .color(theme.text_primary)
                            .strong(),
                    );
                    ui.label(
                        RichText::new(format!("Applying action: {action_name}"))
                            .font(FontId::proportional(12.0))
                            .color(theme.text_secondary),
                    );
                    ui.add_space(8.0);
                });
            }
            HudState::DiffView {
                original_text,
                refined_text,
                diff_chunks,
                stats,
                action_name,
                model,
                duration_ms,
            } => {
                // Layout follows the Notion AI / Google Docs pattern: one calm
                // headline stating what happened, one sentence telling the user
                // their text was already replaced and undo is available, then
                // the inline diff, then three clear verbs.
                let secs = *duration_ms as f64 / 1000.0;
                let duration_str = if secs >= 1.0 {
                    format!("{secs:.1} s")
                } else {
                    format!("{duration_ms} ms")
                };
                let model_short = model.split('/').next_back().unwrap_or(model);

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(icons::CHECK_MARK)
                            .font(FontId::proportional(15.0))
                            .color(theme.diff_add_text),
                    );
                    ui.label(
                        RichText::new(format!("{action_name} applied"))
                            .font(FontId::proportional(15.0))
                            .color(theme.text_primary)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{} · {}", duration_str, model_short))
                                .font(FontId::proportional(12.0))
                                .color(theme.text_tertiary),
                        );
                    });
                });

                let changed = diff_chunks
                    .iter()
                    .filter(|c| !matches!(c.kind, crate::diff::DiffKind::Equal))
                    .count();
                let change_word = if changed == 1 { "change" } else { "changes" };
                ui.label(
                    RichText::new(format!(
                        "{changed} {change_word} · replaced in your text — {} → {} words",
                        stats.original_words, stats.refined_words
                    ))
                    .font(FontId::proportional(12.0))
                    .color(theme.text_secondary),
                );

                ui.add_space(10.0);

                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Frame::new()
                            .fill(theme.surface_elevated)
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(egui::Margin::same(12))
                            .show(ui, |ui| {
                                // A fixed width makes the single-flow diff
                                // wrap like a text document, not a column.
                                ui.set_width(ui.available_width());
                                render_diff_view(ui, diff_chunks, theme);
                            });
                    });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if kbd_button(ui, &format!("{} Copy", icons::COPY), "Ctrl+C", false, 118.0, theme)
                        .clicked()
                        || (ctx.input(|i| i.modifiers.command && i.key_pressed(Key::C)))
                    {
                        crate::clipboard::set_clipboard_text(refined_text);
                    }

                    if kbd_button(ui, "Undo", "Ctrl+Z", false, 118.0, theme)
                        .clicked()
                        || (ctx.input(|i| i.modifiers.command && i.key_pressed(Key::Z)))
                    {
                        let hwnd = HWND(self.target_hwnd as *mut std::ffi::c_void);
                        crate::clipboard::replace_selection_in_target(hwnd, original_text);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if kbd_button(ui, "Done", "Esc", true, 118.0, theme).clicked() {
                            should_close.set(true);
                        }
                    });
                });

                ui.add_space(2.0);
                ui.label(
                    RichText::new("Undo replaces the selection with your original text.")
                        .font(FontId::proportional(12.0))
                        .color(theme.text_tertiary),
                );
            }
        }

        if should_close.get() {
            self.close();
        }
    }
}
