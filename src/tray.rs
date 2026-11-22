//! System tray icon and context menu.

use eframe::egui::{self, ViewportCommand};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

const OPEN_STUDIO: &str = "open_studio";
const QUIT: &str = "quit";

pub struct Tray {
    #[cfg(windows)]
    _icon: Option<TrayIcon>,
}

fn build(tooltip: String) -> Option<TrayIcon> {
    let menu = Menu::new();
    let open_studio = MenuItem::with_id(OPEN_STUDIO, "Open Refined Studio", true, None);
    let quit = MenuItem::with_id(QUIT, "Quit Refined", true, None);
    menu.append_items(&[&open_studio, &PredefinedMenuItem::separator(), &quit])
        .ok()?;
    TrayIconBuilder::new()
        .with_icon(Icon::from_rgba(crate::icon::rgba(32), 32, 32).ok()?)
        .with_tooltip(tooltip)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .ok()
}

impl Tray {
    pub fn new(
        ctx: egui::Context,
        quitting: Arc<AtomicBool>,
        studio_open: Arc<AtomicBool>,
        tooltip: String,
    ) -> Self {
        let ctx_click = ctx.clone();
        let studio_open_click = studio_open.clone();
        TrayIconEvent::set_event_handler(Some(move |event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                studio_open_click.store(true, Ordering::Relaxed);
                ctx_click.request_repaint_of(egui::ViewportId::ROOT);
            }
        }));

        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if event.id == OPEN_STUDIO {
                studio_open.store(true, Ordering::Relaxed);
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            } else if event.id == QUIT {
                quitting.store(true, Ordering::Relaxed);
                ctx.send_viewport_cmd(ViewportCommand::Close);
                ctx.request_repaint();
            }
        }));

        #[cfg(windows)]
        return Self { _icon: build(tooltip) };

        // GTK owns the tray on Linux and needs its own loop. Without an
        // AppIndicator library the app runs without a tray icon; starting
        // Refined again opens Studio.
        #[cfg(target_os = "linux")]
        {
            std::thread::spawn(move || {
                let available = ["libayatana-appindicator3.so.1", "libappindicator3.so.1"]
                    .iter()
                    .any(|name| unsafe { libloading::Library::new(name) }.is_ok());
                if available && gtk::init().is_ok() {
                    let _icon = build(tooltip);
                    gtk::main();
                }
            });
            Self {}
        }
    }
}

pub fn show_studio_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(ViewportCommand::CancelClose);
    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(ViewportCommand::Focus);
    ctx.request_repaint();
}
