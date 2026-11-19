//! Refined application root: manages HUD viewport, Studio window, tray icon, and hotkey triggers.

use crate::hotkey::TriggerPayload;
use crate::hud::Hud;
use crate::store::{HotkeyConfig, Store, ThemeMode};
use crate::studio::Studio;
#[cfg(feature = "demo")]
use crate::studio::StudioTab;
use crate::theme::{self, SpectrumTheme};
use crate::tray::{self, Tray};
use eframe::egui::{self, Ui, ViewportCommand, ViewportId};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, WAIT_OBJECT_0};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CLOAK};
use windows::Win32::System::Threading::{
    OpenEventW, SetEvent, WaitForSingleObject, INFINITE, SYNCHRONIZATION_SYNCHRONIZE,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, IsWindowVisible};

/// Demo / capture options. Compiled out of shipped binaries - the fields
/// and the argument parser only exist in `--features demo` builds.
#[derive(Default)]
#[cfg_attr(not(feature = "demo"), allow(dead_code))]
pub struct DemoOptions {
    pub active: bool,
    pub size: Option<[f32; 2]>,
    pub tab: Option<String>,
    pub run: Option<String>,
}

impl DemoOptions {
    #[cfg(feature = "demo")]
    pub fn parse() -> Self {
        Self {
            active: std::env::args().any(|a| a == "--demo"),
            size: std::env::args().find_map(|a| {
                let s = a.strip_prefix("--demo-size=")?;
                let (w, h) = s.split_once('x')?;
                Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?])
            }),
            tab: std::env::args().find_map(|a| a.strip_prefix("--demo-tab=").map(str::to_string)),
            run: std::env::args().find_map(|a| a.strip_prefix("--demo-run=").map(str::to_string)),
        }
    }

    #[cfg(not(feature = "demo"))]
    pub fn none() -> Self {
        Self::default()
    }
}

pub struct RefinedApp {
    pub store: Arc<Mutex<Store>>,
    pub hud: Arc<Mutex<Hud>>,
    pub studio: Studio,
    pub studio_open: Arc<AtomicBool>,
    pub _tray: Tray,
    pub hotkey_rx: Receiver<TriggerPayload>,
    pub hotkey_failed: Arc<AtomicBool>,
    pub quitting: Arc<AtomicBool>,
    pub restore_event: HANDLE,
    hotkey_listener: Option<std::thread::JoinHandle<()>>,
    restore_listener: Option<std::thread::JoinHandle<()>>,
    root_window_hidden: bool,
    root_hwnd: HWND,
    fonts_loaded: bool,
    /// Live keybind shared with the hotkey thread; bumped via
    /// `hotkey_version` after a Settings change to trigger re-registration.
    pub hotkey: Arc<Mutex<HotkeyConfig>>,
    pub hotkey_version: Arc<AtomicU64>,
    pub hotkey_applied: HotkeyConfig,
    #[cfg(feature = "demo")]
    demo_hud_pending: bool,
    #[cfg(feature = "demo")]
    demo_run_action: Option<String>,
}

impl RefinedApp {
    pub fn new(
        cc: &eframe::CreationContext,
        store: Arc<Mutex<Store>>,
        hotkey_failed: Arc<AtomicBool>,
        restore_event: HANDLE,
        quitting: Arc<AtomicBool>,
        initially_visible: bool,
        #[cfg_attr(not(feature = "demo"), allow(unused_variables))] demo_opts: DemoOptions,
    ) -> Self {
        // Hotkey listener: started here, not in main, because it needs the
        // egui context to wake the UI for immediate HUD opens. The combo is
        // user-configurable; the listener re-registers when the version is
        // bumped from `logic` after a Settings change.
        let hotkey_cfg = { store.lock().unwrap().settings.hotkey.clone() };
        let hotkey = Arc::new(Mutex::new(hotkey_cfg.clone()));
        let hotkey_version = Arc::new(AtomicU64::new(1));
        let (hotkey_tx, hotkey_rx) = std::sync::mpsc::channel();
        let hotkey_listener = crate::hotkey::start_hotkey_listener(
            cc.egui_ctx.clone(),
            hotkey_tx,
            hotkey.clone(),
            hotkey_version.clone(),
            quitting.clone(),
            hotkey_failed.clone(),
        );

        let studio_open = Arc::new(AtomicBool::new(initially_visible));
        let tray = Tray::new(
            cc.egui_ctx.clone(),
            quitting.clone(),
            studio_open.clone(),
            format!(
                "Refined - {}+{} Text Refiner",
                hotkey_cfg.modifier, hotkey_cfg.key
            ),
        );

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
            let Ok(event) =
                (unsafe { OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, false, crate::RESTORE_EVENT) })
            else {
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
                restore_ctx.request_repaint_of(ViewportId::ROOT);
            }
            unsafe {
                let _ = CloseHandle(event);
            }
        });

        #[cfg(feature = "demo")]
        let demo_hud_pending =
            demo_opts.active && (demo_opts.tab.is_none() || demo_opts.run.is_some());
        #[cfg(feature = "demo")]
        let demo_run_action = demo_opts.run.clone();

        let root_hwnd =
            unsafe { FindWindowW(PCWSTR::null(), w!("Refined Studio")) }.unwrap_or_default();
        if !initially_visible {
            set_studio_cloaked(root_hwnd, true);
        }

        Self {
            store,
            hud: Arc::new(Mutex::new(Hud::new())),
            studio: {
                #[cfg_attr(not(feature = "demo"), allow(unused_mut))]
                let mut s = Studio::new();
                #[cfg(feature = "demo")]
                if let Some(tab) = demo_opts.tab.as_deref() {
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
            hotkey_failed,
            quitting,
            restore_event,
            hotkey_listener: Some(hotkey_listener),
            restore_listener: Some(restore_listener),
            root_window_hidden: !initially_visible,
            root_hwnd,
            fonts_loaded: false,
            hotkey,
            hotkey_version,
            hotkey_applied: hotkey_cfg,
            #[cfg(feature = "demo")]
            demo_hud_pending,
            #[cfg(feature = "demo")]
            demo_run_action,
        }
    }
}

fn set_studio_cloaked(hwnd: HWND, hidden: bool) {
    let cloak = i32::from(hidden);
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CLOAK,
            std::ptr::from_ref(&cloak).cast(),
            std::mem::size_of_val(&cloak) as u32,
        );
    }
}

impl eframe::App for RefinedApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.quitting.store(true, Ordering::Relaxed);
        // Quitting can also strand a debounced settings save.
        if self.studio.take_pending_save() {
            if let Ok(s) = self.store.lock() {
                s.save_settings();
            }
        }
        unsafe {
            let _ = SetEvent(self.restore_event);
        }
        if let Some(listener) = self.restore_listener.take() {
            let _ = listener.join();
        }
        if let Some(listener) = self.hotkey_listener.take() {
            let _ = listener.join();
        }
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting.load(Ordering::Relaxed) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.studio_open.store(false, Ordering::Relaxed);
            if self.studio.take_pending_save() {
                self.store.lock().unwrap().save_settings();
            }
        }

        let studio_hidden = !self.studio_open.load(Ordering::Relaxed);
        if studio_hidden {
            if !self.root_window_hidden || unsafe { IsWindowVisible(self.root_hwnd).as_bool() } {
                ctx.send_viewport_cmd(ViewportCommand::Visible(false));
                // eframe can show the root after the first painted frame.
                ctx.request_repaint();
            }
        } else if self.root_window_hidden {
            set_studio_cloaked(self.root_hwnd, false);
            tray::show_studio_window(ctx);
        }
        self.root_window_hidden = studio_hidden;

        if let Some(payload) = self.hotkey_rx.try_iter().last() {
            self.hud.lock().unwrap().open(
                payload.selected_text,
                payload.cursor_x,
                payload.cursor_y,
                payload.target_hwnd,
            );
            if studio_hidden {
                ctx.send_viewport_cmd(ViewportCommand::RequestPaintWhileHidden);
                ctx.request_repaint();
            }
        }

        let hud_id = crate::hud::viewport_id();
        if ctx.input(|i| i.raw.viewports.contains_key(&hud_id)) {
            let hud = self.hud.lock().unwrap();
            if hud.is_visible && !hud.presented {
                ctx.send_viewport_cmd_to(hud_id, ViewportCommand::RequestPaintWhileHidden);
                ctx.request_repaint_of(hud_id);
            }
        }

        // Push keybind changes from Settings to the hotkey listener.
        let hud_open = self.hud.lock().unwrap().is_visible;
        {
            let mut store = self.store.lock().unwrap();
            self.studio.sync_models(ctx, &mut store.settings.groq);
            self.studio.sync_updates(ctx);
            if self.studio.restart_for_update(&mut store, hud_open) {
                self.quitting.store(true, Ordering::Relaxed);
                ctx.send_viewport_cmd(ViewportCommand::Close);
                return;
            }
            let cfg = &store.settings.hotkey;
            if cfg != &self.hotkey_applied {
                *self.hotkey.lock().unwrap() = cfg.clone();
                self.hotkey_version.fetch_add(1, Ordering::Relaxed);
                self.hotkey_applied = cfg.clone();
            }
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Demo mode (`refined --demo`): open the HUD once with sample text so
        // the UI can be inspected without a text selection in another app.
        // `--demo-tab=<name>` opens the Studio directly on a given tab (no HUD,
        // so the dashboard is fully visible for captures).
        // `--demo-run=<action_id>` triggers that action right away (with a real
        // API key in settings this produces a genuine Processing -> Diff flow).
        #[cfg(feature = "demo")]
        {
            if self.demo_hud_pending {
                self.demo_hud_pending = false;
                let sample = "The replacment of human editors with AI is unlikely to happen soon, \
because AI models are unable to distinguishes between facts and convincing-sounding \
nonsense, which is a very serious problem for publishing.\n";
                let mut hud = self.hud.lock().unwrap();
                hud.open(sample.to_string(), 260, 180, 0);
                hud.demo_sticky = true;
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
                    self.hud
                        .lock()
                        .unwrap()
                        .trigger_action(&ctx, &action, None, &config, &auto);
                }
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
            ThemeMode::System => ctx.system_theme() != Some(egui::Theme::Light),
        };
        let spectrum_theme = SpectrumTheme::for_mode(is_dark);
        theme::configure_egui_theme(&ctx, &spectrum_theme);

        let hud_layout = {
            let hud = self.hud.lock().unwrap();
            hud.is_visible.then(|| {
                let store = self.store.lock().unwrap();
                (hud.position, hud.desired_height(&store.settings))
            })
        };
        if !self.fonts_loaded && (hud_layout.is_some() || self.studio_open.load(Ordering::Relaxed))
        {
            theme::setup_custom_fonts(&ctx);
            self.fonts_loaded = true;
        }
        let hud_open = hud_layout.is_some();
        if let Some((position, height)) = hud_layout {
            let hud = self.hud.clone();
            let store = self.store.clone();
            let studio_open = self.studio_open.clone();
            ctx.show_viewport_deferred(
                crate::hud::viewport_id(),
                egui::ViewportBuilder::default()
                    .with_title("Refined HUD")
                    .with_decorations(false)
                    .with_always_on_top()
                    .with_taskbar(false)
                    .with_resizable(false)
                    .with_inner_size([crate::hud::WIDTH, height])
                    .with_position(position)
                    .with_active(false)
                    .with_visible(false),
                move |ui, _| {
                    let ctx = ui.ctx().clone();
                    let mut hud = hud.lock().unwrap();
                    if !hud.is_visible || (hud.presented && hud.dismiss_requested(ui)) {
                        hud.close_window(&ctx);
                        return;
                    }

                    let mut store = store.lock().unwrap();
                    hud.poll_worker(&ctx, &mut |entry| store.add_history(entry));
                    let settings = &mut store.settings;
                    let previous_language = (
                        settings.selected_language.clone(),
                        settings.custom_language.clone(),
                    );
                    let mut open_studio = false;
                    theme::configure_egui_theme(&ctx, &spectrum_theme);
                    ui.painter()
                        .rect_filled(ui.max_rect(), 0, spectrum_theme.surface);
                    egui::Frame::new()
                        .inner_margin(egui::Margin::same(12))
                        .show(ui, |ui| {
                            hud.render_ui(ui, settings, &spectrum_theme, &mut || {
                                open_studio = true
                            });
                        });

                    if previous_language.0 != settings.selected_language
                        || previous_language.1 != settings.custom_language
                    {
                        store.save_settings();
                    }
                    if !hud.is_visible {
                        hud.close_window(&ctx);
                    } else {
                        if !hud.presented {
                            hud.present(&ctx);
                        }
                        let next_height = hud.desired_height(&store.settings);
                        if (next_height - height).abs() > 0.5 {
                            ctx.request_repaint_of(ViewportId::ROOT);
                        }
                    }
                    if open_studio {
                        studio_open.store(true, Ordering::Relaxed);
                        ctx.request_repaint_of(ViewportId::ROOT);
                    }
                },
            );
            if !self.hud.lock().unwrap().presented {
                ctx.request_repaint();
            }
        }

        let is_studio_open = self.studio_open.load(Ordering::Relaxed);
        if is_studio_open {
            egui::Frame::new()
                .fill(spectrum_theme.background)
                .inner_margin(egui::Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    let mut store_guard = self.store.lock().unwrap();
                    self.studio.hotkey_failed = self
                        .hotkey_failed
                        .load(std::sync::atomic::Ordering::Relaxed);
                    self.studio
                        .update(ui, &mut store_guard, &spectrum_theme, hud_open);
                });
        } else {
            // Hiding the Studio can strand a debounced settings save that
            // only Studio::update would have flushed.
            if self.studio.take_pending_save() {
                if let Ok(s) = self.store.lock() {
                    s.save_settings();
                }
            }
        }
    }
}
