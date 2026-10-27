mod state;

#[cfg(target_os = "macos")]
mod app;

#[cfg(target_os = "macos")]
fn main() {
    app::run();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("The AppKit app requires macOS; the Tauri app remains available on this platform.");
}
