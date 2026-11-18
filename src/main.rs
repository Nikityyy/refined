#![windows_subsystem = "windows"]

mod actions;
mod app;
mod autostart;
mod clipboard;
mod diff;
mod groq;
mod history;
mod hotkey;
mod hud;
mod icon;
mod stats;
mod store;
mod studio;
mod theme;
mod tray;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::{CreateEventW, CreateMutexW, SetEvent};

pub const INSTANCE_MUTEX: PCWSTR = w!("Local\\Refined");
pub const RESTORE_EVENT: PCWSTR = w!("Local\\Refined.Restore");

fn main() -> eframe::Result {
    // Single instance mutex and restore event
    let restore_event = unsafe { CreateEventW(None, false, false, RESTORE_EVENT) }.expect("restore event");
    let instance = unsafe { CreateMutexW(None, true, INSTANCE_MUTEX) }.expect("instance mutex");

    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            let _ = SetEvent(restore_event);
            let _ = CloseHandle(restore_event);
            let _ = CloseHandle(instance);
        }
        return Ok(());
    }

    // Refresh Windows autostart entry if enabled
    autostart::refresh();

    // Load store
    let store = Arc::new(Mutex::new(store::Store::open()));

    // Hotkey listener thread
    let (hotkey_tx, hotkey_rx) = channel();
    let quitting = Arc::new(AtomicBool::new(false));
    let _hotkey_handle = hotkey::start_hotkey_listener(hotkey_tx, quitting.clone());

    // Window options
    let hidden = std::env::args().any(|a| a == "--hidden");
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Refined - Adobe Spectrum 2 Text Refiner")
            .with_inner_size([960.0, 680.0])
            .with_min_inner_size([740.0, 520.0])
            .with_icon(Arc::new(eframe::egui::IconData {
                rgba: icon::rgba(64),
                width: 64,
                height: 64,
            }))
            .with_visible(!hidden),
        ..Default::default()
    };

    let store_for_app = store.clone();
    let quitting_for_app = quitting.clone();
    let result = eframe::run_native(
        "Refined",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::RefinedApp::new(
                cc,
                store_for_app,
                hotkey_rx,
                restore_event,
                quitting_for_app,
                !hidden,
            )))
        }),
    );

    quitting.store(true, Ordering::Relaxed);
    unsafe {
        let _ = CloseHandle(restore_event);
        let _ = CloseHandle(instance);
    }
    result
}
