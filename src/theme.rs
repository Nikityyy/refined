//! Adobe Spectrum 2 Design System tokens, color palettes, and custom widgets.

use crate::diff::{DiffChunk, DiffKind};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Pos2, Response, RichText, Sense, Stroke, StrokeKind, Ui,
    Vec2, Visuals,
};

#[allow(dead_code)]
pub const SPACE_2: f32 = 2.0;
#[allow(dead_code)]
pub const SPACE_4: f32 = 4.0;
#[allow(dead_code)]
pub const SPACE_8: f32 = 8.0;
#[allow(dead_code)]
pub const SPACE_12: f32 = 12.0;
#[allow(dead_code)]
pub const SPACE_16: f32 = 16.0;
#[allow(dead_code)]
pub const SPACE_20: f32 = 20.0;
#[allow(dead_code)]
pub const SPACE_24: f32 = 24.0;
#[allow(dead_code)]
pub const SPACE_32: f32 = 32.0;

#[derive(Clone, Copy, Debug)]
pub struct SpectrumTheme {
    pub is_dark: bool,
    pub background: Color32,
    pub surface: Color32,
    pub surface_elevated: Color32,
    pub surface_hover: Color32,
    pub surface_active: Color32,
    pub border: Color32,
    #[allow(dead_code)]
pub border_subtle: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_tertiary: Color32,
    pub accent: Color32,
    #[allow(dead_code)]
pub accent_hover: Color32,
    pub accent_subtle: Color32,
    pub diff_add_bg: Color32,
    pub diff_add_text: Color32,
    pub diff_del_bg: Color32,
    pub diff_del_text: Color32,
    pub purple: Color32,
    pub amber: Color32,
}

impl SpectrumTheme {
    pub fn for_mode(dark: bool) -> Self {
        if dark {
            Self {
                is_dark: true,
                background: Color32::from_rgb(0x18, 0x18, 0x1B), // Spectrum 2 Gray 100 Dark
                surface: Color32::from_rgb(0x23, 0x23, 0x27),    // Spectrum 2 Gray 75 Dark
                surface_elevated: Color32::from_rgb(0x2C, 0x2C, 0x31),
                surface_hover: Color32::from_rgb(0x35, 0x35, 0x3B),
                surface_active: Color32::from_rgb(0x40, 0x40, 0x48),
                border: Color32::from_rgb(0x39, 0x39, 0x40),
                border_subtle: Color32::from_rgb(0x2C, 0x2C, 0x32),
                text_primary: Color32::from_rgb(0xFD, 0xFD, 0xFD),
                text_secondary: Color32::from_rgb(0x9E, 0x9E, 0xA7),
                text_tertiary: Color32::from_rgb(0x66, 0x66, 0x70),
                accent: Color32::from_rgb(0x3B, 0x82, 0xF6), // Spectrum Blue 600
                accent_hover: Color32::from_rgb(0x60, 0xA5, 0xFA),
                accent_subtle: Color32::from_rgba_premultiplied(59, 130, 246, 35),
                diff_add_bg: Color32::from_rgba_premultiplied(34, 197, 94, 38),
                diff_add_text: Color32::from_rgb(0x4A, 0xDE, 0x80),
                diff_del_bg: Color32::from_rgba_premultiplied(239, 68, 68, 38),
                diff_del_text: Color32::from_rgb(0xF8, 0x71, 0x71),
                purple: Color32::from_rgb(0x8B, 0x5C, 0xF6),
                amber: Color32::from_rgb(0xF5, 0x9E, 0x0B),
            }
        } else {
            Self {
                is_dark: false,
                background: Color32::from_rgb(0xF5, 0xF5, 0xF7), // Spectrum 2 Gray 100 Light
                surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),    // Spectrum 2 Surface Light
                surface_elevated: Color32::from_rgb(0xFA, 0xFA, 0xFC),
                surface_hover: Color32::from_rgb(0xEB, 0xEB, 0xF0),
                surface_active: Color32::from_rgb(0xDF, 0xDF, 0xE5),
                border: Color32::from_rgb(0xE0, 0xE0, 0xE6),
                border_subtle: Color32::from_rgb(0xEE, 0xEE, 0xF2),
                text_primary: Color32::from_rgb(0x18, 0x18, 0x1B),
                text_secondary: Color32::from_rgb(0x64, 0x64, 0x6E),
                text_tertiary: Color32::from_rgb(0x9E, 0x9E, 0xA6),
                accent: Color32::from_rgb(0x02, 0x65, 0xDC), // Spectrum Blue 800
                accent_hover: Color32::from_rgb(0x00, 0x54, 0xB6),
                accent_subtle: Color32::from_rgba_premultiplied(2, 101, 220, 24),
                diff_add_bg: Color32::from_rgba_premultiplied(22, 163, 74, 30),
                diff_add_text: Color32::from_rgb(0x16, 0xA3, 0x4A),
                diff_del_bg: Color32::from_rgba_premultiplied(220, 38, 38, 30),
                diff_del_text: Color32::from_rgb(0xDC, 0x26, 0x26),
                purple: Color32::from_rgb(0x7C, 0x3A, 0xED),
                amber: Color32::from_rgb(0xD9, 0x77, 0x06),
            }
        }
    }
}

/// Configures egui Visuals to match Adobe Spectrum 2 principles.
pub fn configure_egui_theme(ctx: &egui::Context, theme: &SpectrumTheme) {
    let mut v = if theme.is_dark { Visuals::dark() } else { Visuals::light() };

    v.dark_mode = theme.is_dark;
    v.override_text_color = Some(theme.text_primary);
    v.panel_fill = theme.background;
    v.window_fill = theme.surface;
    v.window_stroke = Stroke::new(1.0, theme.border);
    v.window_corner_radius = CornerRadius::same(12);

    v.widgets.noninteractive.bg_fill = theme.surface;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, theme.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, theme.text_primary);
    v.widgets.noninteractive.corner_radius = CornerRadius::same(6);

    v.widgets.inactive.bg_fill = theme.surface_elevated;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, theme.border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, theme.text_primary);
    v.widgets.inactive.corner_radius = CornerRadius::same(6);

    v.widgets.hovered.bg_fill = theme.surface_hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, theme.accent);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, theme.text_primary);
    v.widgets.hovered.corner_radius = CornerRadius::same(6);

    v.widgets.active.bg_fill = theme.surface_active;
    v.widgets.active.bg_stroke = Stroke::new(1.5, theme.accent);
    v.widgets.active.fg_stroke = Stroke::new(1.0, theme.text_primary);
    v.widgets.active.corner_radius = CornerRadius::same(6);

    v.widgets.open.bg_fill = theme.surface_hover;
    v.selection.bg_fill = theme.accent_subtle;
    v.selection.stroke = Stroke::new(1.0, theme.accent);

    ctx.set_visuals(v);
}

/// Sets up Segoe UI typography from Windows system fonts if available.
pub fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let font_dir = std::path::Path::new("C:\\Windows\\Fonts");

    if let Ok(bytes) = std::fs::read(font_dir.join("segoeui.ttf")) {
        fonts.font_data.insert(
            "SegoeUI".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.insert(0, "SegoeUI".to_owned());
        }
    }
    if let Ok(bytes) = std::fs::read(font_dir.join("seguisb.ttf")) {
        fonts.font_data.insert(
            "SegoeUISemiBold".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
    }

    ctx.set_fonts(fonts);
}

/// Adobe Spectrum 2 Primary or Secondary Button
pub fn spectrum_button(ui: &mut Ui, label: &str, primary: bool, theme: &SpectrumTheme) -> Response {
    let text = if primary {
        RichText::new(label).color(Color32::WHITE).strong()
    } else {
        RichText::new(label).color(theme.text_primary)
    };

    let button = if primary {
        egui::Button::new(text)
            .fill(theme.accent)
            .corner_radius(CornerRadius::same(6))
            .stroke(Stroke::NONE)
    } else {
        egui::Button::new(text)
            .fill(theme.surface_elevated)
            .corner_radius(CornerRadius::same(6))
            .stroke(Stroke::new(1.0, theme.border))
    };

    ui.add(button)
}

/// Adobe Spectrum 2 Animated 40x22 Sliding Switch Toggle
pub fn spectrum_switch(ui: &mut Ui, on: &mut bool, theme: &SpectrumTheme) -> Response {
    let desired_size = Vec2::new(40.0, 22.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        let how_on = ui.ctx().animate_bool_responsive(response.id, *on);

        let bg = if *on {
            theme.accent
        } else {
            theme.surface_active
        };

        let border_stroke = if *on {
            Stroke::NONE
        } else {
            Stroke::new(1.0, theme.border)
        };

        let radius = (rect.height() / 2.0).round() as u8;
        ui.painter().rect_filled(rect, CornerRadius::same(radius), bg);
        if border_stroke != Stroke::NONE {
            ui.painter().rect_stroke(rect, CornerRadius::same(radius), border_stroke, StrokeKind::Inside);
        }

        let knob_radius = (rect.height() - 6.0) / 2.0;
        let min_x = rect.min.x + 3.0 + knob_radius;
        let max_x = rect.max.x - 3.0 - knob_radius;
        let knob_x = egui::lerp(min_x..=max_x, how_on);
        let knob_center = Pos2::new(knob_x, rect.center().y);

        ui.painter().circle(
            knob_center,
            knob_radius,
            Color32::WHITE,
            Stroke::NONE,
        );
    }

    response
}

/// Adobe Spectrum 2 Badge / Pill tag
pub fn spectrum_badge(ui: &mut Ui, text: &str, bg_color: Color32, text_color: Color32) {
    ui.scope(|ui| {
        egui::Frame::new()
            .fill(bg_color)
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin::symmetric(8, 3))
            .show(ui, |ui| {
                ui.label(RichText::new(text).font(FontId::proportional(11.0)).color(text_color).strong());
            });
    });
}

/// Adobe Spectrum 2 Diff Renderer: Displays word additions (green) and deletions (red)
pub fn render_diff_view(ui: &mut Ui, chunks: &[DiffChunk], theme: &SpectrumTheme) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(1.0, 4.0);
        for chunk in chunks {
            match chunk.kind {
                DiffKind::Equal => {
                    ui.label(RichText::new(&chunk.text).color(theme.text_primary).font(FontId::proportional(14.0)));
                }
                DiffKind::Delete => {
                    ui.scope(|ui| {
                        egui::Frame::new()
                            .fill(theme.diff_del_bg)
                            .corner_radius(CornerRadius::same(3))
                            .inner_margin(egui::Margin::symmetric(3, 1))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(&chunk.text)
                                        .color(theme.diff_del_text)
                                        .font(FontId::proportional(14.0))
                                        .strikethrough(),
                                );
                            });
                    });
                }
                DiffKind::Insert => {
                    ui.scope(|ui| {
                        egui::Frame::new()
                            .fill(theme.diff_add_bg)
                            .corner_radius(CornerRadius::same(3))
                            .inner_margin(egui::Margin::symmetric(3, 1))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(&chunk.text)
                                        .color(theme.diff_add_text)
                                        .font(FontId::proportional(14.0))
                                        .strong(),
                                );
                            });
                    });
                }
            }
        }
    });
}
