//! Single instance on Linux through a Unix socket in the runtime directory.
//! A second launch asks the running app to show Studio, and
//! `refined --trigger` asks it to open the palette.

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

pub const SHOW: &[u8] = b"show";
pub const TRIGGER: &[u8] = b"trigger";
const QUIT: &[u8] = b"quit";

fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(std::env::temp_dir);
    let user = std::env::var("USER").unwrap_or_default();
    dir.join(format!("refined-{user}.sock"))
}

/// Returns the listener when this is the first instance. Otherwise sends
/// `message` to the running instance and returns None.
pub fn claim(message: &[u8]) -> Option<UnixListener> {
    let path = socket_path();
    if let Ok(mut stream) = UnixStream::connect(&path) {
        let _ = stream.write_all(message);
        return None;
    }
    // Nothing answers, so the file is left over from a crash.
    let _ = std::fs::remove_file(&path);
    UnixListener::bind(&path).ok()
}

/// Calls `on_message` for each message until [`release`] runs.
pub fn listen(listener: UnixListener, on_message: impl Fn(&[u8])) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut message = Vec::new();
        let _ = (&mut stream).take(64).read_to_end(&mut message);
        if message == QUIT {
            break;
        }
        on_message(&message);
    }
}

/// Stops [`listen`] and removes the socket.
pub fn release() {
    let path = socket_path();
    if let Ok(mut stream) = UnixStream::connect(&path) {
        let _ = stream.write_all(QUIT);
    }
    let _ = std::fs::remove_file(path);
}
