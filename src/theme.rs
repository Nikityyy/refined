//! Adobe Spectrum 2 Design System tokens, color palettes, and custom widgets.

use crate::diff::{DiffChunk, DiffKind};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Response, RichText, Stroke, Ui, Vec2, Visuals,
};

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
                background: Color32::from_rgb(0x18, 0x18, 0x1B),
                surface: Color32::from_rgb(0x23, 0x23, 0x27),
                surface_elevated: Color32::from_rgb(0x2C, 0x2C, 0x31),
                surface_hover: Color32::from_rgb(0x35, 0x35, 0x3B),
                surface_active: Color32::from_rgb(0x40, 0x40, 0x48),
                border: Color32::from_rgb(0x39, 0x39, 0x40),
                border_subtle: Color32::from_rgb(0x2C, 0x2C, 0x32),
                text_primary: Color32::from_rgb(0xFD, 0xFD, 0xFD),
                text_secondary: Color32::from_rgb(0x9E, 0x9E, 0xA7),
                text_tertiary: Color32::from_rgb(0x66, 0x66, 0x70),
                accent: Color32::from_rgb(0x3B, 0x82, 0xF6),
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
                background: Color32::from_rgb(0xF5, 0xF5, 0xF7),
                surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
                surface_elevated: Color32::from_rgb(0xFA, 0xFA, 0xFC),
                surface_hover: Color32::from_rgb(0xEB, 0xEB, 0xF0),
                surface_active: Color32::from_rgb(0xDF, 0xDF, 0xE5),
                border: Color32::from_rgb(0xE0, 0xE0, 0xE6),
                border_subtle: Color32::from_rgb(0xEE, 0xEE, 0xF2),
                text_primary: Color32::from_rgb(0x18, 0x18, 0x1B),
                text_secondary: Color32::from_rgb(0x64, 0x64, 0x6E),
                text_tertiary: Color32::from_rgb(0x9E, 0x9E, 0xA6),
                accent: Color32::from_rgb(0x02, 0x65, 0xDC),
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

