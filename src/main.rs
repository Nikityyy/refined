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

    // Headless model list: `refined --list-models` prints all current chat
    // model ids (one per line) using the REFINED_DEMO_KEY environment key.
    if std::env::args().any(|a| a == "--list-models") {
        let key = std::env::var("REFINED_DEMO_KEY").unwrap_or_default();
        if key.trim().is_empty() {
            eprintln!("REFINED_DEMO_KEY is not set");
            std::process::exit(2);
        }
        match groq::fetch_chat_models(&key) {
            Ok(models) => {
                for m in models {
                    println!("{}", m.id);
                }
            }
            Err(err) => {
                eprintln!("error: {err}");
                std::process::exit(2);
            }
        }
        return Ok(());
    }

    // Headless benchmark mode: `refined --bench=<model_id>` runs one Polish
    // completion on a fixed sample with the REFINED_DEMO_KEY environment key and
    // prints a JSON result line. Used to measure latency/quality per model.
    if let Some(model) = std::env::args().find_map(|a| a.strip_prefix("--bench=").map(str::to_string)) {
        let mut config = groq::GroqConfig::default();
        if let Ok(k) = std::env::var("REFINED_DEMO_KEY") {
            config.api_key = k;
        }
        config.model = model;
        let polish = crate::actions::default_actions()
            .into_iter()
            .find(|a| a.id == "polish")
            .map(|a| a.prompt_instruction)
            .unwrap_or_default();
        let sample = "The replacment of human editors with AI is unlikely to happen soon, \
because AI models are unable to distinguishes between facts and convincing-sounding nonsense.";
        match crate::groq::complete_action(&config, &polish, sample, "auto") {
            Ok(resp) => {
                let out = serde_json::json!({
                    "model": config.model,
                    "ms": resp.duration_ms,
                    "chars": resp.text.chars().count(),
                    "text": resp.text,
                });
                println!("{out}");
            }
            Err(err) => {
                eprintln!("{{\"model\":\"{}\",\"error\":\"{err}\"}}", config.model);
                std::process::exit(2);
            }
        }
        return Ok(());
    }

    // Load store
    let store = Arc::new(Mutex::new(store::Store::open()));

    // Hotkey listener thread
    let (hotkey_tx, hotkey_rx) = channel();
    let quitting = Arc::new(AtomicBool::new(false));
    let _hotkey_handle = hotkey::start_hotkey_listener(hotkey_tx, quitting.clone());

    // Window options
    let hidden = std::env::args().any(|a| a == "--hidden");
    let demo = std::env::args().any(|a| a == "--demo");
    let demo_tab = std::env::args()
        .find_map(|a| a.strip_prefix("--demo-tab=").map(str::to_string));
    // --demo-size=WIDTHxHEIGHT : bigger window so full pages (e.g. charts) are visible
    let demo_size = std::env::args().find_map(|a| {
        let s = a.strip_prefix("--demo-size=")?;
        let (w, h) = s.split_once('x')?;
        Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?])
    });
    // --demo-run=<action_id> : auto-trigger the given action on the demo text
    // (used with a real API key to capture the Processing and Diff screens end-to-end)
    let demo_run = std::env::args()
        .find_map(|a| a.strip_prefix("--demo-run=").map(str::to_string));
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Refined Studio")
            .with_inner_size(if demo { demo_size.unwrap_or([960.0, 680.0]) } else { [960.0, 680.0] })
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
                !hidden || demo,
                demo,
                demo_tab,
                demo_run,
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
