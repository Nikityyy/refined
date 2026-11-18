//! Alt+Q Contextual HUD: Action palette, inline custom prompt, processing state, and diff viewer.

use crate::actions::{standard_languages, ActionItem};
use crate::diff::{compute_diff, DiffChunk, DiffStats};
use crate::groq::{complete_action, GroqConfig};
use crate::history::HistoryEntry;
use crate::theme::{render_diff_view, spectrum_badge, spectrum_button, SpectrumTheme};
use eframe::egui::{self, Color32, CornerRadius, FontId, Key, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use std::sync::mpsc::{channel, Receiver, Sender};

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
    pub selected_index: usize,
    pub custom_rewrite_input: String,
    pub error_message: Option<String>,
    pub position: Pos2,
    pub worker_rx: Option<Receiver<HudWorkerMsg>>,
    pub worker_tx: Sender<HudWorkerMsg>,
    pub just_opened: bool,
}

impl Hud {
    pub fn new() -> Self {
        let (worker_tx, worker_rx) = channel();
        Self {
            is_visible: false,
            state: HudState::SelectingAction,
            original_text: String::new(),
            selected_index: 0,
            custom_rewrite_input: String::new(),
            error_message: None,
            position: Pos2::new(200.0, 200.0),
            worker_rx: Some(worker_rx),
            worker_tx,
            just_opened: false,
        }
    }

    pub fn open(&mut self, text: String, x: i32, y: i32) {
        self.original_text = text;
        self.is_visible = true;
        self.state = HudState::SelectingAction;
        self.selected_index = 0;
        self.custom_rewrite_input.clear();
        self.error_message = None;
        self.just_opened = true;
        self.position = Pos2::new((x - 20).max(40) as f32, (y + 15).max(40) as f32);
    }

    pub fn close(&mut self) {
        self.is_visible = false;
        self.state = HudState::SelectingAction;
        self.error_message = None;
    }

    pub fn trigger_action(
        &mut self,
        action: &ActionItem,
        user_override: Option<&str>,
        groq_config: &GroqConfig,
        language: &str,
    ) {
        let original = self.original_text.clone();
        let config = groq_config.clone();
        let lang = language.to_string();
        let tx = self.worker_tx.clone();

        let mut instruction = action.prompt_instruction.clone();
        if action.is_rewrite_as {
            let custom = user_override.unwrap_or("clear and professional").trim();
            instruction = instruction.replace("{user_instruction}", custom);
        }

        let action_name = action.name.clone();
        let action_id = action.id.clone();
        self.state = HudState::Processing {
            action_name: action_name.clone(),
        };

        std::thread::spawn(move || {
            match complete_action(&config, &instruction, &original, &lang) {
                Ok(resp) => {
                    // 1. Immediately replace the selected text in the active application
                    crate::clipboard::replace_selection(&resp.text);

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
                    let _ = tx.send(HudWorkerMsg::Error(err));
                }
            }
        });
    }

    pub fn update(
        &mut self,
        ctx: &egui::Context,
        actions: &[ActionItem],
        groq_config: &GroqConfig,
        selected_language: &mut String,
        custom_language: &mut String,
        theme: &SpectrumTheme,
        on_save_history: &mut dyn FnMut(HistoryEntry),
        on_open_studio: &mut dyn FnMut(),
    ) {
        if !self.is_visible {
            return;
        }

        // Check for worker messages
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
                        self.error_message = Some(err);
                        self.state = HudState::SelectingAction;
                        ctx.request_repaint();
                    }
                }
            }
        }

        // Global Esc to close
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.close();
            return;
        }

        // Screen clamping
        let screen_rect = ctx.input(|i| i.raw.screen_rect).unwrap_or(Rect::from_min_size(Pos2::ZERO, Vec2::new(1920.0, 1080.0)));
        let max_w = 480.0f32.min(screen_rect.width() - 32.0);
        let pos = Pos2::new(
            self.position.x.clamp(16.0, screen_rect.width() - max_w - 16.0),
            self.position.y.clamp(16.0, screen_rect.height() - 380.0),
        );

        let window = egui::Window::new("refined_hud")
            .fixed_pos(pos)
            .fixed_size(Vec2::new(max_w, 0.0))
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .frame(
                egui::Frame::new()
                    .fill(theme.surface)
                    .stroke(Stroke::new(1.0, theme.border))
                    .corner_radius(CornerRadius::same(12))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 8],
                        blur: 24,
                        spread: 0,
                        color: Color32::from_black_alpha(if theme.is_dark { 90 } else { 35 }),
                    })
                    .inner_margin(egui::Margin::same(12)),
            );

        window.show(ctx, |ui| {
            // Close if user clicked outside window (after initial open frame)
            if !self.just_opened && ctx.input(|i| i.pointer.any_pressed()) {
                if let Some(mouse_pos) = ctx.input(|i| i.pointer.latest_pos()) {
                    if !ui.clip_rect().contains(mouse_pos) {
                        self.close();
                        return;
                    }
                }
            }
            self.just_opened = false;

            // Header: Mark, Language, Studio button, Close
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("âœ¦ Refined")
                        .font(FontId::proportional(14.0))
                        .color(theme.accent)
                        .strong(),
                );
                ui.label(
                    RichText::new(format!("({} words)", self.original_text.split_whitespace().count()))
                        .font(FontId::proportional(11.0))
                        .color(theme.text_secondary),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(egui::Button::new(RichText::new("âœ•").color(theme.text_secondary)).frame(false)).clicked() {
                        self.close();
                    }

                    if ui.add(egui::Button::new(RichText::new("âš™ Studio").font(FontId::proportional(11.0)).color(theme.text_secondary)).frame(false)).clicked() {
                        on_open_studio();
                        self.close();
                    }

                    // Language dropdown
                    egui::ComboBox::from_id_salt("hud_language_select")
                        .selected_text(RichText::new(format!("ðŸŒ {}", selected_language)).font(FontId::proportional(11.0)))
                        .width(90.0)
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
                    ui.label(RichText::new("Language:").font(FontId::proportional(11.0)).color(theme.text_secondary));
                    ui.text_edit_singleline(custom_language);
                });
            }

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            // Error alert if present
            if let Some(ref err) = self.error_message {
                ui.scope(|ui| {
                    egui::Frame::new()
                        .fill(theme.diff_del_bg)
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.label(RichText::new(format!("âš  {err}")).color(theme.diff_del_text).font(FontId::proportional(12.0)));
                        });
                });
                ui.add_space(6.0);
            }

            // Body depending on HUD state
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
                                self.selected_index = idx;
                                if action.is_rewrite_as {
                                    self.state = HudState::RewriteAsInput { action_idx: idx };
                                } else {
                                    let lang = if *selected_language == "custom" { custom_language.as_str() } else { selected_language.as_str() };
                                    self.trigger_action(action, None, groq_config, lang);
                                }
                                return;
                            }
                        }
                    }

                    if ctx.input(|i| i.key_pressed(Key::Enter)) {
                        let action = &actions[self.selected_index];
                        if action.is_rewrite_as {
                            self.state = HudState::RewriteAsInput { action_idx: self.selected_index };
                        } else {
                            let lang = if *selected_language == "custom" { custom_language.as_str() } else { selected_language.as_str() };
                            self.trigger_action(action, None, groq_config, lang);
                        }
                        return;
                    }

                    // Render action items list
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
                            .inner_margin(egui::Margin::symmetric(10, 7));

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
                                            .font(FontId::proportional(11.0))
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
                            self.selected_index = idx;
                            if action.is_rewrite_as {
                                self.state = HudState::RewriteAsInput { action_idx: idx };
                            } else {
                                let lang = if *selected_language == "custom" { custom_language.as_str() } else { selected_language.as_str() };
                                self.trigger_action(action, None, groq_config, lang);
                            }
                        }
                    }
                }
                HudState::RewriteAsInput { action_idx } => {
                    let action = &actions[*action_idx];
                    ui.label(
                        RichText::new(format!("âœŽ {}", action.name))
                            .font(FontId::proportional(13.0))
                            .color(theme.accent)
                            .strong(),
                    );
                    ui.label(
                        RichText::new("Enter instructions for how to rewrite the selected text:")
                            .font(FontId::proportional(11.0))
                            .color(theme.text_secondary),
                    );
                    ui.add_space(4.0);

                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.custom_rewrite_input)
                            .hint_text("e.g. executive summary, academic style, bullet points...")
                            .desired_width(f32::INFINITY),
                    );
                    edit.request_focus();

                    if edit.lost_focus() && ctx.input(|i| i.key_pressed(Key::Enter)) {
                        let input = self.custom_rewrite_input.clone();
                        let lang = if *selected_language == "custom" { custom_language.as_str() } else { selected_language.as_str() };
                        self.trigger_action(action, Some(&input), groq_config, lang);
                    }

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if spectrum_button(ui, "Rewrite [Enter]", true, theme).clicked() {
                            let input = self.custom_rewrite_input.clone();
                            let lang = if *selected_language == "custom" { custom_language.as_str() } else { selected_language.as_str() };
                            self.trigger_action(action, Some(&input), groq_config, lang);
                        }
                        if spectrum_button(ui, "Cancel [Esc]", false, theme).clicked() {
                            self.state = HudState::SelectingAction;
                        }
                    });
                }
                HudState::Processing { action_name } => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(10.0);
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
                                .font(FontId::proportional(11.0))
                                .color(theme.text_secondary),
                        );
                        ui.add_space(10.0);
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
                    ui.horizontal(|ui| {
                        spectrum_badge(ui, "âœ“ Replaced in App", theme.diff_add_bg, theme.diff_add_text);
                        ui.add_space(4.0);
                        spectrum_badge(ui, action_name, theme.accent_subtle, theme.accent);

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{}ms Â· {}", duration_ms, model))
                                    .font(FontId::proportional(11.0))
                                    .color(theme.text_secondary),
                            );
                        });
                    });

                    ui.add_space(4.0);

                    let delta = stats.word_delta();
                    let delta_str = if delta >= 0 {
                        format!("+{} words (+{:.1}%)", delta, stats.percent_change())
                    } else {
                        format!("{} words ({:.1}%)", delta, stats.percent_change())
                    };
                    ui.label(
                        RichText::new(format!(
                            "{} â†’ {} words ({})",
                            stats.original_words, stats.refined_words, delta_str
                        ))
                        .font(FontId::proportional(12.0))
                        .color(if delta <= 0 { theme.diff_add_text } else { theme.accent })
                        .strong(),
                    );

                    ui.add_space(6.0);

                    egui::ScrollArea::vertical()
                        .max_height(240.0)
                        .show(ui, |ui| {
                            egui::Frame::new()
                                .fill(theme.surface_elevated)
                                .corner_radius(CornerRadius::same(6))
                                .inner_margin(egui::Margin::same(10))
                                .show(ui, |ui| {
                                    render_diff_view(ui, diff_chunks, theme);
                                });
                        });

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if spectrum_button(ui, "Copy Refined [Ctrl+C]", false, theme).clicked()
                            || (ctx.input(|i| i.modifiers.command && i.key_pressed(Key::C)))
                        {
                            crate::clipboard::set_clipboard_text(refined_text);
                        }

                        if spectrum_button(ui, "Undo Replacement [Ctrl+Z]", false, theme).clicked()
                            || (ctx.input(|i| i.modifiers.command && i.key_pressed(Key::Z)))
                        {
                            crate::clipboard::replace_selection(original_text);
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if spectrum_button(ui, "Close [Esc]", true, theme).clicked() {
                                should_close.set(true);
                            }
                        });
                    });
                }
            }
            if should_close.get() { self.close(); }
        });
    }
}







