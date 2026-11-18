//! Alt+V Contextual HUD: Action palette, inline custom prompt, processing state, and diff viewer.

use crate::actions::{standard_languages, ActionItem};
use crate::diff::{compute_diff, DiffChunk, DiffStats};
use crate::groq::{complete_action, GroqConfig};
use crate::history::HistoryEntry;
use crate::theme::{icons, kbd_button, render_diff_view, SpectrumTheme};
use eframe::egui::{self, Color32, CornerRadius, FontId, Key, Pos2, RichText, Sense, Stroke, Ui, Vec2};
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
    /// Height the diff view measured for its own content - the window hugs
    /// the diff instead of leaving dead space below the buttons. Assembled
    /// from cursor landmarks (see render_ui), never from the window's
    /// current size, so it cannot feed on its own resize lag.
    pub measured_diff_height: f32,
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
            measured_diff_height: 430.0,
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
                // One quiet header row (32) + single-line rows (34 incl. 2px gap).
                (44.0 + action_count as f32 * 34.0).clamp(160.0, 460.0)
            }
            HudState::RewriteAsInput { .. } => 210.0,
            HudState::Processing { .. } => 150.0,
            HudState::DiffView { .. } => self.measured_diff_height.clamp(240.0, 430.0),
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

                    // Belt and braces beside the groq.rs check: an empty
                    // result must never replace the selection.
                    if resp.text.trim().is_empty() {
                        let _ = tx.send(HudWorkerMsg::Error(
                            "The model returned an empty response. Your text was left unchanged."
                                .to_string(),
                        ));
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
                        // Start from the full box; the first rendered frame
                        // re-measures deterministically (cursor landmarks,
                        // not window size) and the window shrinks to its
                        // content once, with no feedback loop.
                        self.measured_diff_height = 430.0;
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
    /// clicked so the caller can activate it. Minimal single-line rows:
    /// fixed-slot icon, 14px name, keycap on the right; the description is
    /// available on hover so the palette stays visually quiet.
    pub fn render_action_list(
        &mut self,
        ui: &mut Ui,
        actions: &[ActionItem],
        theme: &SpectrumTheme,
    ) -> Option<usize> {
        let mut clicked_idx = None;
        let row_h = 32.0;
        let width = ui.available_width();

        for (idx, action) in actions.iter().enumerate() {
            let is_selected = idx == self.selected_index;
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(width, row_h), Sense::click());
            if response.clicked() {
                clicked_idx = Some(idx);
            }

            // Spectrum selection is a quiet fill, never an accent outline.
            let bg = if is_selected || response.hovered() {
                theme.surface_hover
            } else {
                Color32::TRANSPARENT
            };
            ui.painter()
                .rect_filled(rect, CornerRadius::same(4), bg);

            let pad = 10.0;

            // Icon in a fixed slot so every glyph optically aligns.
            let icon_galley = ui.painter().layout(
                action.icon.clone(),
                FontId::proportional(16.0),
                if is_selected { theme.accent_visual } else { theme.text_secondary },
                f32::INFINITY,
            );
            let icon_cx = rect.left() + pad + 10.0;
            ui.painter().galley(
                Pos2::new(
                    icon_cx - icon_galley.size().x / 2.0,
                    rect.center().y - icon_galley.size().y / 2.0,
                ),
                icon_galley,
                Color32::WHITE,
            );

            let name_galley = ui.painter().layout(
                action.name.clone(),
                FontId::proportional(14.0),
                theme.text_primary,
                f32::INFINITY,
            );
            ui.painter().galley(
                Pos2::new(
                    rect.left() + pad + 20.0 + 8.0,
                    rect.center().y - name_galley.size().y / 2.0,
                ),
                name_galley,
                Color32::WHITE,
            );

            // Keycap on the right edge.
            if !action.shortcut.is_empty() {
                let cap_galley = ui.painter().layout(
                    action.shortcut.clone(),
                    FontId::proportional(12.0),
                    theme.text_secondary,
                    f32::INFINITY,
                );
                let cap_w = cap_galley.size().x + 10.0;
                let cap_h = cap_galley.size().y + 4.0;
                let cap_rect = egui::Rect::from_min_size(
                    Pos2::new(rect.right() - pad - cap_w, rect.center().y - cap_h / 2.0),
                    Vec2::new(cap_w, cap_h),
                );
                ui.painter()
                    .rect_filled(cap_rect, CornerRadius::same(4), theme.surface_subtle);
                // The fill alone can alias the selected row's fill; the chip
                // needs its own outline to stay a chip.
                ui.painter().rect_stroke(
                    cap_rect,
                    CornerRadius::same(4),
                    Stroke::new(1.0, theme.border),
                    egui::StrokeKind::Outside,
                );
                ui.painter().galley(
                    Pos2::new(
                        cap_rect.center().x - cap_galley.size().x / 2.0,
                        cap_rect.center().y - cap_galley.size().y / 2.0,
                    ),
                    cap_galley,
                    Color32::WHITE,
                );
            }

            response.on_hover_text(&action.description);
            ui.add_space(2.0);
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

        // Global Esc to close HUD. Inside the rewrite-as input Esc means
        // "back to the action list" first (the button row says so); a
        // second Esc then closes the HUD as everywhere else.
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            if matches!(self.state, HudState::RewriteAsInput { .. }) {
                self.state = HudState::SelectingAction;
                return;
            }
            self.close();
            return;
        }

        // Quiet header: brand left; language, Studio, close as icon-only
        // controls right. No word count, no separator - the palette stays
        // minimal; the diff screen already shows the word delta.
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Refined")
                    .font(FontId::proportional(14.0))
                    .color(theme.text_primary)
                    .strong(),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(icons::CLOSE)
                                .font(FontId::proportional(13.0))
                                .color(theme.text_secondary),
                        )
                        // Quiet icon button: it must read as text, not as a
                        // heavy bordered chip against the quiet palette.
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::NONE),
                    )
                    .on_hover_text("Close (Esc)")
                    .clicked()
                {
                    self.close();
                }

                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(icons::SETTINGS)
                                .font(FontId::proportional(13.0))
                                .color(theme.text_secondary),
                        )
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::NONE),
                    )
                    .on_hover_text("Open Studio")
                    .clicked()
                {
                    on_open_studio();
                    self.close();
                }

                let lang_label = standard_languages()
                    .iter()
                    .find(|l| l.code == *selected_language)
                    .map(|l| l.label.clone())
                    .unwrap_or_else(|| selected_language.clone());

                // Quiet combo: same hover-only treatment as the two icon
                // buttons, scoped so TextEdits keep their own surfaces.
                ui.scope(|ui| {
                    ui.style_mut().visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
                    ui.style_mut().visuals.widgets.inactive.bg_stroke =
                        Stroke::NONE;
                    ui.style_mut().visuals.widgets.hovered.bg_fill = theme.surface_hover;
                    ui.style_mut().visuals.widgets.hovered.bg_stroke = Stroke::NONE;
                    ui.style_mut().visuals.widgets.active.bg_fill = theme.surface_active;
                    ui.style_mut().visuals.widgets.active.bg_stroke = Stroke::NONE;
                    egui::ComboBox::from_id_salt("hud_language_select")
                        .selected_text(
                            RichText::new(icons::WORLD)
                                .font(FontId::proportional(13.0))
                                .color(theme.text_secondary),
                        )
                        .width(36.0)
                        .show_ui(ui, |ui| {
                            for lang in standard_languages() {
                                if ui
                                    .selectable_label(*selected_language == lang.code, &lang.label)
                                    .clicked()
                                {
                                    *selected_language = lang.code.clone();
                                }
                            }
                        })
                        .response
                        .on_hover_text(format!("Language: {lang_label}"));
                });
            });
        });

        if *selected_language == "custom" {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Language:")
                        .font(FontId::proportional(12.0))
                        .color(theme.text_secondary),
                );
                ui.text_edit_singleline(custom_language);
            });
        }

        ui.add_space(8.0);

        // Error message alert if present
        if let Some(ref err) = self.error_message {
            ui.scope(|ui| {
                egui::Frame::new()
                    .fill(theme.diff_del_bg)
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.label(RichText::new(format!("{} {}", icons::WARNING, err)).color(theme.diff_del_text).font(FontId::proportional(12.0)));
                    });
            });
            ui.add_space(6.0);
        }

        let should_close = std::cell::Cell::new(false);
        // DiffView cursor landmarks for the content-height measurement below.
        let mut diff_metrics: Option<(f32, f32, f32, f32, bool)> = None;

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
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                    if kbd_button(ui, None, "Rewrite", Some("Enter"), true, theme).clicked() {
                        submitted = true;
                    }
                    if kbd_button(ui, None, "Cancel", Some("Esc"), false, theme).clicked() {
                        self.state = HudState::SelectingAction;
                    }
                });

                if submitted {
                    let input = self.custom_rewrite_input.clone();
                    self.trigger_action(action, Some(&input), groq_config, lang);
                }
            }
            HudState::Processing { action_name } => {
                let model_short = groq_config
                    .model
                    .split('/')
                    .next_back()
                    .unwrap_or(&groq_config.model)
                    .to_string();
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    ui.spinner();
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!("Refining with {model_short}..."))
                            .font(FontId::proportional(13.0))
                            .color(theme.text_primary)
                            .strong(),
                    );
                    ui.label(
                        RichText::new(format!("Applying action: {action_name}"))
                            .font(FontId::proportional(12.0))
                            .color(theme.text_secondary),
                    );
                    // Esc works here but nothing said so; make it discoverable.
                    ui.label(
                        RichText::new("Press Esc to cancel")
                            .font(FontId::proportional(11.0))
                            .color(theme.text_tertiary),
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
                                .color(theme.text_secondary),
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

                // Captured to compute the window's exact natural height
                // AFTER the paint: unlike `min_rect`, cursor positions do
                // not depend on how tall the window happens to be right now,
                // so the measurement cannot feed on its own resize lag.
                let y_before_scroll = ui.cursor().min.y;

                let scroll = egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        // The diff reads as an inset document well: one step
                        // below the window fill, with a real border, not a
                        // fill that aliases the window background.
                        egui::Frame::new()
                            .fill(theme.background)
                            .stroke(Stroke::new(1.0, theme.border))
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(egui::Margin::same(12))
                            .show(ui, |ui| {
                                // A fixed width makes the single-flow diff
                                // wrap like a text document, not a column.
                                ui.set_width(ui.available_width());
                                render_diff_view(ui, diff_chunks, theme);
                            });
                    });

                let scroll_h = scroll.content_size.y.min(200.0);
                // A frame painted before the OS applied the resize shows a
                // squeezed scroll box; its landmarks are transient noise.
                let scroll_valid = scroll.inner_rect.height() >= scroll_h - 0.5;

                ui.add_space(10.0);
                let y_after_scroll = ui.cursor().min.y;
                ui.horizontal(|ui| {
                    // Every verb carries an icon + label + keycap chip: one
                    // consistent pattern across the app.
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                    if kbd_button(ui, Some(icons::COPY), "Copy", Some("Ctrl+C"), false, theme)
                        .clicked()
                        || (ctx.input(|i| i.modifiers.command && i.key_pressed(Key::C)))
                    {
                        crate::clipboard::set_clipboard_text(refined_text);
                    }

                    if kbd_button(ui, Some(icons::REFRESH), "Undo", Some("Ctrl+Z"), false, theme)
                        .clicked()
                        || (ctx.input(|i| i.modifiers.command && i.key_pressed(Key::Z)))
                    {
                        let hwnd = HWND(self.target_hwnd as *mut std::ffi::c_void);
                        crate::clipboard::replace_selection_in_target(hwnd, original_text);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if kbd_button(ui, Some(icons::CHECK_MARK), "Done", Some("Esc"), true, theme)
                            .clicked()
                        {
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

                diff_metrics = Some((
                    y_before_scroll,
                    scroll_h,
                    y_after_scroll,
                    ui.cursor().min.y,
                    scroll_valid,
                ));
            }
        }

        // In the diff view the window hugs its content. The naive measure
        // (`min_rect`) can include the whole window (panels fill their rect),
        // and the OS applies a viewport resize one frame late - feeding that
        // back ping-ponged the window between two sizes forever. Instead the
        // natural height is assembled from cursor landmarks and the scroll
        // area's own rect, none of which depend on the current window size:
        // top margin/header/summary, the capped scroll box, buttons + hint,
        // plus the panel's own margins and strokes.
        if let Some((y_before, scroll_h, y_after, y_end, scroll_valid)) = diff_metrics {
            if scroll_valid {
                let h = (26.0
                    + y_before
                    + scroll_h
                    + (y_end - y_after))
                    .clamp(240.0, 430.0);
                #[cfg(feature = "demo")]
                if std::env::var("REFINED_DEBUG_SIZE").is_ok() {
                    eprintln!(
                        "[size] y_before={:.1} scroll_h={:.1} below={:.1} -> h={:.1} (current {:.1})",
                        y_before,
                        scroll_h,
                        y_end - y_after,
                        h,
                        self.measured_diff_height
                    );
                }
                if (h - self.measured_diff_height).abs() > 0.5 {
                    self.measured_diff_height = h;
                }
            }
        }

        if should_close.get() {
            self.close();
        }
    }
}
