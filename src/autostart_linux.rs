//! Linux autostart through an XDG autostart desktop entry.

use std::path::PathBuf;

fn entry_path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config.join("autostart").join("refined.desktop"))
}

pub fn enabled() -> bool {
    entry_path().is_some_and(|path| path.exists())
}

pub fn set(on: bool) {
    let Some(path) = entry_path() else { return };
    if !on {
        let _ = std::fs::remove_file(path);
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    // Desktop entries quote arguments with double quotes and escape \ " ` $.
    let mut quoted = String::from('"');
    for ch in exe.to_string_lossy().chars() {
        if matches!(ch, '\\' | '"' | '`' | '$') {
            quoted.push('\\');
        }
        quoted.push(ch);
    }
    quoted.push('"');
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=Refined\nComment=AI text refiner\nExec={quoted} --hidden\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    );
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, entry);
}

pub fn refresh() {
    if enabled() {
        set(true);
    }
}
