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
}

/// Reads Unicode text currently in the Windows clipboard with retries and bounds checks.
pub fn get_clipboard_text() -> Option<String> {
    unsafe {
        let mut opened = false;
        for _ in 0..5 {
            if OpenClipboard(None).is_ok() {
                opened = true;
                break;
            }
            sleep(Duration::from_millis(8));
        }
        if !opened {
            return None;
        }

        let result = (|| {
            if IsClipboardFormatAvailable(CF_UNICODETEXT).is_err() {
                return None;
            }
            let handle = GetClipboardData(CF_UNICODETEXT).ok()?;
            let hmem = HGLOBAL(handle.0);
            let ptr = GlobalLock(hmem) as *const u16;
            if ptr.is_null() {
                return None;
            }
            let max_words = GlobalSize(hmem) / std::mem::size_of::<u16>();
            let mut len = 0;
            while len < max_words && *ptr.add(len) != 0 {
                len += 1;
            }
            let slice = std::slice::from_raw_parts(ptr, len);
            let text = String::from_utf16_lossy(slice);
            let _ = GlobalUnlock(hmem);
            Some(text)
        })();

        let _ = CloseClipboard();
        result
    }
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
                    && set_clipboard_bytes(CF_UNICODETEXT, text_bytes)
                    && html
                        .zip(html_format)
                        .is_none_or(|(html, format)| set_clipboard_bytes(format, html.as_bytes()));
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

/// Captures selected text and target window handle.
/// Only returns Some if text was actively selected.
/// Never corrupts clipboard if nothing was selected.
pub fn capture_selection_with_window() -> Option<CapturedSelection> {
    let target_hwnd = unsafe { GetForegroundWindow() };
    if target_hwnd.0.is_null() {
        return None;
    }

    let old_seq = unsafe { GetClipboardSequenceNumber() };

    send_control_key(VK_C, true);

    // Poll for the clipboard sequence change. Heavy applications (big
    // Word documents, some terminals) take longer than one fast poll
    // window, so if nothing changed yet, fire a second Ctrl+C and wait a
    // longer second round instead of giving up silently.
    let start = std::time::Instant::now();
    let mut changed = false;
    while start.elapsed() < Duration::from_millis(120) {
        let seq = unsafe { GetClipboardSequenceNumber() };
        if seq != old_seq {
            changed = true;
            break;
        }
        sleep(Duration::from_millis(8));
    }
    if !changed && unsafe { GetForegroundWindow() } == target_hwnd {
        send_control_key(VK_C, true);
        // A fresh window: slow responders (heavy Word documents, some
        // terminals) need their own budget, not the tail of round one.
        let second = std::time::Instant::now();
        while second.elapsed() < Duration::from_millis(300) {
            let seq = unsafe { GetClipboardSequenceNumber() };
            if seq != old_seq {
                changed = true;
                break;
            }
            sleep(Duration::from_millis(16));
        }
    }

    if changed && unsafe { GetForegroundWindow() } == target_hwnd {
        if let Some(text) = get_clipboard_text() {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(CapturedSelection { text, target_hwnd });
            }
        }
    }

    // Nothing was selected; do NOT touch or empty clipboard!
    None
}

/// Replaces text in target window: restores target window focus, normalizes CRLF,
/// pastes via Ctrl+V.
pub fn replace_selection_in_target(target_hwnd: HWND, new_text: &str, structure: bool) -> bool {
    let owner = unsafe { GetActiveWindow() };
    let owner = (!owner.0.is_null()).then_some(owner);
    let rich = structure
        .then(|| crate::rich_text::render(new_text))
        .flatten();
    if !focus_target(target_hwnd) {
        return false;
    }
    let text = rich.as_ref().map_or(new_text, |rich| rich.plain.as_str());
    let normalized = text.replace("\r\n", "\n").replace('\n', "\r\n");
    if !write_clipboard(
        &normalized,
        rich.as_ref().map(|rich| rich.html.as_str()),
        owner,
    ) || unsafe { GetForegroundWindow() } != target_hwnd
    {
        return false;
    }
    if rich.is_some() {
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
