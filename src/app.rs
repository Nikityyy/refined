//! Refined application root: manages HUD, Studio window, tray icon, and hotkey messages.

use crate::hotkey::TriggerPayload;
use crate::hud::Hud;
use crate::store::{Store, ThemeMode};
use crate::studio::Studio;
use crate::theme::{self, SpectrumTheme};
use crate::tray::{self, Tray};
use eframe::egui::{self, Ui, ViewportCommand};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{OpenEventW, WaitForSingleObject, INFINITE, SYNCHRONIZATION_SYNCHRONIZE};

pub struct RefinedApp {
    pub store: Arc<Mutex<Store>>,
    pub hud: Hud,
    pub studio: Studio,
    pub _tray: Tray,
    pub hotkey_rx: Receiver<TriggerPayload>,
    pub quitting: Arc<AtomicBool>,
    #[allow(dead_code)]
pub restore_event: HANDLE,
    pub _restore_listener: Option<std::thread::JoinHandle<()>>,
}

impl RefinedApp {
    pub fn new(
        cc: &eframe::CreationContext,
        store: Arc<Mutex<Store>>,
        hotkey_rx: Receiver<TriggerPayload>,
        restore_event: HANDLE,
        quitting: Arc<AtomicBool>,
    ) -> Self {
        let tray = Tray::new(cc.egui_ctx.clone(), quitting.clone());

        let restore_ctx = cc.egui_ctx.clone();
        let restore_quitting = quitting.clone();
        let restore_listener = std::thread::spawn(move || {
            let Ok(event) = (unsafe { OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, false, crate::RESTORE_EVENT) }) else {
                return;
            };
            loop {
                if unsafe { WaitForSingleObject(event, INFINITE) } != WAIT_OBJECT_0 {
                    break;
                }
                if restore_quitting.load(Ordering::Relaxed) {
                    break;
                }
                tray::show_studio_window(&restore_ctx);
            }
            unsafe {
                let _ = CloseHandle(event);
            }
        });

        Self {
            store,
            hud: Hud::new(),
            studio: Studio::new(),
            _tray: tray,
            hotkey_rx,
            quitting,
            restore_event,
            _restore_listener: Some(restore_listener),
        }
    }
}

impl eframe::App for RefinedApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Intercept close request to minimize to tray instead of quitting
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting.load(Ordering::Relaxed) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }

        // Process Alt + Q triggers from hotkey thread
        while let Ok(payload) = self.hotkey_rx.try_recv() {
            self.hud.open(payload.selected_text, payload.cursor_x, payload.cursor_y);
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::Focus);
            ctx.request_repaint();
        }

        // Determine theme
        let theme_mode = {
            let s = self.store.lock().unwrap();
            s.settings.theme_mode
        };
        let is_dark = match theme_mode {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => ui.visuals().dark_mode,
        };
        let spectrum_theme = SpectrumTheme::for_mode(is_dark);
        theme::configure_egui_theme(&ctx, &spectrum_theme);

        // Studio Full Window view
        egui::Frame::new()
            .fill(spectrum_theme.background)
            .inner_margin(egui::Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                let mut store_guard = self.store.lock().unwrap();
                self.studio.update(ui, &mut store_guard, &spectrum_theme);
            });

        // Floating Alt+Q HUD
        {
            let mut store_guard = self.store.lock().unwrap();
            let actions = store_guard.settings.actions.clone();
            let groq_config = store_guard.settings.groq.clone();
            let mut sel_lang = store_guard.settings.selected_language.clone();
            let mut cust_lang = store_guard.settings.custom_language.clone();

            let mut new_history: Option<crate::history::HistoryEntry> = None;
            let mut open_studio = false;

            self.hud.update(
                &ctx,
                &actions,
                &groq_config,
                &mut sel_lang,
                &mut cust_lang,
                &spectrum_theme,
                &mut |entry| {
                    new_history = Some(entry);
                },
                &mut || {
                    open_studio = true;
                },
            );

            if let Some(entry) = new_history {
                store_guard.add_history(entry);
            }
            if sel_lang != store_guard.settings.selected_language || cust_lang != store_guard.settings.custom_language {
                store_guard.settings.selected_language = sel_lang;
                store_guard.settings.custom_language = cust_lang;
                store_guard.save_settings();
            }
            if open_studio {
                tray::show_studio_window(&ctx);
            }
        }

        if let crate::hud::HudState::Processing { .. } = self.hud.state {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
    }
}

