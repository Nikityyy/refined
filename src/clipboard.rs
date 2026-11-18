//! Windows clipboard and text selection management.
//!
//! Provides text capture via simulated Ctrl+C with sequence checking,
//! and text replacement via simulated Ctrl+V.

use std::ptr;
use std::thread::sleep;
use std::time::Duration;
use windows::Win32::Foundation::{HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
    OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_C,
    VK_CONTROL, VK_V,
};

const CF_UNICODETEXT: u32 = 13;

/// Reads Unicode text currently in the Windows clipboard.
pub fn get_clipboard_text() -> Option<String> {
    unsafe {
        if OpenClipboard(None).is_err() {
            return None;
        }
        let result = (|| {
            if IsClipboardFormatAvailable(CF_UNICODETEXT).is_err() {
                return None;
            }
            let handle = GetClipboardData(CF_UNICODETEXT).ok()?;
            let ptr = GlobalLock(HGLOBAL(handle.0)) as *const u16;
            if ptr.is_null() {
                return None;
            }
            let mut len = 0;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            let slice = std::slice::from_raw_parts(ptr, len);
            let text = String::from_utf16_lossy(slice);
            let _ = GlobalUnlock(HGLOBAL(handle.0));
            Some(text)
        })();
        let _ = CloseClipboard();
        result
    }
}

/// Sets Unicode text into the Windows clipboard.
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
            return false;
        }
        ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, ptr as *mut u8, bytes);
        let _ = GlobalUnlock(mem);

        // Retry OpenClipboard if another app is currently inspecting it
        let mut opened = false;
        for _ in 0..5 {
            if OpenClipboard(None).is_ok() {
                opened = true;
                break;
            }
            sleep(Duration::from_millis(10));
        }
        if !opened {
            return false;
        }

        let _ = EmptyClipboard();
        let ok = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(mem.0))).is_ok();
        let _ = CloseClipboard();
        ok
    }
}

/// Simulates pressing a key down or releasing it.
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

/// Sends Ctrl+C to copy whatever is currently selected in the active app.
pub fn send_copy() {
    let inputs = [
        key_event(VK_CONTROL, false),
        key_event(VK_C, false),
        key_event(VK_C, true),
        key_event(VK_CONTROL, true),
    ];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

/// Sends Ctrl+V to paste the clipboard text into the active app.
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

/// Captures selected text from the currently active application.
/// Returns Some(selected_text) only if text was actively selected.
/// If no text was selected, returns None and preserves the existing clipboard.
pub fn capture_selection() -> Option<String> {
    let old_seq = unsafe { GetClipboardSequenceNumber() };
    let previous_clipboard = get_clipboard_text();

    // Trigger Ctrl+C
    send_copy();

    // Poll for clipboard update for up to 90ms
    let start = std::time::Instant::now();
    let mut changed = false;
    while start.elapsed() < Duration::from_millis(90) {
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
                return Some(text);
            }
        }
    }

    // Nothing was selected or copy failed. Restore previous clipboard if needed.
    if let Some(prev) = previous_clipboard {
        set_clipboard_text(&prev);
    }
    None
}

/// Replaces the selected text in the active application with new_text.
pub fn replace_selection(new_text: &str) -> bool {
    if !set_clipboard_text(new_text) {
        return false;
    }
    // Small delay so target app receives clipboard format
    sleep(Duration::from_millis(30));
    send_paste();
    true
}
