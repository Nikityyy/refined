//! Linux selection capture and replacement.
//!
//! Highlighted text is already in the PRIMARY selection, so capturing it
//! needs no simulated copy and leaves the clipboard untouched. The result is
//! placed on the clipboard as HTML and text, then pasted with Ctrl+V.

use arboard::{Clipboard, GetExtLinux, LinuxClipboardKind};
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone)]
pub struct CapturedSelection {
    pub text: String,
    pub target_window: isize,
    pub source: crate::hud::Source,
}

/// On X11 the clipboard owner must stay alive to serve pastes.
fn clipboard() -> Option<std::sync::MutexGuard<'static, Clipboard>> {
    static CLIPBOARD: OnceLock<Option<Mutex<Clipboard>>> = OnceLock::new();
    CLIPBOARD
        .get_or_init(|| Clipboard::new().ok().map(Mutex::new))
        .as_ref()?
        .lock()
        .ok()
}

pub fn get_clipboard_text() -> Option<String> {
    clipboard()?.get_text().ok()
}

pub fn set_clipboard_text(text: &str) -> bool {
    clipboard().is_some_and(|mut clipboard| clipboard.set_text(text).is_ok())
}

/// Reads the highlighted text, or the clipboard text when nothing is
/// highlighted, with `<sub>` and `<sup>` tags restored from the HTML
/// version when the source app offers one. Images carry no text.
pub fn capture_selection_with_window() -> Option<CapturedSelection> {
    use crate::hud::Source;
    let mut clipboard = clipboard()?;
    let mut read = |kind: LinuxClipboardKind| {
        let text = clipboard.get().clipboard(kind).text().ok()?;
        if text.trim().is_empty() {
            return None;
        }
        let html = clipboard.get().clipboard(kind).html().ok();
        Some(crate::rich_text::mark_scripts(&text, html.as_deref(), None).unwrap_or(text))
    };
    let (text, source) = match read(LinuxClipboardKind::Primary) {
        Some(text) => (text, Source::Selection),
        None => match read(LinuxClipboardKind::Clipboard) {
            Some(text) => (text, Source::Clipboard),
            None => (String::new(), Source::Nothing),
        },
    };
    let target_window = crate::x11::active_window().map_or(0, |window| window as isize);
    Some(CapturedSelection { text, target_window, source })
}

/// Copies a refinement with the same formats used for in-place replacement.
pub fn copy_refinement(text: &str) -> bool {
    let Some(mut clipboard) = clipboard() else { return false };
    match crate::rich_text::render(text) {
        Some(rich) => clipboard.set_html(rich_html(&rich.html), Some(rich.plain)).is_ok(),
        None => clipboard.set_text(text).is_ok(),
    }
}

/// Linux apps read plain HTML, without the Windows clipboard header.
fn rich_html(windows_html: &str) -> String {
    let start = windows_html.find("<html>").unwrap_or(0);
    windows_html[start..].trim_end_matches('\0').to_owned()
}

/// Always leaves the result on the clipboard, so Ctrl+V works even where
/// the paste cannot be sent (native Wayland windows).
pub fn replace_selection_in_target(target_window: isize, new_text: &str) -> bool {
    copy_refinement(new_text)
        && target_window > 0
        && crate::x11::activate(target_window as u32)
        && crate::x11::send_ctrl(u32::from(b'v'))
}

pub fn undo_in_target(target_window: isize) -> bool {
    target_window > 0
        && crate::x11::activate(target_window as u32)
        && crate::x11::send_ctrl(u32::from(b'z'))
}
