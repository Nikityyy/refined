//! Windows clipboard and text selection management.
//! Provides robust text capture, Alt-modifier release, target window focus restoration,
//! and format-preserving text replacement.

use std::ptr;
use std::thread::sleep;
use std::time::Duration;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
    OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_C, VK_CONTROL, VK_MENU, VK_V,
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
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = wide.len() * std::mem::size_of::<u16>();

    unsafe {
        let mem = match GlobalAlloc(GMEM_MOVEABLE, bytes) {
            Ok(h) => h,
            Err(_) => return false,
        };
        let ptr = GlobalLock(mem);
        if ptr.is_null() {
            let _ = GlobalFree(Some(mem));
            return false;
        }
        ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, ptr as *mut u8, bytes);
        let _ = GlobalUnlock(mem);

        let mut opened = false;
        for _ in 0..5 {
            if OpenClipboard(None).is_ok() {
                opened = true;
                break;
            }
            sleep(Duration::from_millis(8));
        }
        if !opened {
            let _ = GlobalFree(Some(mem));
            return false;
        }

        let _ = EmptyClipboard();
        let ok = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(mem.0))).is_ok();
        let _ = CloseClipboard();
        if !ok {
            let _ = GlobalFree(Some(mem));
        }
        ok
    }
}

fn key_event(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let flags = if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) };
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

/// Releases physical Alt key if held, then sends Ctrl+C.
pub fn send_copy_with_alt_release() {
    unsafe {
        // If Alt is physically held down from Alt+Q, synthesize Alt KeyUp first
        // so target application sees pure Ctrl+C rather than Ctrl+Alt+C
        if (GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0 {
            let alt_up = [key_event(VK_MENU, true)];
            SendInput(&alt_up, std::mem::size_of::<INPUT>() as i32);
            sleep(Duration::from_millis(20));
        }

        let inputs = [
            key_event(VK_CONTROL, false),
            key_event(VK_C, false),
            key_event(VK_C, true),
            key_event(VK_CONTROL, true),
        ];
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

/// Sends Ctrl+C to copy whatever is currently selected in the active app.
#[allow(dead_code)]
pub fn send_copy() {
    send_copy_with_alt_release();
}

/// Sends Ctrl+V into the active window.
pub fn send_paste() {
    let inputs = [
        key_event(VK_CONTROL, false),
        key_event(VK_V, false),
        key_event(VK_V, true),
        key_event(VK_CONTROL, true),
    ];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
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

    send_copy_with_alt_release();

    // Poll for clipboard sequence change (120ms window for heavy apps like Word / Slack)
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

    if changed {
        if let Some(text) = get_clipboard_text() {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(CapturedSelection {
                    text,
                    target_hwnd,
                });
            }
        }
    }

    // Nothing was selected; do NOT touch or empty clipboard!
    None
}

/// Captures selected text from the currently active application.
#[allow(dead_code)]
pub fn capture_selection() -> Option<String> {
    capture_selection_with_window().map(|c| c.text)
}

/// Replaces text in target window: restores target window focus, normalizes CRLF,
/// pastes via Ctrl+V.
pub fn replace_selection_in_target(target_hwnd: HWND, new_text: &str) -> bool {
    // Normalize newlines to standard Windows CRLF for consistent paragraph pasting
    let normalized = new_text.replace("\r\n", "\n").replace('\n', "\r\n");

    if !set_clipboard_text(&normalized) {
        return false;
    }

    // Restore target window to foreground so Ctrl+V lands in the user's app
    unsafe {
        if !target_hwnd.0.is_null() {
            let _ = SetForegroundWindow(target_hwnd);
        }
    }

    // Wait briefly for target window to regain active foreground focus
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_millis(180) {
        if unsafe { GetForegroundWindow() } == target_hwnd {
            break;
        }
        sleep(Duration::from_millis(10));
    }

    sleep(Duration::from_millis(25));
    send_paste();
    true
}

/// Replaces the selected text in the active application with new_text.
#[allow(dead_code)]
pub fn replace_selection(new_text: &str) -> bool {
    let target_hwnd = unsafe { GetForegroundWindow() };
    replace_selection_in_target(target_hwnd, new_text)
}
