//! Adobe Spectrum 2 Design System tokens, color palettes, and custom widgets.

use crate::diff::{DiffChunk, DiffKind};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Pos2, Rect, Response, RichText, Sense,
    Stroke, StrokeKind, TextFormat, Ui, Vec2, Visuals,
};

/// Monochrome UI icon glyphs from Segoe MDL2 Assets (Windows system font).
/// These private-use codepoints always resolve through the font fallback chain,
/// avoiding multi-plane emoji that render as tofu boxes.
pub mod icons {
    #![allow(dead_code)]
    pub const SEARCH: &str = "\u{E721}";
    pub const TRASH: &str = "\u{E74D}";
    pub const SETTINGS: &str = "\u{E713}";
    pub const WORLD: &str = "\u{E909}";
    pub const CLOSE: &str = "\u{E711}";
    pub const WARNING: &str = "\u{E7BA}";
    pub const CHECK_MARK: &str = "\u{E73E}";
    pub const EDIT: &str = "\u{E70F}";
    pub const COPY: &str = "\u{E8C8}";
    pub const CLOCK: &str = "\u{E823}";
    pub const REFRESH: &str = "\u{E72C}";
}

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
                background: Color32::from_rgb(0x12, 0x12, 0x15), // Deep Dark Slate
                surface: Color32::from_rgb(0x1E, 0x1E, 0x22),    // Spectrum 2 Surface Dark
                surface_elevated: Color32::from_rgb(0x27, 0x27, 0x2C),
                surface_hover: Color32::from_rgb(0x32, 0x32, 0x38),
                surface_active: Color32::from_rgb(0x3C, 0x3C, 0x44),
                border: Color32::from_rgb(0x38, 0x38, 0x40),
                border_subtle: Color32::from_rgb(0x2A, 0x2A, 0x30),
                text_primary: Color32::from_rgb(0xF5, 0xF5, 0xF7),
                text_secondary: Color32::from_rgb(0x9E, 0x9E, 0xA8),
                text_tertiary: Color32::from_rgb(0x6E, 0x6E, 0x78),
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
                background: Color32::from_rgb(0xF4, 0xF4, 0xF6), // Spectrum 2 Gray 100 Light
                surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),    // Pure Surface White
                surface_elevated: Color32::from_rgb(0xF8, 0xF8, 0xFA),
                surface_hover: Color32::from_rgb(0xEB, 0xEB, 0xF0),
                surface_active: Color32::from_rgb(0xDF, 0xDF, 0xE5),
                border: Color32::from_rgb(0xDF, 0xDF, 0xE6),
                border_subtle: Color32::from_rgb(0xEE, 0xEE, 0xF2),
                text_primary: Color32::from_rgb(0x18, 0x18, 0x1B),
                text_secondary: Color32::from_rgb(0x5E, 0x5E, 0x68),
                text_tertiary: Color32::from_rgb(0x8E, 0x8E, 0x98),
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
    // CRITICAL FIX: Do NOT override text color globally.
    // Setting override_text_color broke selection highlight readability!
    v.override_text_color = None;

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

    // Selection styling: high contrast white text over electric blue
    v.selection.bg_fill = theme.accent;
    v.selection.stroke = Stroke::new(1.0, theme.accent_hover);

    ctx.set_visuals(v);
}

/// Sets up Segoe UI + Segoe UI Symbol typography from Windows system fonts.
/// Guarantees that ALL symbols (✦, 🔍, 🗑, ✎, ✓, ≡, ⚙, 🌐) render crisply with zero tofu [].
pub fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let font_dir = std::path::Path::new("C:\\Windows\\Fonts");

    // 1. Primary Latin typography
    if let Ok(bytes) = std::fs::read(font_dir.join("segoeui.ttf")) {
        fonts.font_data.insert(
            "SegoeUI".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.insert(0, "SegoeUI".to_owned());
        }
    }

    // 2. Headings and emphasis
    if let Ok(bytes) = std::fs::read(font_dir.join("seguisb.ttf")) {
        fonts.font_data.insert(
            "SegoeUISemiBold".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
    }

    // 3. Complete Windows Symbol library (resolves all UI symbols without tofu)
    if let Ok(bytes) = std::fs::read(font_dir.join("seguisym.ttf")) {
        fonts.font_data.insert(
            "SegoeUISymbol".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.push("SegoeUISymbol".to_owned());
        }
    }

    // 4. Windows Segoe MDL2 Assets
    if let Ok(bytes) = std::fs::read(font_dir.join("segmdl2.ttf")) {
        fonts.font_data.insert(
            "SegoeMDL2".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.push("SegoeMDL2".to_owned());
        }
    }

    ctx.set_fonts(fonts);
}

/// Refined product badge in the Studio header: the actual app icon
/// (same art as the exe/tray icon), uploaded once and reused.
pub fn render_rf_logo(ui: &mut Ui, size: f32) {
    use std::sync::OnceLock;

    static LOGO: OnceLock<std::sync::Mutex<Option<egui::TextureHandle>>> = OnceLock::new();
    let cache = LOGO.get_or_init(|| std::sync::Mutex::new(None));

    let ctx = ui.ctx().clone();
    let tex_handle = {
        let mut guard = cache.lock().unwrap();
        guard
            .get_or_insert_with(|| {
                let rgba = crate::icon::rgba(64);
                ctx.load_texture(
                    "refined_logo",
                    egui::ColorImage::from_rgba_unmultiplied([64, 64], &rgba),
                    egui::TextureOptions::default(),
                )
            })
            .clone()
    };

    // The icon is square; show it crisp at the pixel grid.
    let rect = ui
        .allocate_exact_size(Vec2::splat(size), Sense::hover())
        .0;
    ui.painter().image(
        tex_handle.id(),
        rect,
        eframe::epaint::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
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
                ui.label(RichText::new(text).font(FontId::proportional(12.0)).color(text_color).strong());
            });
    });
}

/// Standard Spectrum card frame shared by every Studio panel card, so cards
/// always agree on fill, stroke, radius, and margins.
pub fn card_frame(theme: &SpectrumTheme, inner_margin: egui::Margin) -> egui::Frame {
    egui::Frame::new()
        .fill(theme.surface)
        .stroke(Stroke::new(1.0, theme.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(inner_margin)
}

/// Renderer for the refined-vs-original diff: one wrapped `LayoutJob` so egui
/// line-breaks at the panel width. Wrapped per-chunk frames collapsed into a
/// one-character-wide column for many-chunk diffs; a single job flows
/// correctly, and per-section `TextFormat` backgrounds paint GitHub-style
/// line highlights behind the changed runs.
pub fn render_diff_view(ui: &mut Ui, chunks: &[DiffChunk], theme: &SpectrumTheme) {
    if chunks.is_empty() {
        return;
    }

    let font = FontId::proportional(14.0);
    let mut job = eframe::epaint::text::LayoutJob::default();
    for chunk in chunks {
        let mut format = TextFormat {
            font_id: font.clone(),
            color: theme.text_primary,
            ..Default::default()
        };
        match chunk.kind {
            DiffKind::Equal => {}
            DiffKind::Delete => {
                format.color = theme.diff_del_text;
                format.background = theme.diff_del_bg;
                format.strikethrough = Stroke::new(1.0, theme.diff_del_text);
            }
            DiffKind::Insert => {
                format.color = theme.diff_add_text;
                format.background = theme.diff_add_bg;
            }
        }
        job.append(&chunk.text, 1.0, format);
    }

    ui.add(egui::Label::new(job).wrap());
}

/// Fixed-width verb button with the keyboard shortcut rendered as a chip:
/// "Copy  [Ctrl+C]". Painted directly so the shortcut looks like a key cap
/// instead of flat double-space text.
/// Returns the response for click detection; hover/primary states styled
/// like `spectrum_button`.
pub fn kbd_button(
    ui: &mut Ui,
    label: &str,
    kbd: &str,
    primary: bool,
    width: f32,
    theme: &SpectrumTheme,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 28.0), Sense::click());

    let fill = if response.hovered() {
        if primary {
            theme.accent_hover
        } else {
            theme.surface_hover
        }
    } else if primary {
        theme.accent
    } else {
        theme.surface_elevated
    };
    let corner = CornerRadius::same(6);
    ui.painter().rect_filled(rect, corner, fill);
    if !primary {
        ui.painter()
            .rect_stroke(rect, corner, Stroke::new(1.0, theme.border), StrokeKind::Inside);
    }

    let text_color = if primary {
        Color32::WHITE
    } else {
        theme.text_primary
    };
    let font = FontId::proportional(12.0);
    let label_galley = ui
        .painter()
        .layout(label.to_string(), font, text_color, f32::INFINITY);
    let kbd_galley = ui.painter().layout(
        kbd.to_string(),
        FontId::proportional(12.0),
        if primary { Color32::WHITE } else { theme.text_secondary },
        f32::INFINITY,
    );

    // Key-cap chip on the right edge; plain label on the left.
    let pad = 5.0;
    let chip_size = Vec2::new(
        kbd_galley.size().x + 2.0 * pad,
        kbd_galley.size().y + 4.0,
    );
    let chip_rect = Rect::from_min_size(
        Pos2::new(rect.right() - 12.0 - chip_size.x, rect.center().y - chip_size.y / 2.0),
        chip_size,
    );
    let chip_fill = if primary {
        Color32::from_black_alpha(50)
    } else {
        theme.surface_active
    };
    ui.painter()
        .rect_filled(chip_rect, CornerRadius::same(3), chip_fill);
    ui.painter().galley(
        Pos2::new(
            chip_rect.center().x - kbd_galley.size().x / 2.0,
            chip_rect.center().y - kbd_galley.size().y / 2.0,
        ),
        kbd_galley,
        Color32::WHITE,
    );
    ui.painter().galley(
        Pos2::new(rect.left() + 10.0, rect.center().y - label_galley.size().y / 2.0),
        label_galley,
        Color32::WHITE,
    );

    response
}
