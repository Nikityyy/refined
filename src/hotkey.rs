//! Global Alt+Q Windows hotkey registration and selection trigger.

use crate::clipboard::capture_selection;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_NOREPEAT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetCursorPos, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_HOTKEY, WM_QUIT,
};

const HOTKEY_ID: i32 = 0x4151; // 'A' 'Q'

#[derive(Debug, Clone)]
pub struct TriggerPayload {
    pub selected_text: String,
    pub cursor_x: i32,
    pub cursor_y: i32,
}

pub fn start_hotkey_listener(sender: Sender<TriggerPayload>, running: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || unsafe {
        // Register Alt + Q globally
        let ok = RegisterHotKey(
            None,
            HOTKEY_ID,
            HOT_KEY_MODIFIERS(MOD_ALT.0 | MOD_NOREPEAT.0),
            0x51, // 'Q'
        );

        if ok.is_err() {
            eprintln!("Warning: Could not register Alt+Q hotkey (another app may be using it).");
        }

        let mut msg = MSG::default();
        while running.load(Ordering::Relaxed) {
            if PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    break;
                }
                if msg.message == WM_HOTKEY && msg.wParam.0 as i32 == HOTKEY_ID {
                    // Hotkey triggered: only proceed if text is actively selected!
                    if let Some(text) = capture_selection() {
                        let mut pt = POINT::default();
                        let _ = GetCursorPos(&mut pt);
                        let _ = sender.send(TriggerPayload {
                            selected_text: text,
                            cursor_x: pt.x,
                            cursor_y: pt.y,
                        });
                    }
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            } else {
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
        }

        let _ = UnregisterHotKey(None, HOTKEY_ID);
    })
}
