//! System tray icon and context menu.

use eframe::egui::{self, ViewportCommand};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub struct Tray {
    _icon: Option<TrayIcon>,
}

impl Tray {
    pub fn new(
        ctx: egui::Context,
        quitting: Arc<AtomicBool>,
        studio_open: Arc<AtomicBool>,
        tooltip: String,
    ) -> Self {
        let open_studio = MenuItem::new("Open Refined Studio", true, None);
        let quit = MenuItem::new("Quit Refined", true, None);
        let menu = Menu::new();
        let _ = menu.append_items(&[&open_studio, &PredefinedMenuItem::separator(), &quit]);

        let open_studio_id = open_studio.id().clone();
        let quit_id = quit.id().clone();

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

        let ctx_menu = ctx.clone();
        let quitting_clone = quitting.clone();
        let studio_open_menu = studio_open;
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if event.id == open_studio_id {
                studio_open_menu.store(true, Ordering::Relaxed);
                ctx_menu.request_repaint_of(egui::ViewportId::ROOT);
            } else if event.id == quit_id {
                quitting_clone.store(true, Ordering::Relaxed);
                ctx_menu.send_viewport_cmd(ViewportCommand::Close);
                ctx_menu.request_repaint();
            }
        }));

        let icon = TrayIconBuilder::new()
            .with_icon(Icon::from_rgba(crate::icon::rgba(32), 32, 32).expect("icon"))
            .with_tooltip(tooltip)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .ok();

        Self { _icon: icon }
    }
}

pub fn show_studio_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(ViewportCommand::CancelClose);
    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(ViewportCommand::Focus);
    ctx.request_repaint();
}
