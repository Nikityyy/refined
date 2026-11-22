//! Windows clipboard and text selection management.
//! Provides robust text capture, Alt-modifier release, target window focus restoration,
//! and format-preserving text replacement.

use std::ptr;
use std::thread::sleep;
use std::time::Duration;
use windows::core::w;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetActiveWindow, GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_C, VK_CONTROL, VK_LSHIFT, VK_MENU,
    VK_RSHIFT, VK_V, VK_Z,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};

const CF_UNICODETEXT: u32 = 13;

#[derive(Debug, Clone)]
pub struct CapturedSelection {
    pub text: String,
    pub target_hwnd: HWND,
    pub source: crate::hud::Source,
}

/// Reads the copied text, with `<sub>` and `<sup>` tags restored from the
/// HTML or RTF copy of the same selection (Word, browsers, Acrobat).
fn get_selection_text() -> Option<String> {
    unsafe {
        let html_format = RegisterClipboardFormatW(w!("HTML Format"));
        let rtf_format = RegisterClipboardFormatW(w!("Rich Text Format"));
        if !open_clipboard() {
            return None;
        }
        let text = read_unicode_text();
        let html = (html_format != 0)
            .then(|| read_bytes(html_format))
            .flatten()
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
        let rtf = (rtf_format != 0).then(|| read_bytes(rtf_format)).flatten();
        let _ = CloseClipboard();
        let text = text?;
        Some(crate::rich_text::mark_scripts(&text, html.as_deref(), rtf.as_deref()).unwrap_or(text))
    }
}

/// Copies one clipboard format while the clipboard is open.
unsafe fn read_bytes(format: u32) -> Option<Vec<u8>> {
    if IsClipboardFormatAvailable(format).is_err() {
        return None;
    }
    let hmem = HGLOBAL(GetClipboardData(format).ok()?.0);
    let ptr = GlobalLock(hmem) as *const u8;
    if ptr.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(ptr, GlobalSize(hmem));
    let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(bytes.len());
    let bytes = bytes[..end].to_vec();
    let _ = GlobalUnlock(hmem);
    Some(bytes)
}

/// Reads CF_UNICODETEXT while the clipboard is open.
unsafe fn read_unicode_text() -> Option<String> {
    if IsClipboardFormatAvailable(CF_UNICODETEXT).is_err() {
        return None;
    }
    let hmem = HGLOBAL(GetClipboardData(CF_UNICODETEXT).ok()?.0);
    let ptr = GlobalLock(hmem) as *const u16;
    if ptr.is_null() {
        return None;
    }
    let max_words = GlobalSize(hmem) / std::mem::size_of::<u16>();
    let mut len = 0;
    while len < max_words && *ptr.add(len) != 0 {
        len += 1;
    }
    let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
    let _ = GlobalUnlock(hmem);
    Some(text)
}

/// Reads Unicode text currently in the Windows clipboard with retries and bounds checks.
pub fn get_clipboard_text() -> Option<String> {
    unsafe {
        open_clipboard().then(|| {
            let text = read_unicode_text();
            let _ = CloseClipboard();
            text
        })?
    }
}

unsafe fn open_clipboard() -> bool {
    for _ in 0..5 {
        if OpenClipboard(None).is_ok() {
            return true;
        }
        sleep(Duration::from_millis(8));
    }
    false
}

/// Sets Unicode text into the Windows clipboard safely without leaking memory.
pub fn set_clipboard_text(text: &str) -> bool {
    write_clipboard(text, None, None)
}

fn write_clipboard(text: &str, html: Option<&str>, owner: Option<HWND>) -> bool {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let html_format = html.map(|_| RegisterClipboardFormatW(w!("HTML Format")));
        if html_format == Some(0) {
            return false;
        }
        for _ in 0..5 {
            if OpenClipboard(owner).is_ok() {
                let text_bytes = std::slice::from_raw_parts(
                    wide.as_ptr().cast::<u8>(),
                    std::mem::size_of_val(wide.as_slice()),
                );
                let written = EmptyClipboard().is_ok()
                    && html
                        .zip(html_format)
                        .is_none_or(|(html, format)| set_clipboard_bytes(format, html.as_bytes()))
                    && set_clipboard_bytes(CF_UNICODETEXT, text_bytes);
                let _ = CloseClipboard();
                return written;
            }
            sleep(Duration::from_millis(8));
        }
    }
    false
}

/// Transfers ownership to Windows on success and frees the allocation on error.
unsafe fn set_clipboard_bytes(format: u32, bytes: &[u8]) -> bool {
    let Ok(mem) = GlobalAlloc(GMEM_MOVEABLE, bytes.len()) else {
        return false;
    };
    let target = GlobalLock(mem);
    if !target.is_null() {
        ptr::copy_nonoverlapping(bytes.as_ptr(), target.cast(), bytes.len());
        let _ = GlobalUnlock(mem);
        if SetClipboardData(format, Some(HANDLE(mem.0))).is_ok() {
            return true;
        }
    }
    let _ = GlobalFree(Some(mem));
    false
}

fn key_event(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let flags = if up {
        KEYEVENTF_KEYUP
    } else {
        KEYBD_EVENT_FLAGS(0)
    };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send_control_key(key: VIRTUAL_KEY, release_modifiers: bool) -> bool {
    let mut inputs = Vec::with_capacity(7);
    if release_modifiers {
        inputs.extend(
            [VK_MENU, VK_LSHIFT, VK_RSHIFT]
                .into_iter()
                .filter_map(|modifier| {
                    (unsafe { GetAsyncKeyState(i32::from(modifier.0)) } < 0)
                        .then(|| key_event(modifier, true))
                }),
        );
    }
    inputs.extend([
        key_event(VK_CONTROL, false),
        key_event(key, false),
        key_event(key, true),
        key_event(VK_CONTROL, true),
    ]);
    unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) == inputs.len() as u32 }
}

/// Most apps answer Ctrl+C within this time. Without an answer by then,
/// the palette opens with the clipboard text while [`PendingSelection`]
/// keeps watching for a slow app's copy.
const QUICK_COPY: Duration = Duration::from_millis(60);
/// Heavy documents can take longer; the first copy gets this long in total.
const FIRST_COPY: Duration = Duration::from_millis(120);
/// A second Ctrl+C for apps that dropped the first, with its own budget.
const SECOND_COPY: Duration = Duration::from_millis(300);

fn clipboard_changed(old_seq: u32, budget: Duration, poll: Duration, since: std::time::Instant) -> bool {
    while since.elapsed() < budget {
        if unsafe { GetClipboardSequenceNumber() } != old_seq {
            return true;
        }
        sleep(poll);
    }
    unsafe { GetClipboardSequenceNumber() != old_seq }
}

/// Captures the selected text and the target window. Without a quick
/// answer to Ctrl+C, returns the clipboard text at once, together with a
/// [`PendingSelection`] that finishes checking for a selection. Returns
/// None only when focus moved away during the capture.
pub fn capture_selection_with_window() -> Option<(CapturedSelection, Option<PendingSelection>)> {
    use crate::hud::Source;
    let target_hwnd = unsafe { GetForegroundWindow() };
    if target_hwnd.0.is_null() {
        return None;
    }
    let old_seq = unsafe { GetClipboardSequenceNumber() };
    send_control_key(VK_C, true);
    let started = std::time::Instant::now();
    let changed = clipboard_changed(old_seq, QUICK_COPY, Duration::from_millis(4), started);
    if unsafe { GetForegroundWindow() } != target_hwnd {
        return None;
    }
    // Without a selection the copy changed nothing, so the clipboard still
    // holds what the user copied earlier. Images and files carry no text.
    let text = get_selection_text().filter(|text| !text.trim().is_empty());
    let source = match (&text, changed) {
        (None, _) => Source::Nothing,
        (Some(_), true) => Source::Selection,
        (Some(_), false) => Source::Clipboard,
    };
    let capture = CapturedSelection {
        text: text.unwrap_or_default(),
        target_hwnd,
        source,
    };
    let pending = (!changed).then_some(PendingSelection {
        target_hwnd,
        old_seq,
        started,
    });
    Some((capture, pending))
}

/// The rest of the selection check after the palette has opened.
pub struct PendingSelection {
    target_hwnd: HWND,
    old_seq: u32,
    started: std::time::Instant,
}

impl PendingSelection {
    /// Waits out the copy budgets. Returns the selected text if a slow app
    /// answered after all, or None when nothing was selected.
    pub fn finish(self) -> Option<String> {
        let mut changed =
            clipboard_changed(self.old_seq, FIRST_COPY, Duration::from_millis(8), self.started);
        if !changed {
            if unsafe { GetForegroundWindow() } == self.target_hwnd {
                send_control_key(VK_C, true);
                let second = std::time::Instant::now();
                changed =
                    clipboard_changed(self.old_seq, SECOND_COPY, Duration::from_millis(16), second);
            } else {
                // The palette has focus, so a second copy would not reach the
                // editor. The first one, already queued there, gets the time.
                changed = clipboard_changed(
                    self.old_seq,
                    FIRST_COPY + SECOND_COPY,
                    Duration::from_millis(16),
                    self.started,
                );
            }
        }
        changed
            .then(get_selection_text)
            .flatten()
            .filter(|text| !text.trim().is_empty())
    }
}

/// Copies a refinement with the same clipboard formats used for in-place replacement.
pub fn copy_refinement(text: &str) -> bool {
    write_refinement(text, clipboard_owner()).is_some()
}

/// Returns whether the clipboard contains HTML, or None if the write failed.
fn write_refinement(text: &str, owner: Option<HWND>) -> Option<bool> {
    let rich = crate::rich_text::render(text);
    let plain = rich.as_ref().map_or(text, |rich| rich.plain.as_str());
    let normalized = plain.replace("\r\n", "\n").replace('\n', "\r\n");
    let html = rich.as_ref().map(|rich| rich.html.as_str());
    write_clipboard(&normalized, html, owner).then_some(html.is_some())
}

fn clipboard_owner() -> Option<HWND> {
    let owner = unsafe { GetActiveWindow() };
    (!owner.0.is_null()).then_some(owner)
}

/// Replaces selected text using the richest clipboard format the editor accepts.
pub fn replace_selection_in_target(target_hwnd: HWND, new_text: &str) -> bool {
    let owner = clipboard_owner();
    if !focus_target(target_hwnd) {
        return false;
    }
    let Some(has_html) = write_refinement(new_text, owner) else {
        return false;
    };
    if unsafe { GetForegroundWindow() } != target_hwnd {
        return false;
    }
    if has_html {
        if let Some(pasted) = crate::word::paste_html(target_hwnd) {
            return pasted;
        }
    }
    send_control_key(VK_V, false)
}

pub fn undo_in_target(target_hwnd: HWND) -> bool {
    focus_target(target_hwnd) && send_control_key(VK_Z, false)
}

fn focus_target(target_hwnd: HWND) -> bool {
    if target_hwnd.0.is_null() {
        return false;
    }
    unsafe {
        if GetForegroundWindow() == target_hwnd {
            return true;
        }
        if !SetForegroundWindow(target_hwnd).as_bool() {
            return false;
        }
        let deadline = std::time::Instant::now() + Duration::from_millis(80);
        while GetForegroundWindow() != target_hwnd {
            if std::time::Instant::now() >= deadline {
                return false;
            }
            sleep(Duration::from_millis(4));
        }
        true
    }
}
