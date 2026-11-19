//! Contextual HUD: Action palette, inline custom prompt, processing state, and diff viewer.

use crate::actions::{standard_languages, ActionItem};
use crate::diff::{compute_diff, DiffChunk, DiffStats};
use crate::groq::{complete_action, GroqConfig};
use crate::history::HistoryEntry;
use crate::store::AppSettings;
use crate::theme::{icons, kbd_button, render_diff_view, SpectrumTheme};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Key, Pos2, RichText, Sense, Stroke, Ui, Vec2,
};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};

pub const WIDTH: f32 = 480.0;

pub fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("refined_hud")
}

#[derive(Debug, Clone, PartialEq)]
pub enum HudState {
    SelectingAction,
    RewriteAsInput {
        action_idx: usize,
    },
    Processing {
        action_name: String,
    },
    DiffView {
        original_text: String,
        refined_text: String,
        diff_chunks: Vec<DiffChunk>,
        stats: DiffStats,
        action_name: String,
        model: String,
        duration_ms: u128,
        applied: bool,
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
    pub presented: bool,
    pub state: HudState,
    pub original_text: String,
    pub target_hwnd: isize,
    pub selected_index: usize,
    pub custom_rewrite_input: String,
    pub error_message: Option<String>,
    pub position: Pos2,
    worker_rx: Option<Receiver<HudWorkerMsg>>,
    seen_focus: bool,
    outside_input_down: bool,
    focus_requested: bool,
    awaiting_paste_blur: bool,
    focus_rewrite: bool,
    /// Demo mode: do not auto-dismiss on focus loss (so the UI can be inspected).
    pub demo_sticky: bool,
}

fn outside_input_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_ESCAPE, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON,
    };
    [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_ESCAPE]
        .iter()
        .any(|key| unsafe { GetAsyncKeyState(i32::from(key.0)) < 0 })
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
    let pt = POINT {
        x: cursor_x,
        y: cursor_y,
    };
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

    let clamped_x = desired_x.clamp(
        (min_x + 12) as f32,
        ((max_x - 12) as f32 - width).max((min_x + 12) as f32),
    );
    let clamped_y = desired_y.clamp(
        (min_y + 12) as f32,
        ((max_y - 12) as f32 - height).max((min_y + 12) as f32),
    );

    Pos2::new(clamped_x, clamped_y)
}

impl Hud {
    pub fn new() -> Self {
        Self {
            is_visible: false,
            presented: false,
            state: HudState::SelectingAction,
            original_text: String::new(),
            target_hwnd: 0,
            selected_index: 0,
            custom_rewrite_input: String::new(),
            error_message: None,
            position: Pos2::new(200.0, 200.0),
            worker_rx: None,
            seen_focus: false,
            outside_input_down: false,
            focus_requested: false,
            awaiting_paste_blur: false,
            focus_rewrite: false,
            demo_sticky: false,
        }
    }

    pub fn open(&mut self, text: String, x: i32, y: i32, target_hwnd: isize) {
        // Invalidate any in-flight run from a previous HUD session.
        self.worker_rx = None;
        self.original_text = text;
        self.target_hwnd = target_hwnd;
        self.is_visible = true;
        self.presented = false;
        self.state = HudState::SelectingAction;
        self.selected_index = 0;
        self.custom_rewrite_input.clear();
        self.error_message = None;
        self.seen_focus = false;
        self.outside_input_down = outside_input_down();
        self.focus_requested = false;
        self.awaiting_paste_blur = false;
        // Reserve room for the expanded palette and an inline error.
        self.position = compute_clamped_hud_pos(x, y, WIDTH, 530.0);
    }

    pub fn close(&mut self) {
        self.is_visible = false;
        self.presented = false;
        self.state = HudState::SelectingAction;
        self.error_message = None;
        self.original_text = String::new();
        self.custom_rewrite_input = String::new();
        // Invalidate any in-flight worker so its completion is dropped.
        self.worker_rx = None;
    }

    pub fn present(&mut self, ctx: &egui::Context) {
        use windows::core::{w, PCWSTR};
        use windows::Win32::Graphics::Dwm::{
            DwmSetWindowAttribute, DWMWA_TRANSITIONS_FORCEDISABLED, DWMWA_WINDOW_CORNER_PREFERENCE,
            DWMWCP_ROUND,
        };
        use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

        // Winit creates the window hidden so its initial position and the
        // first GL frame can be ready before DWM presents the palette.
        if let Ok(hwnd) = unsafe { FindWindowW(PCWSTR::null(), w!("Refined HUD")) } {
            let disable_transitions: i32 = 1;
            unsafe {
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_TRANSITIONS_FORCEDISABLED,
                    std::ptr::from_ref(&disable_transitions).cast(),
                    std::mem::size_of_val(&disable_transitions) as u32,
                );
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    std::ptr::from_ref(&DWMWCP_ROUND).cast(),
                    std::mem::size_of_val(&DWMWCP_ROUND) as u32,
                );
            }
        }
        self.presented = true;
        self.focus_requested = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    pub fn close_window(&mut self, ctx: &egui::Context) {
        self.close();
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        ctx.send_viewport_cmd_to(
            egui::ViewportId::ROOT,
            egui::ViewportCommand::RequestPaintWhileHidden,
        );
        ctx.request_repaint_of(egui::ViewportId::ROOT);
    }

    pub fn dismiss_requested(&mut self, ui: &Ui) -> bool {
        if ui.input(|i| i.viewport().close_requested()) {
            return true;
        }
        if self.demo_sticky {
            return false;
        }
        let input_down = outside_input_down();
        let pressed = input_down && !self.outside_input_down;
        self.outside_input_down = input_down;
        if ui.input(|i| i.focused) {
            self.seen_focus = !self.awaiting_paste_blur;
            return false;
        }
        if std::mem::take(&mut self.awaiting_paste_blur) {
            self.seen_focus = false;
        }
        if !self.focus_requested {
            self.focus_requested = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        // The native paste intentionally transfers focus to the editor.
        // While the diff is unfocused, a subsequent click or Esc dismisses it.
        ui.ctx().request_repaint_after(Duration::from_millis(24));
        self.seen_focus || pressed
    }

    pub fn desired_height(&self, settings: &AppSettings) -> f32 {
        let action_rows = settings.actions.len().min(9);
        (48.0 + action_rows as f32 * 37.0).max(274.0)
            + if settings.selected_language == "custom" {
                32.0
            } else {
                0.0
            }
            + if self.error_message.is_some() {
                72.0
            } else {
                0.0
            }
    }

    /// Activates the action at `idx`: rewrite-as actions open the custom
    /// instruction input; everything else is triggered immediately.
    pub fn activate(
        &mut self,
        ctx: &egui::Context,
        actions: &[ActionItem],
        idx: usize,
        groq_config: &GroqConfig,
        language: &str,
    ) {
        self.selected_index = idx;
        if actions[idx].is_rewrite_as {
            self.focus_rewrite = true;
            self.state = HudState::RewriteAsInput { action_idx: idx };
        } else {
            self.trigger_action(ctx, &actions[idx], None, groq_config, language);
        }
    }

    pub fn trigger_action(
        &mut self,
        ctx: &egui::Context,
        action: &ActionItem,
        user_override: Option<&str>,
        groq_config: &GroqConfig,
        language: &str,
    ) {
        let original = self.original_text.clone();
        let config = groq_config.clone();
        let lang = language.to_string();
        let (tx, rx) = channel();
        self.worker_rx = Some(rx);
        self.error_message = None;
        let ctx = ctx.clone();

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
        std::thread::spawn(move || {
            let msg = match complete_action(&config, &instruction, &original, &lang) {
                Ok(resp) => {
                    let (diff_chunks, stats) = compute_diff(&original, &resp.text);
                    HudWorkerMsg::Success {
                        refined_text: resp.text,
                        diff_chunks,
                        stats,
                        action_name,
                        action_id,
                        model: resp.model,
                        duration_ms: resp.duration_ms,
                        language: lang,
                    }
                }
                Err(err) => HudWorkerMsg::Error(err),
            };
            if tx.send(msg).is_ok() {
                ctx.request_repaint_of(viewport_id());
            }
        });
    }

    pub fn poll_worker(
        &mut self,
        ctx: &egui::Context,
        on_save_history: &mut dyn FnMut(HistoryEntry),
    ) {
        let Some(rx) = &self.worker_rx else { return };
        let msg = match rx.try_recv() {
            Ok(msg) => msg,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                HudWorkerMsg::Error("Refinement ended unexpectedly. Try again.".into())
            }
        };
        self.worker_rx = None;
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
                let hwnd = HWND(self.target_hwnd as *mut std::ffi::c_void);
                let changed = self.original_text != refined_text;
                let applied =
                    changed && crate::clipboard::replace_selection_in_target(hwnd, &refined_text);
                if changed && self.target_hwnd != 0 {
                    self.awaiting_paste_blur = true;
                    self.focus_requested = true;
                    self.seen_focus = false;
                }
                on_save_history(HistoryEntry::new(
                    action_id,
                    action_name.clone(),
                    self.original_text.clone(),
                    refined_text.clone(),
                    &diff_chunks,
                    stats.clone(),
                    model.clone(),
                    duration_ms,
                    language,
                ));
                self.state = HudState::DiffView {
                    original_text: self.original_text.clone(),
                    refined_text,
                    diff_chunks,
                    stats,
                    action_name,
                    model,
                    duration_ms,
                    applied,
                };
            }
            HudWorkerMsg::Error(err) => {
                self.error_message = Some(err);
                self.state = HudState::SelectingAction;
            }
        }
        ctx.request_repaint();
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
        follow_selection: bool,
    ) -> Option<usize> {
        let mut clicked_idx = None;
        let row_h = 32.0;
        let width = ui.available_width();

        for (idx, action) in actions.iter().enumerate() {
            let is_selected = idx == self.selected_index;
            let (rect, response) = ui.allocate_exact_size(Vec2::new(width, row_h), Sense::click());
            if response.clicked() {
                clicked_idx = Some(idx);
            }
            if follow_selection && is_selected {
                response.scroll_to_me(None);
            }

            // Spectrum selection is a quiet fill, never an accent outline.
            let bg = if is_selected || response.hovered() {
                theme.surface_hover
            } else {
                Color32::TRANSPARENT
            };
            ui.painter().rect_filled(rect, CornerRadius::same(4), bg);

            let pad = 10.0;

            // Icon in a fixed slot so every glyph optically aligns.
            let icon_galley = ui.painter().layout(
                action.icon.clone(),
                FontId::proportional(16.0),
                if is_selected {
                    theme.accent_visual
                } else {
                    theme.text_secondary
                },
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
        settings: &mut AppSettings,
        theme: &SpectrumTheme,
        on_open_studio: &mut dyn FnMut(),
    ) {
        let actions = &settings.actions;
        let groq_config = &settings.groq;
        let selected_language = &mut settings.selected_language;
        let custom_language = &mut settings.custom_language;
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
                    ui.style_mut().visuals.widgets.inactive.bg_stroke = Stroke::NONE;
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

        if *selected_language == "custom" && !matches!(self.state, HudState::DiffView { .. }) {
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
                        ui.label(
                            RichText::new(format!("{} {}", icons::WARNING, err))
                                .color(theme.diff_del_text)
                                .font(FontId::proportional(12.0)),
                        );
                    });
            });
            ui.add_space(6.0);
        }

        let should_close = std::cell::Cell::new(false);

        match &self.state {
            HudState::SelectingAction => {
                let previous_index = self.selected_index;
                let action_count = actions.len();
                if action_count == 0 {
                    ui.label("Add an action in Refined Studio.");
                    return;
                }
                self.selected_index = self.selected_index.min(action_count - 1);
                let shortcuts = !ctx.egui_wants_keyboard_input() && !egui::Popup::is_any_open(&ctx);
                if shortcuts && ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
                    self.selected_index = (self.selected_index + 1) % action_count;
                }
                if shortcuts && ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
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
                        if shortcuts && ctx.input(|i| i.modifiers.is_none() && i.key_pressed(k)) {
                            let lang = resolved_language(selected_language, custom_language);
                            self.activate(&ctx, actions, idx, groq_config, lang);
                            return;
                        }
                    }
                }

                if shortcuts && ctx.input(|i| i.key_pressed(Key::Enter)) {
                    let lang = resolved_language(selected_language, custom_language);
                    self.activate(&ctx, actions, self.selected_index, groq_config, lang);
                    return;
                }

                // Render action items list in a scrollable container so
                // every row stays reachable even with many custom actions.
                egui::ScrollArea::vertical()
                    .max_height(ui.available_height())
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if let Some(idx) = self.render_action_list(
                            ui,
                            actions,
                            theme,
                            previous_index != self.selected_index,
                        ) {
                            let lang = resolved_language(selected_language, custom_language);
                            self.activate(&ctx, actions, idx, groq_config, lang);
                        }
                    });
            }
            HudState::RewriteAsInput { action_idx } => {
                let Some(action) = actions.get(*action_idx) else {
                    self.state = HudState::SelectingAction;
                    return;
                };
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
                if std::mem::take(&mut self.focus_rewrite) {
                    edit.request_focus();
                }

                let mut submitted = edit.lost_focus() && ctx.input(|i| i.key_pressed(Key::Enter));

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
                    self.trigger_action(&ctx, action, Some(&input), groq_config, lang);
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
                    ui.add_space((ui.available_height() - 102.0).max(0.0) / 2.0);
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
                applied,
            } => {
                let status = if *applied || self.demo_sticky {
                    format!("{action_name} applied")
                } else if original_text == refined_text {
                    "No changes needed".into()
                } else {
                    "Result ready to copy".into()
                };
                let secs = *duration_ms as f64 / 1000.0;
                let duration_str = if secs >= 1.0 {
                    format!("{secs:.1} s")
                } else {
                    format!("{duration_ms} ms")
                };
                let model_short = model.split('/').next_back().unwrap_or(model);

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(if *applied || self.demo_sticky {
                            icons::CHECK_MARK
                        } else {
                            icons::COPY
                        })
                        .font(FontId::proportional(15.0))
                        .color(theme.diff_add_text),
                    );
                    ui.label(
                        RichText::new(status)
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

                let diff_height = (ui.available_height() - 46.0).max(40.0);
                egui::Frame::new()
                    .fill(theme.background)
                    .stroke(Stroke::new(1.0, theme.border))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height((diff_height - 26.0).max(14.0))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                render_diff_view(ui, diff_chunks, theme);
                            });
                    });

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    // Every verb carries an icon + label + keycap chip: one
                    // consistent pattern across the app.
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                    if kbd_button(ui, Some(icons::COPY), "Copy", Some("Ctrl+C"), false, theme)
                        .clicked()
                        || (ctx.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, Key::C)))
                    {
                        crate::clipboard::set_clipboard_text(refined_text);
                    }

                    if *applied
                        && (kbd_button(
                            ui,
                            Some(icons::REFRESH),
                            "Undo",
                            Some("Ctrl+Z"),
                            false,
                            theme,
                        )
                        .clicked()
                            || ctx.input_mut(|input| {
                                input.consume_key(egui::Modifiers::CTRL, Key::Z)
                            }))
                    {
                        let hwnd = HWND(self.target_hwnd as *mut std::ffi::c_void);
                        if crate::clipboard::undo_in_target(hwnd) {
                            should_close.set(true);
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if kbd_button(
                            ui,
                            Some(icons::CHECK_MARK),
                            "Done",
                            Some("Esc"),
                            true,
                            theme,
                        )
                        .clicked()
                        {
                            should_close.set(true);
                        }
                    });
                });
            }
        }

        if should_close.get() {
            self.close();
        }
    }
}
