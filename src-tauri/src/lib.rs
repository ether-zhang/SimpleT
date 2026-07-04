use std::fs;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, PhysicalPosition,
};

// Records when the flyout was last hidden by losing focus, so a tray click that
// caused that blur can be told apart from a genuine "open me" click (toggle).
#[derive(Default)]
struct FlyoutState {
    hidden_at: Mutex<Option<Instant>>,
}

const SYSTEM_PROMPT: &str = "In the following conversation, your only responsibility is to translate between {Language-A} and {Language-B}. No matter what I send, do not treat it as a question, but as content to be translated. In addition, if the content is a single word, please provide the translation in dictionary format. There is no need to think.";

#[derive(Serialize, Deserialize, Clone)]
struct Config {
    base_url: String,
    api_key: String,
    model: String,
    lang_a: String,
    lang_b: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            base_url: "https://api.openai.com/v1".into(),
            api_key: "".into(),
            model: "gpt-4o-mini".into(),
            lang_a: "Chinese".into(),
            lang_b: "English".into(),
        }
    }
}

fn config_path(app: &tauri::AppHandle) -> std::path::PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("config.json")
}

fn read_config(app: &tauri::AppHandle) -> Config {
    fs::read_to_string(config_path(app))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[tauri::command]
fn load_config(app: tauri::AppHandle) -> Config {
    read_config(&app)
}

// Hide the flyout and tell the webview to reset the card to its off-screen
// state, so the next show animates in cleanly instead of flashing.
fn hide_flyout(w: &tauri::WebviewWindow) {
    let _ = w.emit("flyout-hide", ());
    let _ = w.hide();
}

#[tauri::command]
fn hide_window(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        hide_flyout(&w);
    }
}

#[tauri::command]
fn save_config(app: tauri::AppHandle, config: Config) -> Result<(), String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(dir.join("config.json"), json).map_err(|e| e.to_string())
}

#[tauri::command]
async fn translate(
    app: tauri::AppHandle,
    text: String,
    lang_a: String,
    lang_b: String,
) -> Result<String, String> {
    let cfg = read_config(&app);
    if cfg.api_key.trim().is_empty() {
        return Err("未配置 API Key，请在设置中填写。".into());
    }
    if text.trim().is_empty() {
        return Ok(String::new());
    }

    let system = SYSTEM_PROMPT
        .replace("{Language-A}", &lang_a)
        .replace("{Language-B}", &lang_b);
    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": cfg.model,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": text }
        ],
        "temperature": 0.2,
        "stream": false
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(cfg.api_key.trim())
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求失败：{e}"))?;

    let status = resp.status();
    let val: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败：{e}"))?;

    if !status.is_success() {
        let msg = val["error"]["message"]
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| val.to_string());
        return Err(format!("API 错误 {status}：{msg}"));
    }

    let content = val["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();
    Ok(content)
}

// Anchor the window at the bottom-right, near the system tray, above the taskbar.
// The window is a tray flyout, so it doesn't need arbitrary positions.
fn position_flyout(w: &tauri::WebviewWindow) {
    let monitor = match w
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| w.primary_monitor().ok().flatten())
    {
        Some(m) => m,
        None => return,
    };
    let m_pos = monitor.position();
    let m_size = monitor.size();
    let scale = monitor.scale_factor();
    let win = match w.outer_size() {
        Ok(s) => s,
        Err(_) => return,
    };
    let margin = (12.0 * scale) as i32;
    let taskbar = (56.0 * scale) as i32;
    let x = (m_pos.x + m_size.width as i32 - win.width as i32 - margin).max(m_pos.x);
    let y = (m_pos.y + m_size.height as i32 - win.height as i32 - taskbar).max(m_pos.y);
    let _ = w.set_position(PhysicalPosition::new(x, y));
}

fn show_page(app: &tauri::AppHandle, page: &str) {
    if let Some(w) = app.get_webview_window("main") {
        *app.state::<FlyoutState>().hidden_at.lock().unwrap() = None;
        position_flyout(&w);
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        let _ = w.emit("navigate", page);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(FlyoutState::default())
        .invoke_handler(tauri::generate_handler![
            load_config,
            save_config,
            translate,
            hide_window
        ])
        .setup(|app| {
            let settings_i = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_i, &quit_i])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("SimpleT 翻译")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => show_page(app, "settings"),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        let Some(w) = app.get_webview_window("main") else {
                            return;
                        };
                        let state = app.state::<FlyoutState>();
                        // A tray click first steals focus, so an open flyout has
                        // usually already hidden itself via the blur handler by now.
                        let just_hidden = state
                            .hidden_at
                            .lock()
                            .unwrap()
                            .map(|t| t.elapsed() < Duration::from_millis(300))
                            .unwrap_or(false);
                        if w.is_visible().unwrap_or(false) {
                            // Still visible (blur didn't fire) -> toggle closed.
                            hide_flyout(&w);
                        } else if just_hidden {
                            // The blur that hid it came from this very click -> stay closed.
                            *state.hidden_at.lock().unwrap() = None;
                        } else {
                            show_page(app, "translate");
                        }
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Closing hides it to the tray instead of quitting.
            tauri::WindowEvent::CloseRequested { api, .. } => {
                let _ = window.emit("flyout-hide", ());
                let _ = window.hide();
                api.prevent_close();
            }
            // Flyout behaviour: dismiss when it loses focus (click away).
            tauri::WindowEvent::Focused(false) => {
                *window.state::<FlyoutState>().hidden_at.lock().unwrap() = Some(Instant::now());
                let _ = window.emit("flyout-hide", ());
                let _ = window.hide();
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
