//! Translation, persistence and model discovery shared by both desktop frontends.
//! This crate does not create windows, initialize a WebView or depend on Tauri.

pub mod config;
pub mod history;
pub mod http;
pub mod models;
pub mod translation;

pub use translation::{CONNECT_TIMEOUT, REQUEST_TIMEOUT};

#[cfg(test)]
fn test_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
