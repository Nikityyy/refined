//! Adobe Spectrum 2 design tokens, color palettes, and custom widgets.
//!
//! Token values are the verified Spectrum 2 source of truth
//! (adobe/spectrum-design-data, main branch): gray/blue/red/green ramps,
//! spacing scale, corner radii, and typography steps. Contrast ratios for
//! text pairs are WCAG-computed (>= 4.5:1 for body text in both schemes).

use crate::diff::{DiffChunk, DiffKind};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Pos2, Rect, Response, RichText, Sense,
    Stroke, StrokeKind, TextFormat, Ui, Vec2, Visuals,
};

/// One icon set, one optical weight: the app's only icon family. Every
/// constant here must have a call site; the compiler enforces it.
/// Adobe Spectrum 2 workflow icons, built into assets/refined-icons.ttf
/// by assets/build-icons.py (codepoints U+F1000 onward, in its order).
pub mod icons {
    pub const TRASH: &str = "\u{F1000}";
    pub const SEARCH: &str = "\u{F1001}";
    pub const SETTINGS: &str = "\u{F1002}";
    pub const WORLD: &str = "\u{F1003}";
    pub const CLOSE: &str = "\u{F1004}";
    pub const WARNING: &str = "\u{F1005}";
    pub const CHECK_MARK: &str = "\u{F1006}";
    pub const EDIT: &str = "\u{F1007}";
    pub const COPY: &str = "\u{F1008}";
    pub const REFRESH: &str = "\u{F1009}";
    pub const CHEVRON_DOWN: &str = "\u{F100A}";
    pub const CHEVRON_UP: &str = "\u{F100B}";
    pub const BULLETED_LIST: &str = "\u{F100C}";
    pub const CLEAR: &str = "\u{F100D}";
    pub const REPLACE: &str = "\u{F100E}";
    pub const ADD: &str = "\u{F100F}";
    pub const DOWNLOAD: &str = "\u{F1010}";
    pub const SAVE: &str = "\u{F1011}";
    pub const EYE: &str = "\u{F1012}";
    pub const MINIMIZE: &str = "\u{F1013}";
    pub const MAXIMIZE: &str = "\u{F1014}";
    pub const TEXT: &str = "\u{F1015}";
    pub const CHECKLIST: &str = "\u{F1016}";
}

/// Icons offered for custom actions.
pub const ACTION_ICONS: [&str; 12] = [
    icons::EDIT,
    icons::BULLETED_LIST,
    icons::MINIMIZE,
    icons::MAXIMIZE,
    icons::TEXT,
    icons::CHECKLIST,
    icons::REPLACE,
    icons::WORLD,
    icons::SEARCH,
    icons::CHECK_MARK,
    icons::COPY,
    icons::EYE,
];

#[derive(Clone, Copy, Debug)]
pub struct SpectrumTheme {
    pub is_dark: bool,
    /// layer-1 app background: gray-25 / gray-50
    pub background: Color32,
    /// layer-2 surface (panels, cards): gray-75 / gray-25
    pub surface: Color32,
    pub surface_elevated: Color32,
    /// row hover: gray-200 / gray-100
    pub surface_hover: Color32,
    /// pressed/track: gray-300 / gray-300
    pub surface_active: Color32,
    /// neutral-subtle fills (keycaps, segmented tracks, quiet chips):
    /// gray-300 / gray-100
    pub surface_subtle: Color32,
    pub border: Color32,
    /// headings: gray-900/300
    pub text_primary: Color32,
    /// body text: neutral-content gray-800 / gray-800
    pub text_body: Color32,
    pub text_secondary: Color32,
    pub text_tertiary: Color32,
    /// accent fill: blue-800 / blue-900 (white labels are 4.5:1+)
    pub accent: Color32,
    pub accent_hover: Color32,
    /// accent-visual: icons and accent text: blue-900 / blue-800
    pub accent_visual: Color32,
    /// blue-subtle chip pair for quiet accent badges
    pub accent_badge_bg: Color32,
    pub accent_badge_text: Color32,
    pub diff_add_bg: Color32,
    pub diff_add_text: Color32,
    pub diff_del_bg: Color32,
    pub diff_del_text: Color32,
}

impl SpectrumTheme {
    pub fn for_mode(dark: bool) -> Self {
        if dark {
            Self {
                is_dark: true,
                background: Color32::from_rgb(0x11, 0x11, 0x11),
                surface: Color32::from_rgb(0x22, 0x22, 0x22),
                // One real step above surface: the token must mean "elevated",
                // not alias the window fill (that made the diff card vanish).
                surface_elevated: Color32::from_rgb(0x2A, 0x2A, 0x2A),
                surface_hover: Color32::from_rgb(0x32, 0x32, 0x32),
                surface_active: Color32::from_rgb(0x39, 0x39, 0x39),
                surface_subtle: Color32::from_rgb(0x39, 0x39, 0x39),
                border: Color32::from_rgb(0x44, 0x44, 0x44),
                text_primary: Color32::from_rgb(0xF2, 0xF2, 0xF2),
                text_body: Color32::from_rgb(0xDB, 0xDB, 0xDB),
                text_secondary: Color32::from_rgb(0xAF, 0xAF, 0xAF),
                text_tertiary: Color32::from_rgb(0x8A, 0x8A, 0x8A),
                accent: Color32::from_rgb(0x40, 0x69, 0xFD),
                accent_hover: Color32::from_rgb(0x34, 0x5B, 0xF8),
                accent_visual: Color32::from_rgb(0x56, 0x81, 0xFF),
                accent_badge_bg: Color32::from_rgb(0x0C, 0x21, 0x75),
                accent_badge_text: Color32::from_rgb(0x7C, 0xA9, 0xFC),
                diff_add_bg: Color32::from_rgb(0x00, 0x33, 0x26),
                diff_add_text: Color32::from_rgb(0x39, 0xD7, 0x86),
                diff_del_bg: Color32::from_rgb(0x57, 0x0F, 0x07),
                diff_del_text: Color32::from_rgb(0xFF, 0xA7, 0x9D),
            }
        } else {
            Self {
                is_dark: false,
                background: Color32::from_rgb(0xF8, 0xF8, 0xF8),
                surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
                // One real step above surface, mirroring the dark palette:
                // a white `elevated` made buttons and inset containers
                // invisible on white cards.
                surface_elevated: Color32::from_rgb(0xF4, 0xF4, 0xF4),
                surface_hover: Color32::from_rgb(0xE9, 0xE9, 0xE9),
                surface_active: Color32::from_rgb(0xDA, 0xDA, 0xDA),
                // One step below hover: chips filled `subtle` must stay
                // visible when they sit on a `hover`-filled selected row.
                surface_subtle: Color32::from_rgb(0xDA, 0xDA, 0xDA),
                border: Color32::from_rgb(0xDA, 0xDA, 0xDA),
                text_primary: Color32::from_rgb(0x13, 0x13, 0x13),
                text_body: Color32::from_rgb(0x29, 0x29, 0x29),
                text_secondary: Color32::from_rgb(0x50, 0x50, 0x50),
                text_tertiary: Color32::from_rgb(0x71, 0x71, 0x71),
                accent: Color32::from_rgb(0x3B, 0x63, 0xFB),
                accent_hover: Color32::from_rgb(0x27, 0x4D, 0xEA),
                accent_visual: Color32::from_rgb(0x4B, 0x75, 0xFF),
                accent_badge_bg: Color32::from_rgb(0xE5, 0xF0, 0xFE),
                accent_badge_text: Color32::from_rgb(0x27, 0x4D, 0xEA),
                diff_add_bg: Color32::from_rgb(0xD7, 0xF7, 0xE1),
                diff_add_text: Color32::from_rgb(0x02, 0x5D, 0x3C),
                diff_del_bg: Color32::from_rgb(0xFF, 0xEB, 0xE8),
                diff_del_text: Color32::from_rgb(0x9C, 0x21, 0x13),
            }
        }
    }
}

/// Configures egui Visuals to match Spectrum 2.
/// Accent is reserved for primary actions and selection; hover states change
/// the fill only (S2 never strokes hovered widgets with the accent).
pub fn configure_egui_theme(ctx: &egui::Context, theme: &SpectrumTheme) {
    let mut v = if theme.is_dark { Visuals::dark() } else { Visuals::light() };

    v.dark_mode = theme.is_dark;
    // Do NOT override text color globally; it broke selection readability.
    v.override_text_color = None;

    v.panel_fill = theme.background;
    v.window_fill = theme.surface;
    v.window_stroke = Stroke::new(1.0, theme.border);
    v.window_corner_radius = CornerRadius::same(10);

    v.widgets.noninteractive.bg_fill = theme.surface;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, theme.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, theme.text_primary);
    v.widgets.noninteractive.corner_radius = CornerRadius::same(4);

    v.widgets.inactive.bg_fill = theme.surface_elevated;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, theme.border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, theme.text_primary);
    v.widgets.inactive.corner_radius = CornerRadius::same(8);

    v.widgets.hovered.bg_fill = theme.surface_hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, theme.border);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, theme.text_primary);
    v.widgets.hovered.corner_radius = CornerRadius::same(8);

    v.widgets.active.bg_fill = theme.surface_active;
    v.widgets.active.bg_stroke = Stroke::new(1.0, theme.border);
    v.widgets.active.fg_stroke = Stroke::new(1.0, theme.text_primary);
    v.widgets.active.corner_radius = CornerRadius::same(8);

    // The "open" state doubles as the ComboBox DROPDOWN menu surface:
    // elevated fill + real border, like any Spectrum popover. egui's
    // default menu fill here looked like a plain black slab.
    v.widgets.open.bg_fill = theme.surface_elevated;
    v.widgets.open.bg_stroke = Stroke::new(1.0, theme.border);
    v.widgets.open.fg_stroke = Stroke::new(1.0, theme.text_primary);
    v.widgets.open.corner_radius = CornerRadius::same(6);

    // Selection tokens feed a dropdown's SELECTED item (and the
    // text-selection highlight): the Spectrum blue-800/200 badge pair -
    // the raw accent fill with white text was below readable contrast.
    v.selection.bg_fill = theme.accent_badge_bg;
    v.selection.stroke = Stroke::new(1.0, theme.accent_badge_text);

    ctx.set_visuals(v);

    // Spectrum's type scale: 12 detail, 14 body and controls, 18 heading.
    // Set once: replacing the styles every frame would invalidate layout caches.
    static TEXT_STYLES: std::sync::Once = std::sync::Once::new();
    TEXT_STYLES.call_once(|| {
        ctx.all_styles_mut(|style| {
            use egui::{FontFamily, TextStyle};
            style.text_styles = [
                (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
                (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
                (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
                (TextStyle::Heading, FontId::new(18.0, FontFamily::Proportional)),
                (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
            ]
            .into();
        });
    });
}

/// Loads the text fonts, then the Spectrum icons as the last fallback.
pub fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut add = |name: &str, data: egui::FontData, first: bool| {
        fonts.font_data.insert(name.to_owned(), std::sync::Arc::new(data));
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            if first {
                family.insert(0, name.to_owned());
            } else {
                family.push(name.to_owned());
            }
        }
    };
    add_text_fonts(&mut add);
    add(
        "RefinedIcons",
        egui::FontData::from_static(include_bytes!("../assets/refined-icons.ttf")),
        false,
    );
    ctx.set_fonts(fonts);
}

/// Segoe UI with Segoe UI Symbol as fallback. Segoe MDL2 Assets keeps icons
/// of custom actions made before 0.4.1 visible.
#[cfg(windows)]
fn add_text_fonts(add: &mut impl FnMut(&str, egui::FontData, bool)) {
    let font_dir = std::path::Path::new("C:\\Windows\\Fonts");
    for (name, file, first) in [
        ("SegoeUI", "segoeui.ttf", true),
        ("SegoeUISymbol", "seguisym.ttf", false),
        ("SegoeMDL2", "segmdl2.ttf", false),
    ] {
        if let Ok(bytes) = std::fs::read(font_dir.join(file)) {
            add(name, egui::FontData::from_owned(bytes), first);
        }
    }
}

/// The desktop's sans-serif font when fontconfig finds one that covers
/// sub- and superscript letters, ahead of egui's bundled font.
#[cfg(not(windows))]
fn add_text_fonts(add: &mut impl FnMut(&str, egui::FontData, bool)) {
    let system = std::process::Command::new("fc-match")
        .args(["--format=%{index}:%{file}", "sans-serif:charset=2099"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|found| {
            let (index, file) = found.split_once(':')?;
            Some((index.parse().ok()?, std::fs::read(file).ok()?))
        });
    if let Some((index, bytes)) = system {
        let mut data = egui::FontData::from_owned(bytes);
        data.index = index;
        add("SystemSans", data, true);
    }
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

/// Spectrum 2 button: always carries an icon + label so no call site can end
/// up inconsistent. Pill-shaped, accent fill only when primary.
pub fn spectrum_button(
    ui: &mut Ui,
    icon: Option<&str>,
    label: &str,
    primary: bool,
    theme: &SpectrumTheme,
) -> Response {
    styled_button(ui, icon, label, primary, 32.0, 13.0, theme)
}

/// Compact density variant for in-row controls (e.g. history card headers),
/// so buttons do not tower over the surrounding text.
pub fn spectrum_button_compact(
    ui: &mut Ui,
    icon: Option<&str>,
    label: &str,
    primary: bool,
    theme: &SpectrumTheme,
) -> Response {
    styled_button(ui, icon, label, primary, 26.0, 12.0, theme)
}

/// Spectrum keyboard focus indicator: a 2 px accent ring with a 2 px gap.
/// Pointer clicks also focus egui widgets, so the ring shows only while
/// the keyboard moves focus.
pub fn focus_ring(ui: &Ui, response: &Response, radius: f32, theme: &SpectrumTheme) {
    if response.has_focus() && !ui.input(|i| i.pointer.any_down() || i.pointer.any_released()) {
        ui.painter().rect_stroke(
            response.rect.expand(2.0),
            CornerRadius::same((radius + 2.0).round() as u8),
            Stroke::new(2.0, theme.accent),
            StrokeKind::Outside,
        );
    }
}

fn styled_button(
    ui: &mut Ui,
    icon: Option<&str>,
    label: &str,
    primary: bool,
    height: f32,
    font_size: f32,
    theme: &SpectrumTheme,
) -> Response {
    let text = match icon {
        Some(ic) => format!("{ic}  {label}"),
        None => label.to_string(),
    };
    let rich = RichText::new(text)
        .font(FontId::proportional(font_size))
        .color(if primary { Color32::WHITE } else { theme.text_primary });
    let rich = if primary { rich.strong() } else { rich };

    // Spectrum 2 buttons are pill shaped.
    let corner = CornerRadius::same((height / 2.0).round() as u8);
    let button = if primary {
        egui::Button::new(rich)
            .fill(theme.accent)
            .stroke(Stroke::NONE)
    } else {
        egui::Button::new(rich)
            .fill(theme.surface_elevated)
            .stroke(Stroke::new(1.0, theme.border))
    };
    let response = ui.add(button.min_size(Vec2::new(0.0, height)).corner_radius(corner));
    focus_ring(ui, &response, height / 2.0, theme);
    response
}

/// Spectrum 2 sliding switch (pill track, white knob).
pub fn spectrum_switch(ui: &mut Ui, on: &mut bool, theme: &SpectrumTheme) -> Response {
    let desired_size = Vec2::new(40.0, 22.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }

    if ui.is_rect_visible(rect) {
        let how_on = ui.ctx().animate_bool_responsive(response.id, *on);

        let bg = if *on { theme.accent } else { theme.surface_active };
        let radius = (rect.height() / 2.0).round() as u8;
        ui.painter().rect_filled(rect, CornerRadius::same(radius), bg);
        if !*on {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(radius),
                Stroke::new(1.0, theme.border),
                StrokeKind::Inside,
            );
        }

        let knob_radius = (rect.height() - 6.0) / 2.0;
        let min_x = rect.min.x + 3.0 + knob_radius;
        let max_x = rect.max.x - 3.0 - knob_radius;
        let knob_x = egui::lerp(min_x..=max_x, how_on);
        let knob_center = Pos2::new(knob_x, rect.center().y);

        ui.painter().circle(knob_center, knob_radius, Color32::WHITE, Stroke::NONE);
        focus_ring(ui, &response, rect.height() / 2.0, theme);
    }

    response
}

/// Spectrum 2 quiet badge (pill) - e.g. action names, word counts.
pub fn spectrum_badge(ui: &mut Ui, text: &str, bg_color: Color32, text_color: Color32) {
    ui.scope(|ui| {
        egui::Frame::new()
            .fill(bg_color)
            .corner_radius(CornerRadius::same(4))
            .inner_margin(egui::Margin::symmetric(8, 3))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(text)
                        .font(FontId::proportional(12.0))
                        .color(text_color),
                );
            });
    });
}

/// Standard Spectrum card frame shared by every Studio panel card.
pub fn card_frame(theme: &SpectrumTheme, inner_margin: egui::Margin) -> egui::Frame {
    egui::Frame::new()
        .fill(theme.surface)
        .stroke(Stroke::new(1.0, theme.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(inner_margin)
}

/// Spectrum-styled continuous slider: a visible 4px track (egui's default
/// rail paints with the same color as the card surface and disappears),
/// an accent trailing fill, and a solid knob.
pub fn spectrum_slider(
    ui: &mut Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f32,
    theme: &SpectrumTheme,
) -> Response {
    let width = ui.available_width().min(320.0);
    let (rect, mut response) =
        ui.allocate_exact_size(Vec2::new(width, 20.0), Sense::drag());

    if response.dragged() {
        if let Some(p) = response.interact_pointer_pos() {
            let t = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            let raw = egui::lerp(range.clone(), t);
            let snapped = (raw / step).round() * step;
            let clamped = snapped.clamp(*range.start(), *range.end());
            if (clamped - *value).abs() > f32::EPSILON {
                *value = clamped;
                response.mark_changed();
            }
        }
    }

    let (start, end) = (*range.start(), *range.end());
    let t = ((*value - start) / (end - start).max(f32::EPSILON)).clamp(0.0, 1.0);
    let cy = rect.center().y;

    // Track
    ui.painter().rect_filled(
        Rect::from_min_max(
            Pos2::new(rect.left(), cy - 2.0),
            Pos2::new(rect.right(), cy + 2.0),
        ),
        CornerRadius::same(2),
        theme.surface_active,
    );
    // Trailing fill
    let fill_x = rect.left() + t * rect.width();
    ui.painter().rect_filled(
        Rect::from_min_max(
            Pos2::new(rect.left(), cy - 2.0),
            Pos2::new(fill_x, cy + 2.0),
        ),
        CornerRadius::same(2),
        theme.accent,
    );
    // Knob: small solid circle so it reads as part of the track, not a
    // separate black puck.
    let knob_r = 6.0;
    let knob_x = fill_x.clamp(rect.left() + knob_r, rect.right() - knob_r);
    ui.painter().circle(
        Pos2::new(knob_x, cy),
        knob_r,
        theme.text_primary,
        Stroke::new(2.0, theme.surface),
    );

    response
}

/// Renders the refined-vs-original diff as a single wrapped text flow:
/// one layout job so all sections wrap at the true panel width, with
/// opaque Spectrum red/green subtle backgrounds behind the changed runs.
/// The strikethrough is painted from the galley's own glyph data: the
/// strike spans each deleted run exactly as the run's background does
/// (glyph edges plus the same 1px `expand_bg` pad), seated slightly below
/// mid-x-height, anchored on the glyph baseline.
pub fn render_diff_view(ui: &mut Ui, chunks: &[DiffChunk], theme: &SpectrumTheme) {
    if chunks.is_empty() {
        return;
    }

    let font = FontId::proportional(14.0);
    let mut job = eframe::egui::text::LayoutJob::default();
    // Character ranges of the Delete chunks within the laid-out text,
    // used to place the manual strikethrough.
    let mut del_ranges: Vec<(usize, usize)> = Vec::new();

    let mut prev_kind: Option<DiffKind> = None;
    for chunk in chunks {
        // A red run butting straight into a green run reads as one smear;
        // a thin neutral gap separates the before from the after.
        if matches!((prev_kind, chunk.kind), (Some(DiffKind::Delete), DiffKind::Insert))
            || matches!((prev_kind, chunk.kind), (Some(DiffKind::Insert), DiffKind::Delete))
        {
            job.append(
                " ",
                1.0,
                TextFormat {
                    font_id: font.clone(),
                    color: theme.text_body,
                    background: Color32::TRANSPARENT,
                    ..Default::default()
                },
            );
        }
        let start = job.text.chars().count();
        let (color, background) = match chunk.kind {
            DiffKind::Equal => (theme.text_body, Color32::TRANSPARENT),
            DiffKind::Delete => (theme.diff_del_text, theme.diff_del_bg),
            DiffKind::Insert => (theme.diff_add_text, theme.diff_add_bg),
        };
        job.append(
            &chunk.text,
            1.0,
            TextFormat {
                font_id: font.clone(),
                color,
                background,
                ..Default::default()
            },
        );
        let end = job.text.chars().count();
        if chunk.kind == DiffKind::Delete {
            del_ranges.push((start, end));
        }
        prev_kind = Some(chunk.kind);
    }

    job.wrap.max_width = ui.available_width().max(64.0);
    let galley = ui.painter().layout_job(job);
    let (rect, _response) = ui.allocate_exact_size(galley.size(), Sense::hover());
    let origin = rect.min;
    ui.painter().galley(origin, galley.clone(), Color32::WHITE);

    // Strikethrough pass. Runs are built from the galley glyph stream so
    // each strike piece spans its deleted run edge-to-edge with the run's
    // background (glyph edges + the same 1px pad egui gives backgrounds),
    // breaking exactly where wrapping breaks the run.
    let in_del = |c: usize| del_ranges.iter().any(|(s, e)| c >= *s && c < *e);
    let mut base = 0usize; // chars covered by all previous rows
    for placed in &galley.rows {
        let glyphs = &placed.row.glyphs;
        let mut run: Option<(usize, usize)> = None; // glyph index range

        let strike_run = |placed: &eframe::egui::epaint::text::PlacedRow,
                              glyphs: &Vec<eframe::egui::epaint::text::Glyph>,
                              run: (usize, usize),
                              painter: &egui::Painter| {
            let (a, b) = run;
            // Trailing space glyphs get no background paint (their rect is
            // dropped at paint time in some wrap positions), so the strike
            // must stop at the last visible glyph - mid-run spaces stay.
            let mut b = b;
            while b > a + 1 && glyphs[b - 1].chr.is_whitespace() {
                b -= 1;
            }
            if glyphs[b - 1].chr.is_whitespace() && b == a + 1 {
                // run is only whitespace: nothing to strike
                return;
            }
            let first = &glyphs[a];
            let last = &glyphs[b - 1];
            let baseline = placed.pos.y + first.pos.y;
            let x0 = placed.pos.x + first.pos.x - 1.0;
            let x1 = placed.pos.x + last.pos.x + last.advance_width + 1.0;
            let y = origin.y + baseline - first.font_ascent * 0.25;
            if x1 > x0 {
                painter.line_segment(
                    [Pos2::new(origin.x + x0, y), Pos2::new(origin.x + x1, y)],
                    Stroke::new(1.0, theme.diff_del_text),
                );
            }
        };

        for (gi, _glyph) in glyphs.iter().enumerate() {
            if in_del(base + gi) {
                match run.as_mut() {
                    Some((_, ref mut b)) => *b = gi + 1,
                    None => run = Some((gi, gi + 1)),
                }
            } else if let Some(r) = run.take() {
                strike_run(placed, glyphs, r, ui.painter());
            }
        }
        if let Some(r) = run.take() {
            strike_run(placed, glyphs, r, ui.painter());
        }

        base += glyphs.len() + placed.ends_with_newline as usize;
    }
}

/// Sized verb button with optional leading icon and trailing keycap chip,
/// e.g. "[icon] Copy  [Ctrl+C]". Width is computed from the measured
/// galley widths, so long localized labels or icons never clip.
pub fn kbd_button(
    ui: &mut Ui,
    icon: Option<&str>,
    label: &str,
    kbd: Option<&str>,
    primary: bool,
    theme: &SpectrumTheme,
) -> Response {
    let height = 32.0;
    let label_color = if primary { Color32::WHITE } else { theme.text_primary };
    let kbd_color = if primary { Color32::WHITE } else { theme.text_secondary };

    let font = FontId::proportional(14.0);
    let label_galley = ui
        .painter()
        .layout(label.to_string(), font, label_color, f32::INFINITY);
    let icon_galley = icon.map(|ic| {
        ui.painter()
            .layout(ic.to_string(), FontId::proportional(14.0), label_color, f32::INFINITY)
    });
    let kbd_galley = kbd.map(|k| {
        ui.painter()
            .layout(k.to_string(), FontId::proportional(12.0), kbd_color, f32::INFINITY)
    });

    let pad = 10.0;
    let icon_w = icon_galley.as_ref().map(|g| g.size().x + 8.0).unwrap_or(0.0);
    let mut width = pad + icon_w + label_galley.size().x;
    if let Some(g) = &kbd_galley {
        width += 12.0 + g.size().x + 2.0 * 5.0; // chip + horizontal padding
    }
    width += pad;
    let width = width.max(64.0);

    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());

    let fill = if response.hovered() || response.has_focus() {
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
    let corner = CornerRadius::same((height / 2.0) as u8);
    focus_ring(ui, &response, height / 2.0, theme);
    ui.painter().rect_filled(rect, corner, fill);
    if !primary {
        ui.painter()
            .rect_stroke(rect, corner, Stroke::new(1.0, theme.border), StrokeKind::Inside);
    }

    let mut x = rect.left() + pad;
    if let Some(g) = &icon_galley {
        let y = rect.center().y - g.size().y / 2.0;
        ui.painter().galley(Pos2::new(x, y), g.clone(), Color32::WHITE);
        x += g.size().x + 8.0;
    }
    ui.painter().galley(
        Pos2::new(x, rect.center().y - label_galley.size().y / 2.0),
        label_galley,
        Color32::WHITE,
    );

    if let Some(g) = &kbd_galley {
        let chip_size = Vec2::new(g.size().x + 10.0, g.size().y + 4.0);
        let chip_rect = Rect::from_min_size(
            Pos2::new(rect.right() - pad - chip_size.x, rect.center().y - chip_size.y / 2.0),
            chip_size,
        );
        let chip_fill = if primary {
            Color32::from_black_alpha(50)
        } else {
            theme.surface_subtle
        };
        ui.painter()
            .rect_filled(chip_rect, CornerRadius::same(4), chip_fill);
        ui.painter().galley(
            Pos2::new(
                chip_rect.center().x - g.size().x / 2.0,
                chip_rect.center().y - g.size().y / 2.0,
            ),
            g.clone(),
            Color32::WHITE,
        );
    }

    response
}
