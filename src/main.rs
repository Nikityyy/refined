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
mod updater;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::{CreateEventW, CreateMutexW, SetEvent};

pub const INSTANCE_MUTEX: PCWSTR = w!("Local\\Refined");
pub const RESTORE_EVENT: PCWSTR = w!("Local\\Refined.Restore");

// Demo / capture / benchmark scaffolding, kept out of shipped binaries via
// the `demo` cargo feature.
#[cfg(feature = "demo")]
#[derive(serde::Deserialize)]
struct HistorySeed {
    action_id: String,
    action_name: String,
    original_text: String,
    refined_text: String,
    age_hours: i64,
    duration_ms: u128,
}

/// Writes demo history entries computed through the real diff engine.
#[cfg(feature = "demo")]
fn seed_history(path: &str) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("could not read seed file '{path}': {e}");
            std::process::exit(2);
        }
    };
    let seeds: Vec<HistorySeed> = match serde_json::from_str(&content) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("could not parse seed file '{path}': {e}");
            std::process::exit(2);
        }
    };

    let mut store = store::Store::open();
    // add_history inserts at index 0 (newest first), so inserting
    // oldest-first keeps the shipping evidence in true newest-first order.
    let mut seeds = seeds;
    seeds.sort_by(|a, b| b.age_hours.cmp(&a.age_hours));
    for seed in seeds {
        let (chunks, stats) = diff::compute_diff(&seed.original_text, &seed.refined_text);
        let mut entry = history::HistoryEntry::new(
            seed.action_id.clone(),
            seed.action_name.clone(),
            seed.original_text,
            seed.refined_text,
            chunks,
            stats,
            "openai/gpt-oss-120b".to_string(),
            seed.duration_ms,
            "auto".to_string(),
        );
        entry.timestamp = chrono::Local::now() - chrono::Duration::hours(seed.age_hours);
        store.add_history(entry);
    }
    println!("seeded {} history entries", store.history.len());
}

#[cfg(feature = "demo")]
fn demo_only_modes() -> bool {
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
        return true;
    }

    // Headless benchmark mode: `refined --bench=<model_id>` runs one Polish
    // completion on a fixed sample with the REFINED_DEMO_KEY environment key and
    // prints a JSON result line. Used to measure latency/quality per model.
    if let Some(model) =
        std::env::args().find_map(|a| a.strip_prefix("--bench=").map(str::to_string))
    {
        let mut config = groq::GroqConfig::default();
        if let Ok(k) = std::env::var("REFINED_DEMO_KEY") {
            config.api_key = k;
        }
        config.model = model;
        let action_id = std::env::args()
            .find_map(|a| a.strip_prefix("--bench-action=").map(str::to_string))
            .unwrap_or_else(|| "polish".to_string());
        let sample = "The replacment of human editors with AI is unlikely to happen soon, \
because AI models are unable to distinguishes between facts and convincing-sounding nonsense.";
        let (instruction, id_used) = crate::actions::default_actions()
            .into_iter()
            .find(|a| a.id == action_id)
            .map(|a| (a.prompt_instruction, a.id))
            .unwrap_or_default();
        let instruction = instruction.replace("{user_instruction}", "a formal academic tone");
        match crate::groq::complete_action(&config, &instruction, sample, "auto") {
            Ok(resp) => {
                let out = serde_json::json!({
                    "model": config.model,
                    "action": id_used,
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
        return true;
    }

    // Demo seeding: `refined --seed-history=<file>` loads a JSON array of
    // {action_id, action_name, original_text, refined_text, age_hours,
    // duration_ms} records and writes real history entries whose diff chunks
    // and stats come from the production diff engine, so captures show only
    // data the app could have produced itself.
    if let Some(seed_file) =
        std::env::args().find_map(|a| a.strip_prefix("--seed-history=").map(str::to_string))
    {
        seed_history(&seed_file);
        return true;
    }

    false
}

#[cfg(not(feature = "demo"))]
fn demo_only_modes() -> bool {
    false
}

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

    // Previous version cleaned off after a self-update
    updater::cleanup_after_update();

    if demo_only_modes() {
        return Ok(());
    }

    // Load store
    let store = Arc::new(Mutex::new(store::Store::open()));

    // Hotkey listener thread
    let (hotkey_tx, hotkey_rx) = channel();
    let quitting = Arc::new(AtomicBool::new(false));
    let hotkey_failed = Arc::new(AtomicBool::new(false));
    let _hotkey_handle = hotkey::start_hotkey_listener(hotkey_tx, quitting.clone(), hotkey_failed.clone());

    // Window options
    let hidden = std::env::args().any(|a| a == "--hidden");
    #[cfg(feature = "demo")]
    let demo_opts = app::DemoOptions::parse();
    #[cfg(not(feature = "demo"))]
    let demo_opts = app::DemoOptions::none();

    #[cfg(feature = "demo")]
    let inner_size = demo_opts
        .size
        .unwrap_or([960.0, 680.0]);
    #[cfg(not(feature = "demo"))]
    let inner_size = [960.0, 680.0];

    let initial_visible = !hidden || demo_opts.active;
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Refined Studio")
            .with_inner_size(inner_size)
            .with_min_inner_size([740.0, 520.0])
            .with_icon(Arc::new(eframe::egui::IconData {
                rgba: icon::rgba(64),
                width: 64,
                height: 64,
            }))
            .with_visible(initial_visible),
        ..Default::default()
    };

    let store_for_app = store.clone();
    let quitting_for_app = quitting.clone();
    let hotkey_failed_for_app = hotkey_failed.clone();
    let result = eframe::run_native(
        "Refined",
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::RefinedApp::new(
                cc,
                store_for_app,
                hotkey_rx,
                hotkey_failed_for_app,
                restore_event,
                quitting_for_app,
                initial_visible,
                demo_opts,
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
