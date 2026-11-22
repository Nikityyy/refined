//! Global shortcut on Linux through an X11 key grab on the root window.
//!
//! Wayland compositors do not allow global grabs. There, a desktop
//! shortcut that runs `refined --trigger` opens the palette instead
//! (see [`trigger`]).

use crate::store::HotkeyConfig;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, GrabMode, ModMask};
use x11rb::protocol::Event;

#[derive(Debug, Clone)]
pub struct TriggerPayload {
    pub selected_text: String,
    pub cursor_x: i32,
    pub cursor_y: i32,
    pub target_hwnd: isize,
    pub source: crate::hud::Source,
}

/// X keysym of the settings key ("X", "4", "F3").
fn keysym(key: &str) -> Option<u32> {
    if let Some(n) = key.strip_prefix('F').filter(|suffix| !suffix.is_empty()) {
        let n: u32 = n.parse().ok()?;
        return (1..=12).contains(&n).then_some(0xffbd + n); // XK_F1 == 0xffbe
    }
    let mut chars = key.chars();
    let c = chars.next()?;
    if chars.next().is_some() || !c.is_ascii_alphanumeric() {
        return None;
    }
    Some(u32::from(c.to_ascii_lowercase()))
}

fn modifiers(cfg: &HotkeyConfig) -> Option<ModMask> {
    match cfg.modifier.as_str() {
        "Ctrl+Shift" => Some(ModMask::CONTROL | ModMask::SHIFT),
        "Alt" => Some(ModMask::M1),
        "Alt+Shift" => Some(ModMask::M1 | ModMask::SHIFT),
        _ => None,
    }
}

/// Captures the highlighted or copied text and opens the palette at the pointer.
pub fn trigger(ctx: &eframe::egui::Context, sender: &Sender<TriggerPayload>) {
    let Some(cap) = crate::clipboard::capture_selection_with_window() else { return };
    let (cursor_x, cursor_y) = crate::x11::pointer().map_or_else(
        || crate::x11::screen_size().map_or((400, 300), |(w, h)| (w / 2, h / 3)),
        |(x, y, _)| (x, y),
    );
    if sender
        .send(TriggerPayload {
            selected_text: cap.text,
            cursor_x,
            cursor_y,
            target_hwnd: cap.target_window,
            source: cap.source,
        })
        .is_ok()
    {
        ctx.request_repaint_of(eframe::egui::ViewportId::ROOT);
    }
}

pub fn start_hotkey_listener(
    ctx: eframe::egui::Context,
    sender: Sender<TriggerPayload>,
    hotkey: Arc<Mutex<HotkeyConfig>>,
    hotkey_version: Arc<AtomicU64>,
    stopping: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let Ok((conn, screen)) = x11rb::connect(None) else {
            failed.store(true, Ordering::Relaxed);
            return;
        };
        let root = conn.setup().roots[screen].root;
        let mut registered: Option<(ModMask, u8)> = None;
        let mut version_seen = u64::MAX;
        // NumLock (Mod2) and CapsLock must not stop the shortcut.
        let lock_masks = [
            ModMask::from(0u16),
            ModMask::LOCK,
            ModMask::M2,
            ModMask::LOCK | ModMask::M2,
        ];
        while !stopping.load(Ordering::Relaxed) {
            let version = hotkey_version.load(Ordering::Relaxed);
            if version != version_seen {
                version_seen = version;
                if let Some((mods, code)) = registered.take() {
                    for lock in lock_masks {
                        let _ = conn.ungrab_key(code, root, mods | lock);
                    }
                }
                let cfg = hotkey.lock().unwrap().clone();
                let combo = modifiers(&cfg).zip(
                    keysym(&cfg.key).and_then(|sym| crate::x11::keycode(&conn, sym)),
                );
                let grabbed = combo.is_some_and(|(mods, code)| {
                    lock_masks.iter().all(|lock| {
                        conn.grab_key(true, root, mods | *lock, code, GrabMode::ASYNC, GrabMode::ASYNC)
                            .ok()
                            .and_then(|cookie| cookie.check().ok())
                            .is_some()
                    })
                });
                if grabbed {
                    registered = combo;
                } else if let Some((mods, code)) = combo {
                    for lock in lock_masks {
                        let _ = conn.ungrab_key(code, root, mods | lock);
                    }
                }
                failed.store(!grabbed, Ordering::Relaxed);
                let _ = conn.flush();
            }
            match conn.poll_for_event() {
                Ok(Some(Event::KeyPress(_))) => trigger(&ctx, &sender),
                Ok(Some(_)) => {}
                Ok(None) => std::thread::sleep(Duration::from_millis(30)),
                Err(_) => break,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn keys_map_to_x_keysyms() {
        assert_eq!(super::keysym("X"), Some(u32::from(b'x')));
        assert_eq!(super::keysym("4"), Some(u32::from(b'4')));
        assert_eq!(super::keysym("F3"), Some(0xffc0));
        assert_eq!(super::keysym("F13"), None);
        assert_eq!(super::keysym("XY"), None);
    }
}
