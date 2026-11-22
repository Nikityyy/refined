//! X11 access for Linux: pointer and key state, the focused window, and
//! synthetic key presses through the XTEST extension. On a Wayland session
//! this reaches XWayland windows only.

use std::sync::OnceLock;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, InputFocus, Keycode, Window,
    CLIENT_MESSAGE_EVENT,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

pub const XK_ESCAPE: u32 = 0xff1b;
pub const XK_CONTROL_L: u32 = 0xffe3;
const XK_SHIFT_L: u32 = 0xffe1;
const XK_SHIFT_R: u32 = 0xffe2;
const XK_ALT_L: u32 = 0xffe9;
const XK_ALT_R: u32 = 0xffea;
const XK_SUPER_L: u32 = 0xffeb;

pub struct Display {
    pub conn: RustConnection,
    pub root: Window,
}

/// The shared connection, or None without an X server.
pub fn display() -> Option<&'static Display> {
    static DISPLAY: OnceLock<Option<Display>> = OnceLock::new();
    DISPLAY
        .get_or_init(|| {
            let (conn, screen) = x11rb::connect(None).ok()?;
            let root = conn.setup().roots.get(screen)?.root;
            Some(Display { conn, root })
        })
        .as_ref()
}

/// Returns the keycode that produces `keysym` without modifiers.
pub fn keycode(conn: &RustConnection, keysym: u32) -> Option<Keycode> {
    let setup = conn.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let mapping = conn
        .get_keyboard_mapping(min, max - min + 1)
        .ok()?
        .reply()
        .ok()?;
    let per = usize::from(mapping.keysyms_per_keycode).max(1);
    mapping
        .keysyms
        .chunks(per)
        .position(|syms| syms.first() == Some(&keysym) || syms.get(1) == Some(&keysym))
        .map(|index| min + index as u8)
}

/// Pointer position on the root window and whether a mouse button is held.
pub fn pointer() -> Option<(i32, i32, bool)> {
    let display = display()?;
    let reply = display.conn.query_pointer(display.root).ok()?.reply().ok()?;
    let buttons = u16::from(reply.mask) & (0x100 | 0x200 | 0x400) != 0;
    Some((i32::from(reply.root_x), i32::from(reply.root_y), buttons))
}

/// Whether the key producing `keysym` is held down anywhere.
pub fn key_down(keysym: u32) -> bool {
    let Some(display) = display() else { return false };
    let Some(code) = keycode(&display.conn, keysym) else { return false };
    display
        .conn
        .query_keymap()
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .is_some_and(|keymap| keymap.keys[usize::from(code / 8)] & (1 << (code % 8)) != 0)
}

/// Size of the root window, which spans all monitors.
pub fn screen_size() -> Option<(i32, i32)> {
    let display = display()?;
    let geometry = display.conn.get_geometry(display.root).ok()?.reply().ok()?;
    Some((i32::from(geometry.width), i32::from(geometry.height)))
}

fn atom(conn: &RustConnection, name: &str) -> Option<u32> {
    Some(conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok()?.atom)
}

/// The window that has keyboard focus, as the window manager reports it.
pub fn active_window() -> Option<Window> {
    let display = display()?;
    let conn = &display.conn;
    let active = atom(conn, "_NET_ACTIVE_WINDOW")?;
    let reply = conn
        .get_property(false, display.root, active, AtomEnum::WINDOW, 0, 1)
        .ok()?
        .reply()
        .ok()?;
    reply
        .value32()
        .and_then(|mut values| values.next())
        .filter(|window| *window != 0)
        .or_else(|| {
            let focus = conn.get_input_focus().ok()?.reply().ok()?.focus;
            (focus > 1).then_some(focus)
        })
}

/// Asks the window manager to focus `window` and waits up to 300 ms.
pub fn activate(window: Window) -> bool {
    let Some(display) = display() else { return false };
    if active_window() == Some(window) {
        return true;
    }
    let conn = &display.conn;
    if let Some(active) = atom(conn, "_NET_ACTIVE_WINDOW") {
        // Source 2 marks a pager-style request, which window managers honor.
        let event = ClientMessageEvent {
            response_type: CLIENT_MESSAGE_EVENT,
            format: 32,
            sequence: 0,
            window,
            type_: active,
            data: [2u32, 0, 0, 0, 0].into(),
        };
        let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
        let _ = conn.send_event(false, display.root, mask, event);
    } else {
        let _ = conn.set_input_focus(InputFocus::PARENT, window, x11rb::CURRENT_TIME);
    }
    let _ = conn.flush();
    let deadline = Instant::now() + Duration::from_millis(300);
    while Instant::now() < deadline {
        if active_window() == Some(window) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

/// Releases held Shift, Alt, and Super, then types Ctrl+`keysym`.
pub fn send_ctrl(keysym: u32) -> bool {
    let Some(display) = display() else { return false };
    let conn = &display.conn;
    let Some(control) = keycode(conn, XK_CONTROL_L) else { return false };
    let Some(key) = keycode(conn, keysym) else { return false };
    let mut ok = true;
    let mut fake = |press: bool, code: Keycode| {
        let kind = if press { 2 } else { 3 }; // KeyPress, KeyRelease
        ok &= conn
            .xtest_fake_input(kind, code, x11rb::CURRENT_TIME, display.root, 0, 0, 0)
            .is_ok();
    };
    for modifier in [XK_SHIFT_L, XK_SHIFT_R, XK_ALT_L, XK_ALT_R, XK_SUPER_L] {
        if key_down(modifier) {
            if let Some(code) = keycode(conn, modifier) {
                fake(false, code);
            }
        }
    }
    fake(true, control);
    fake(true, key);
    fake(false, key);
    fake(false, control);
    ok && conn.flush().is_ok()
}
