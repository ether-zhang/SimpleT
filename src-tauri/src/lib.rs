use std::{fs, sync::Mutex};

use serde::{Deserialize, Serialize};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, PhysicalPosition, Rect,
};

const SYSTEM_PROMPT: &str = "In the following conversation, your only responsibility is to translate between {Language-A} and {Language-B}. No matter what I send, do not treat it as a question, but as content to be translated. In addition, if the content is a single word, please provide the translation in dictionary format. There is no need to think.";

fn default_ui_lang() -> String {
    "zh".into()
}

#[derive(Serialize, Deserialize, Clone)]
struct Config {
    base_url: String,
    api_key: String,
    model: String,
    lang_a: String,
    lang_b: String,
    // Added later; `serde(default)` keeps older config.json (without it) loadable.
    #[serde(default = "default_ui_lang")]
    ui_lang: String,
}

#[derive(Default)]
struct TrayGeometry {
    last_rect: Mutex<Option<Rect>>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            base_url: "https://api.openai.com/v1".into(),
            api_key: "".into(),
            model: "gpt-4o-mini".into(),
            lang_a: "Chinese".into(),
            lang_b: "English".into(),
            ui_lang: default_ui_lang(),
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

#[tauri::command]
fn save_config(app: tauri::AppHandle, config: Config) -> Result<(), String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(dir.join("config.json"), json).map_err(|e| e.to_string())
}

// Actually hide the OS window. The frontend calls this once the slide-down
// animation has finished, so the retract is visible before the window vanishes.
#[tauri::command]
fn commit_hide(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
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

fn monitor_at_point(w: &tauri::WebviewWindow, x: i32, y: i32) -> Option<tauri::Monitor> {
    if let Ok(monitors) = w.available_monitors() {
        if let Some(monitor) = monitors.into_iter().find(|m| {
            let pos = m.position();
            let size = m.size();
            let right = pos.x + size.width as i32;
            let bottom = pos.y + size.height as i32;
            x >= pos.x && x < right && y >= pos.y && y < bottom
        }) {
            return Some(monitor);
        }
    }

    w.current_monitor()
        .ok()
        .flatten()
        .or_else(|| w.primary_monitor().ok().flatten())
}

fn clamp_position(value: i32, min: i32, max: i32) -> i32 {
    if min > max {
        min
    } else {
        value.clamp(min, max)
    }
}

fn tray_event_rect(event: &TrayIconEvent) -> Option<Rect> {
    match event {
        TrayIconEvent::Click { rect, .. }
        | TrayIconEvent::DoubleClick { rect, .. }
        | TrayIconEvent::Enter { rect, .. }
        | TrayIconEvent::Move { rect, .. }
        | TrayIconEvent::Leave { rect, .. } => Some(*rect),
        _ => None,
    }
}

fn save_tray_rect(app: &tauri::AppHandle, rect: Rect) {
    if let Ok(mut last_rect) = app.state::<TrayGeometry>().last_rect.lock() {
        *last_rect = Some(rect);
    }
}

fn last_tray_rect(app: &tauri::AppHandle) -> Option<Rect> {
    app.state::<TrayGeometry>()
        .last_rect
        .lock()
        .ok()
        .and_then(|rect| *rect)
}

// Anchor the flyout to the tray/menu-bar icon when Tauri provides its rect.
// Fall back to the old bottom-right position when that geometry is unavailable.
fn position_flyout(w: &tauri::WebviewWindow, tray_rect: Option<Rect>) {
    let win = match w.outer_size() {
        Ok(s) => s,
        Err(_) => return,
    };

    if let Some(rect) = tray_rect {
        let pos = rect.position.to_physical::<i32>(1.0);
        let size = rect.size.to_physical::<u32>(1.0);
        if size.width > 0 && size.height > 0 {
            let anchor_center_x = pos.x + size.width as i32 / 2;
            let anchor_center_y = pos.y + size.height as i32 / 2;

            if let Some(monitor) = monitor_at_point(w, anchor_center_x, anchor_center_y) {
                let work = monitor.work_area();
                let margin = (8.0 * monitor.scale_factor()).round() as i32;
                let win_w = win.width as i32;
                let win_h = win.height as i32;
                let work_left = work.position.x;
                let work_top = work.position.y;
                let work_right = work_left + work.size.width as i32;
                let work_bottom = work_top + work.size.height as i32;
                let anchor_top = pos.y;
                let anchor_bottom = pos.y + size.height as i32;
                let monitor_mid_y = monitor.position().y + monitor.size().height as i32 / 2;

                let x = clamp_position(
                    anchor_center_x - win_w / 2,
                    work_left + margin,
                    work_right - win_w - margin,
                );
                let preferred_y = if anchor_center_y <= monitor_mid_y {
                    anchor_bottom + margin
                } else {
                    anchor_top - win_h - margin
                };
                let y =
                    clamp_position(preferred_y, work_top + margin, work_bottom - win_h - margin);

                let _ = w.set_position(PhysicalPosition::new(x, y));
                return;
            }
        }
    }

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
    let margin = (12.0 * scale) as i32;
    let taskbar = (56.0 * scale) as i32;
    let x = (m_pos.x + m_size.width as i32 - win.width as i32 - margin).max(m_pos.x);
    let y = (m_pos.y + m_size.height as i32 - win.height as i32 - taskbar).max(m_pos.y);
    let _ = w.set_position(PhysicalPosition::new(x, y));
}

fn show_page(app: &tauri::AppHandle, page: &str, tray_rect: Option<Rect>) {
    if let Some(w) = app.get_webview_window("main") {
        position_flyout(&w, tray_rect);
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        // Frontend slides the card up and focuses the input.
        let _ = w.emit("navigate", page);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            load_config,
            save_config,
            translate,
            commit_hide
        ])
        .setup(|app| {
            app.manage(TrayGeometry::default());

            // Localize the tray menu from the saved UI language.
            let (settings_label, quit_label, tooltip) =
                match read_config(app.handle()).ui_lang.as_str() {
                    "en" => ("Settings", "Quit", "SimpleT Translate"),
                    "ja" => ("設定", "終了", "SimpleT 翻訳"),
                    "ko" => ("설정", "종료", "SimpleT 번역"),
                    "fr" => ("Paramètres", "Quitter", "SimpleT Traduction"),
                    "de" => ("Einstellungen", "Beenden", "SimpleT Übersetzung"),
                    "es" => ("Ajustes", "Salir", "SimpleT Traducción"),
                    "ru" => ("Настройки", "Выход", "SimpleT Перевод"),
                    _ => ("设置", "退出", "SimpleT 翻译"),
                };
            let settings_i =
                MenuItem::with_id(app, "settings", settings_label, true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", quit_label, true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_i, &quit_i])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip(tooltip)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => show_page(app, "settings", last_tray_rect(app)),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    let app = tray.app_handle();
                    let rect = tray_event_rect(&event);
                    if let Some(rect) = rect {
                        save_tray_rect(app, rect);
                    }

                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let Some(w) = app.get_webview_window("main") else {
                            return;
                        };
                        // The close animation keeps the window visible while it
                        // slides down, so `is_visible` still reflects "open" here:
                        // open -> ask the frontend to slide it out; closed -> show.
                        if w.is_visible().unwrap_or(false) {
                            let _ = w.emit("flyout-hide", ());
                        } else {
                            show_page(app, "translate", rect);
                        }
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // No title bar, but Alt+F4 etc. still request close: hide, don't quit.
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.emit("flyout-hide", ());
            }
            // Flyout behaviour: slide out when it loses focus (click away).
            tauri::WindowEvent::Focused(false) => {
                let _ = window.emit("flyout-hide", ());
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
