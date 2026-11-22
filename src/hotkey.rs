//! Global Windows hotkey registration and selection trigger.
//!
//! Ctrl+Shift+X is the default.
//!
//! The combo is user-configurable (Studio > Settings) and applied live: the
//! listener watches a version counter and re-registers on change, falling
//! back to the previous combo if the new one is already taken.

use crate::clipboard::capture_selection_with_window;
use crate::store::HotkeyConfig;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetCursorPos, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_HOTKEY,
    WM_QUIT,
};

const HOTKEY_ID: i32 = 0x4156; // Refined hotkey

#[derive(Debug, Clone)]
pub struct TriggerPayload {
    pub selected_text: String,
    pub cursor_x: i32,
    pub cursor_y: i32,
    pub target_hwnd: isize,
    pub source: crate::hud::Source,
}

/// Parses the settings key ("V", "4", "F3") into a Windows virtual key code.
fn parse_key(key: &str) -> Option<u16> {
    if let Some(n) = key.strip_prefix('F').filter(|suffix| !suffix.is_empty()) {
        let n: u16 = n.parse().ok()?;
        if !(1..=12).contains(&n) {
            return None;
        }
        Some(0x6F + n) // VK_F1 == 0x70
    } else {
        let mut chars = key.chars();
        let c = chars.next()?;
        if chars.next().is_some() {
            return None;
        }
        match c {
            '0'..='9' => Some(0x30 + c as u16 - '0' as u16),
            'a'..='z' | 'A'..='Z' => Some(0x41 + c.to_ascii_uppercase() as u16 - 'A' as u16),
            _ => None,
        }
    }
}

/// Parses the full configured combo into (raw modifier bits, virtual key).
fn parse_combo(cfg: &HotkeyConfig) -> Option<(u32, u16)> {
    let mods = match cfg.modifier.as_str() {
        "Ctrl+Shift" => MOD_CONTROL.0 | MOD_SHIFT.0,
        "Alt" => MOD_ALT.0,
        "Alt+Shift" => MOD_ALT.0 | MOD_SHIFT.0,
        _ => return None,
    };
    Some((mods, parse_key(&cfg.key)?))
}

/// (Re-)registers the configured combo. The previous registration is
/// released first; if the new combo is already owned by another app, the
/// previous one is restored so the app never ends up with no working
/// hotkey at all. `failed` is cleared on success and set on failure, which
/// the Studio Settings tab surfaces (a GUI app's eprintln is invisible).
unsafe fn apply_combo(
    hotkey: &Mutex<HotkeyConfig>,
    failed: &AtomicBool,
    registered: &mut Option<(u32, u16)>,
) {
    let cfg = hotkey.lock().unwrap().clone();
    let Some((mods, vk)) = parse_combo(&cfg) else {
        failed.store(true, Ordering::Relaxed);
        return;
    };
    if *registered == Some((mods, vk)) {
        failed.store(false, Ordering::Relaxed);
        return;
    }
    let prev = *registered;
    if prev.is_some() {
        let _ = UnregisterHotKey(None, HOTKEY_ID);
        *registered = None;
    }
    let ok = RegisterHotKey(
        None,
        HOTKEY_ID,
        HOT_KEY_MODIFIERS(mods | MOD_NOREPEAT.0),
        vk as u32,
    )
    .is_ok();
    if ok {
        *registered = Some((mods, vk));
        failed.store(false, Ordering::Relaxed);
    } else {
        failed.store(true, Ordering::Relaxed);
        if let Some((m, v)) = prev {
            let _ = UnregisterHotKey(None, HOTKEY_ID);
            if RegisterHotKey(
                None,
                HOTKEY_ID,
                HOT_KEY_MODIFIERS(m | MOD_NOREPEAT.0),
                v as u32,
            )
            .is_ok()
            {
                *registered = Some((m, v));
            }
        }
    }
}

/// `stopping` is the app's quit flag: the listener runs while it is false.
/// `hotkey`/`hotkey_version` carry live keybind settings: bumping the
/// version (from `App::logic`) triggers an immediate re-registration.
/// The egui context wakes the UI: without the explicit repaint request the
/// payload sits in the channel until something else makes the app paint,
/// and the HUD opens arbitrarily late.
pub fn start_hotkey_listener(
    ctx: eframe::egui::Context,
    sender: Sender<TriggerPayload>,
    hotkey: Arc<Mutex<HotkeyConfig>>,
    hotkey_version: Arc<AtomicU64>,
    stopping: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || unsafe {
        let mut msg = MSG::default();
        let mut registered: Option<(u32, u16)> = None;
        // u64::MAX forces registration on the first loop iteration.
        let mut version_seen: u64 = u64::MAX;
        let mut ticks: u32 = 0;

        // The loop runs until the app quits. (Passing the quit flag here as
        // a run flag once inverted this condition, and the hotkey registered
        // and immediately unregistered itself: the hotkey was dead code.)
        while !stopping.load(Ordering::Relaxed) {
            ticks = ticks.wrapping_add(1);
            let ver = hotkey_version.load(Ordering::Relaxed);
            if ver != version_seen || (registered.is_none() && ticks % 128 == 0) {
                version_seen = ver;
                apply_combo(&hotkey, &failed, &mut registered);
            }
            if PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    break;
                }
                if msg.message == WM_HOTKEY && msg.wParam.0 as i32 == HOTKEY_ID {
                    if let Some(cap) = capture_selection_with_window() {
                        let mut pt = POINT::default();
                        let _ = GetCursorPos(&mut pt);
                        let _ = sender.send(TriggerPayload {
                            selected_text: cap.text,
                            cursor_x: pt.x,
                            cursor_y: pt.y,
                            target_hwnd: cap.target_hwnd.0 as isize,
                            source: cap.source,
                        });
                        // Wake the UI thread NOW so the palette opens
                        // immediately, not whenever the next frame happens.
                        ctx.request_repaint_of(eframe::egui::ViewportId::ROOT);
                    }
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            } else {
                use windows::Win32::UI::WindowsAndMessaging::{
                    MsgWaitForMultipleObjectsEx, MWMO_INPUTAVAILABLE, QS_ALLINPUT,
                };
                let _ = MsgWaitForMultipleObjectsEx(None, 100, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
            }
        }

        if registered.is_some() {
            let _ = UnregisterHotKey(None, HOTKEY_ID);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_parses_as_ctrl_shift_x() {
        assert_eq!(
            parse_combo(&HotkeyConfig::default()),
            Some((MOD_CONTROL.0 | MOD_SHIFT.0, u16::from(b'X'))),
        );
        assert_eq!(parse_key("F"), Some(u16::from(b'F')));
        assert_eq!(parse_key("F3"), Some(0x72));
    }
}
