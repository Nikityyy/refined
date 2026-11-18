//! Refined application root: manages HUD viewport, Studio window, tray icon, and hotkey triggers.

use crate::hotkey::TriggerPayload;
use crate::hud::Hud;
use crate::store::{Store, ThemeMode};
use crate::studio::{Studio, StudioTab};
use crate::theme::{self, SpectrumTheme};
use crate::tray::{self, Tray};
use eframe::egui::{self, Color32, CornerRadius, Stroke, Ui, ViewportCommand};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{OpenEventW, SetEvent, WaitForSingleObject, INFINITE, SYNCHRONIZATION_SYNCHRONIZE};

pub struct RefinedApp {
    pub store: Arc<Mutex<Store>>,
    pub hud: Hud,
    pub studio: Studio,
    pub studio_open: Arc<AtomicBool>,
    pub _tray: Tray,
    pub hotkey_rx: Receiver<TriggerPayload>,
    pub quitting: Arc<AtomicBool>,
    pub restore_event: HANDLE,
    pub _restore_listener: Option<std::thread::JoinHandle<()>>,
    pub demo_hud_pending: bool,
    pub demo_run_action: Option<String>,
}

impl RefinedApp {
    pub fn new(
        cc: &eframe::CreationContext,
        store: Arc<Mutex<Store>>,
        hotkey_rx: Receiver<TriggerPayload>,
        restore_event: HANDLE,
        quitting: Arc<AtomicBool>,
        initially_visible: bool,
        demo_hud: bool,
        demo_tab: Option<String>,
        demo_run: Option<String>,
    ) -> Self {
        // Setup Segoe UI system fonts
        theme::setup_custom_fonts(&cc.egui_ctx);

        let studio_open = Arc::new(AtomicBool::new(initially_visible));
        let tray = Tray::new(cc.egui_ctx.clone(), quitting.clone(), studio_open.clone());

        // Persist settings once at startup so the on-disk file always
        // reflects the current schema after an app update.
        {
            let store = store.lock().unwrap();
            store.save_settings();
        }

        let restore_ctx = cc.egui_ctx.clone();
        let restore_quitting = quitting.clone();
        let restore_studio_open = studio_open.clone();
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
                restore_studio_open.store(true, Ordering::Relaxed);
                tray::show_studio_window(&restore_ctx);
            }
            unsafe {
                let _ = CloseHandle(event);
            }
        });

        Self {
            store,
            hud: Hud::new(),
            studio: {
                let mut s = Studio::new();
                if let Some(tab) = demo_tab.as_deref() {
                    s.current_tab = match tab {
                        "statistics" => StudioTab::Statistics,
                        "actions" => StudioTab::Actions,
                        "settings" => StudioTab::Settings,
                        _ => StudioTab::History,
                    };
                }
                s
            },
            studio_open,
            _tray: tray,
            hotkey_rx,
            quitting,
            restore_event,
            _restore_listener: Some(restore_listener),
            // The HUD demo only opens when there is no explicit tab, or when a
            // live action run asks for it (--demo-run), so --demo-tab captures
            // show an unoccluded dashboard.
            demo_hud_pending: demo_hud && (demo_tab.is_none() || demo_run.is_some()),
            demo_run_action: demo_run,
        }
    }
}

impl eframe::App for RefinedApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.quitting.store(true, Ordering::Relaxed);
        unsafe {
            let _ = SetEvent(self.restore_event);
        }
        if let Some(listener) = self._restore_listener.take() {
            let _ = listener.join();
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Intercept Studio window close request to minimize to tray
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting.load(Ordering::Relaxed) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
            self.studio_open.store(false, Ordering::Relaxed);
        }

        // Process Alt + Q triggers from hotkey thread:
        // Opens the floating HUD viewport directly WITHOUT unhiding the 960x680 Studio window!
        while let Ok(payload) = self.hotkey_rx.try_recv() {
            self.hud.open(
                payload.selected_text,
                payload.cursor_x,
                payload.cursor_y,
                payload.target_hwnd,
            );
            ctx.request_repaint();
        }

        // Demo mode (`refined --demo`): open the HUD once with sample text so
        // the UI can be inspected without a text selection in another app.
        // `--demo-tab=<name>` opens the Studio directly on a given tab (no HUD,
        // so the dashboard is fully visible for captures).
        // `--demo-run=<action_id>` triggers that action right away (with a real
        // API key in settings this produces a genuine Processing -> Diff flow).
        if self.demo_hud_pending {
            self.demo_hud_pending = false;
            let sample = "The replacment of human editors with AI is unlikely to happen soon, \
because AI models are unable to distinguishes between facts and convincing-sounding \
nonsense, which is a very serious problem for publishing.\n";
            self.hud.open(sample.to_string(), 260, 180, 0);
            self.hud.demo_sticky = true;
            self.studio_open.store(true, Ordering::Relaxed);
            ctx.request_repaint();
        }
        if let Some(action) = self.demo_run_action.take() {
            // Demo-only: allow the API key to be supplied via the environment
            // (REFINED_DEMO_KEY) so a live run can be driven without ever
            // persisting a key to disk.
            let (found, mut config) = {
                let store_guard = self.store.lock().unwrap();
                let action = store_guard
                    .settings
                    .actions
                    .iter()
                    .find(|a| a.id == action)
                    .cloned();
                let config = store_guard.settings.groq.clone();
                (action, config)
            };
            if let Ok(k) = std::env::var("REFINED_DEMO_KEY") {
                k.clone_into(&mut config.api_key);
            }
            if let Some(action) = found {
                let auto = "auto".to_string();
                self.hud.trigger_action(&action, None, &config, &auto);
            }
        }

        // Determine active theme
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

        // Poll any background worker messages for LLM completion
        {
            let mut store_guard = self.store.lock().unwrap();
            self.hud.poll_worker(&ctx, &mut |entry| {
                store_guard.add_history(entry);
            });
        }

        // 1. Contextual Floating Alt+Q HUD Viewport
        if self.hud.is_visible {
            let hud_viewport_id = egui::ViewportId::from_hash_of("refined_hud");
            let hud_pos = self.hud.position;
            let act_count = {
                let s = self.store.lock().unwrap();
                s.settings.actions.len()
            };
            let hud_h = self.hud.desired_height(act_count);

            ctx.show_viewport_immediate(
                hud_viewport_id,
                egui::ViewportBuilder::default()
                    .with_title("Refined HUD")
                    .with_decorations(false)
                    .with_transparent(true)
                    .with_always_on_top()
                    .with_resizable(false)
                    .with_inner_size([480.0, hud_h])
                    .with_position([hud_pos.x, hud_pos.y])
                    .with_window_level(egui::WindowLevel::AlwaysOnTop),
                |hud_ctx, _class| {
                // Auto-dismiss HUD on focus loss (after the initial open frame).
                // In demo mode the HUD stays open for inspection.
                let sticky = self.hud.demo_sticky;
                if !self.hud.just_opened && !sticky && hud_ctx.input(|i| i.viewport().focused == Some(false)) {
                    self.hud.close();
                    return;
                }
                    self.hud.just_opened = false;

                    theme::configure_egui_theme(hud_ctx, &spectrum_theme);

                    egui::CentralPanel::default()
                        .frame(
                            egui::Frame::new()
                                .fill(spectrum_theme.surface)
                                .stroke(Stroke::new(1.0, spectrum_theme.border))
                                .corner_radius(CornerRadius::same(12))
                                .shadow(egui::epaint::Shadow {
                                    offset: [0, 8],
                                    blur: 24,
                                    spread: 0,
                                    color: Color32::from_black_alpha(if spectrum_theme.is_dark { 90 } else { 35 }),
                                })
                                .inner_margin(egui::Margin::same(12)),
                        )
                        .show(hud_ctx, |ui| {
                            let mut store_guard = self.store.lock().unwrap();
                            let actions = store_guard.settings.actions.clone();
                            let groq_config = store_guard.settings.groq.clone();
                            let mut sel_lang = store_guard.settings.selected_language.clone();
                            let mut cust_lang = store_guard.settings.custom_language.clone();
                            let mut open_studio = false;

                            self.hud.render_ui(
                                ui,
                                &actions,
                                &groq_config,
                                &mut sel_lang,
                                &mut cust_lang,
                                &spectrum_theme,
                                &mut || {
                                    open_studio = true;
                                },
                            );

                            if sel_lang != store_guard.settings.selected_language
                                || cust_lang != store_guard.settings.custom_language
                            {
                                store_guard.settings.selected_language = sel_lang;
                                store_guard.settings.custom_language = cust_lang;
                                store_guard.save_settings();
                            }

                            if open_studio {
                                self.studio_open.store(true, Ordering::Relaxed);
                                tray::show_studio_window(&ctx);
                            }
                        });
                },
            );
        }

        // 2. Main Studio Dashboard
        let is_studio_open = self.studio_open.load(Ordering::Relaxed);
        if is_studio_open {
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            egui::Frame::new()
                .fill(spectrum_theme.background)
                .inner_margin(egui::Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    let mut store_guard = self.store.lock().unwrap();
                    self.studio.update(ui, &mut store_guard, &spectrum_theme);
                });
        } else {
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }

        if let crate::hud::HudState::Processing { .. } = self.hud.state {
            ctx.request_repaint_after(std::time::Duration::from_millis(30));
        }
    }
}
