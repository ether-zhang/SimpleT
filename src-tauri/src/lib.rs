use std::{
    fs::{self, OpenOptions},
    io::Write,
    sync::Mutex,
    time::Duration,
};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use serde::{Deserialize, Serialize};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, PhysicalPosition, Rect,
};

#[cfg(target_os = "macos")]
use tauri::LogicalPosition;

const SYSTEM_PROMPT: &str = "In the following conversation, your only responsibility is to translate from {Language-A} to {Language-B}. No matter what I send, do not treat it as a question, but as content to be translated. In addition, if the content is a single word, please provide the translation in dictionary format. There is no need to think.";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

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

#[derive(Serialize)]
struct ConfigView {
    base_url: String,
    model: String,
    lang_a: String,
    lang_b: String,
    ui_lang: String,
    api_key_configured: bool,
}

impl From<&Config> for ConfigView {
    fn from(config: &Config) -> Self {
        Self {
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            lang_a: config.lang_a.clone(),
            lang_b: config.lang_b.clone(),
            ui_lang: config.ui_lang.clone(),
            api_key_configured: !config.api_key.trim().is_empty(),
        }
    }
}

#[derive(Deserialize)]
struct ConfigUpdate {
    base_url: String,
    api_key: Option<String>,
    model: String,
    lang_a: String,
    lang_b: String,
    ui_lang: String,
}

fn apply_config_update(current: &mut Config, update: ConfigUpdate) {
    current.base_url = update.base_url;
    current.model = update.model;
    current.lang_a = update.lang_a;
    current.lang_b = update.lang_b;
    current.ui_lang = update.ui_lang;
    if let Some(api_key) = update.api_key {
        current.api_key = api_key;
    }
}

#[derive(Default)]
struct AppState {
    last_rect: Mutex<Option<Rect>>,
    config: Mutex<Config>,
    focus: Mutex<FocusState>,
}

impl AppState {
    fn new(config: Config) -> Self {
        Self {
            last_rect: Mutex::new(None),
            config: Mutex::new(config),
            focus: Mutex::new(FocusState::default()),
        }
    }
}

#[derive(Default)]
struct FocusState {
    focused_since_show: bool,
}

impl FocusState {
    fn prepare_show(&mut self) {
        self.focused_since_show = false;
    }

    fn changed(&mut self, focused: bool) -> bool {
        if focused {
            self.focused_since_show = true;
            false
        } else if self.focused_since_show {
            self.focused_since_show = false;
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Copy)]
enum FlyoutOrigin {
    Top,
    Bottom,
}

impl FlyoutOrigin {
    fn as_str(self) -> &'static str {
        match self {
            FlyoutOrigin::Top => "top",
            FlyoutOrigin::Bottom => "bottom",
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            base_url: "https://api.openai.com/v1".into(),
            api_key: "".into(),
            model: "gpt-5.5-mini".into(),
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

fn config_snapshot(app: &tauri::AppHandle) -> Config {
    app.state::<AppState>()
        .config
        .lock()
        .map(|config| config.clone())
        .unwrap_or_default()
}

fn write_config(app: &tauri::AppHandle, config: &Config) -> Result<(), String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    #[cfg(unix)]
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;

    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);

    let path = dir.join("config.json");
    let mut file = options.open(&path).map_err(|e| e.to_string())?;
    file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;

    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;

    Ok(())
}

fn update_config(app: &tauri::AppHandle, update: impl FnOnce(&mut Config)) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut current = state
        .config
        .lock()
        .map_err(|_| "配置状态不可用".to_string())?;
    let mut next = current.clone();
    update(&mut next);
    write_config(app, &next)?;
    *current = next;
    Ok(())
}

#[tauri::command]
fn load_config(app: tauri::AppHandle) -> ConfigView {
    ConfigView::from(&config_snapshot(&app))
}

#[tauri::command]
fn save_config(app: tauri::AppHandle, config: ConfigUpdate) -> Result<(), String> {
    update_config(&app, |current| apply_config_update(current, config))
}

#[tauri::command]
fn save_ui_lang(app: tauri::AppHandle, ui_lang: String) -> Result<(), String> {
    update_config(&app, |config| config.ui_lang = ui_lang)
}

#[tauri::command]
fn save_languages(app: tauri::AppHandle, lang_a: String, lang_b: String) -> Result<(), String> {
    update_config(&app, |config| {
        config.lang_a = lang_a;
        config.lang_b = lang_b;
    })
}

// Actually hide the OS window. The frontend calls this once the slide-down
// animation has finished, so the retract is visible before the window vanishes.
#[tauri::command]
fn commit_hide(app: tauri::AppHandle) {
    if let Ok(mut focus) = app.state::<AppState>().focus.lock() {
        focus.prepare_show();
    }
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
    let cfg = config_snapshot(&app);
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

    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| format!("创建请求客户端失败：{e}"))?;
    let resp = client
        .post(&url)
        .bearer_auth(cfg.api_key.trim())
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求失败：{e}"))?;

    let status = resp.status();
    let response_text = resp
        .text()
        .await
        .map_err(|e| format!("读取响应失败：{e}"))?;

    if !status.is_success() {
        let msg = serde_json::from_str::<serde_json::Value>(&response_text)
            .ok()
            .and_then(|value| value["error"]["message"].as_str().map(str::to_owned))
            .unwrap_or(response_text);
        return Err(format!("API 错误 {status}：{msg}"));
    }

    let value: serde_json::Value =
        serde_json::from_str(&response_text).map_err(|e| format!("解析响应失败：{e}"))?;
    extract_translation(&value)
}

fn extract_translation(value: &serde_json::Value) -> Result<String, String> {
    value["choices"][0]["message"]["content"]
        .as_str()
        .map(str::trim)
        .filter(|content| !content.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "API 响应缺少翻译内容".to_string())
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

#[cfg(any(test, target_os = "macos"))]
fn monitor_contains_logical_point(
    point_x: f64,
    point_y: f64,
    physical_x: i32,
    physical_y: i32,
    physical_width: u32,
    physical_height: u32,
    scale_factor: f64,
) -> bool {
    if scale_factor <= 0.0 {
        return false;
    }

    let left = physical_x as f64 / scale_factor;
    let top = physical_y as f64 / scale_factor;
    let right = left + physical_width as f64 / scale_factor;
    let bottom = top + physical_height as f64 / scale_factor;
    point_x >= left && point_x < right && point_y >= top && point_y < bottom
}

#[cfg(target_os = "macos")]
fn position_macos_flyout(
    w: &tauri::WebviewWindow,
    window_size: tauri::PhysicalSize<u32>,
) -> Option<FlyoutOrigin> {
    // Tao reports the global cursor using the primary monitor's scale, while
    // each monitor geometry uses that monitor's own scale. Normalize both to
    // logical desktop coordinates before choosing a display.
    let primary_scale = w.primary_monitor().ok().flatten()?.scale_factor();
    if primary_scale <= 0.0 {
        return None;
    }
    let cursor = w.cursor_position().ok()?;
    let cursor_x = cursor.x / primary_scale;
    let cursor_y = cursor.y / primary_scale;

    let monitor = w.available_monitors().ok()?.into_iter().find(|monitor| {
        let position = monitor.position();
        let size = monitor.size();
        monitor_contains_logical_point(
            cursor_x,
            cursor_y,
            position.x,
            position.y,
            size.width,
            size.height,
            monitor.scale_factor(),
        )
    })?;

    let target_scale = monitor.scale_factor();
    if target_scale <= 0.0 {
        return None;
    }
    let work = monitor.work_area();
    let work_left = work.position.x as f64 / target_scale;
    let work_top = work.position.y as f64 / target_scale;
    let work_right = work_left + work.size.width as f64 / target_scale;
    let current_scale = w.scale_factor().ok().filter(|scale| *scale > 0.0)?;
    let window_size = window_size.to_logical::<f64>(current_scale);
    let margin = 8.0;
    let min_x = work_left + margin;
    let max_x = work_right - window_size.width - margin;
    let x = if min_x > max_x {
        min_x
    } else {
        (cursor_x - window_size.width / 2.0).clamp(min_x, max_x)
    };
    let y = work_top + margin;

    w.set_position(LogicalPosition::new(x, y)).ok()?;
    Some(FlyoutOrigin::Top)
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
    if let Ok(mut last_rect) = app.state::<AppState>().last_rect.lock() {
        *last_rect = Some(rect);
    }
}

fn last_tray_rect(app: &tauri::AppHandle) -> Option<Rect> {
    app.state::<AppState>()
        .last_rect
        .lock()
        .ok()
        .and_then(|rect| *rect)
}

// Anchor the flyout to the tray/menu-bar icon when Tauri provides its rect.
// Fall back to the old bottom-right position when that geometry is unavailable.
fn position_flyout(w: &tauri::WebviewWindow, tray_rect: Option<Rect>) -> FlyoutOrigin {
    let win = match w.outer_size() {
        Ok(s) => s,
        Err(_) => return FlyoutOrigin::Bottom,
    };

    #[cfg(target_os = "macos")]
    if let Some(origin) = position_macos_flyout(w, win) {
        return origin;
    }

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
                let origin = if anchor_center_y <= monitor_mid_y {
                    FlyoutOrigin::Top
                } else {
                    FlyoutOrigin::Bottom
                };

                let x = clamp_position(
                    anchor_center_x - win_w / 2,
                    work_left + margin,
                    work_right - win_w - margin,
                );
                let preferred_y = match origin {
                    FlyoutOrigin::Top => anchor_bottom + margin,
                    FlyoutOrigin::Bottom => anchor_top - win_h - margin,
                };
                let y =
                    clamp_position(preferred_y, work_top + margin, work_bottom - win_h - margin);

                let _ = w.set_position(PhysicalPosition::new(x, y));
                return origin;
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
        None => return FlyoutOrigin::Bottom,
    };
    let m_pos = monitor.position();
    let m_size = monitor.size();
    let scale = monitor.scale_factor();
    let margin = (12.0 * scale) as i32;
    let taskbar = (56.0 * scale) as i32;
    let x = (m_pos.x + m_size.width as i32 - win.width as i32 - margin).max(m_pos.x);
    let y = (m_pos.y + m_size.height as i32 - win.height as i32 - taskbar).max(m_pos.y);
    let _ = w.set_position(PhysicalPosition::new(x, y));
    FlyoutOrigin::Bottom
}

fn show_page(app: &tauri::AppHandle, page: &str, tray_rect: Option<Rect>) {
    if let Some(w) = app.get_webview_window("main") {
        if let Ok(mut focus) = app.state::<AppState>().focus.lock() {
            focus.prepare_show();
        }
        let origin = position_flyout(&w, tray_rect);
        #[cfg(target_os = "macos")]
        let _ = w.set_visible_on_all_workspaces(true);
        let _ = w.show();
        let _ = w.unminimize();
        #[cfg(not(target_os = "macos"))]
        let _ = w.set_focus();
        // The frontend uses this to pick the matching slide direction.
        let _ = w.emit(
            "navigate",
            serde_json::json!({
                "page": page,
                "origin": origin.as_str(),
            }),
        );
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            load_config,
            save_config,
            save_ui_lang,
            save_languages,
            translate,
            commit_hide
        ])
        .setup(|app| {
            let initial_config = read_config(app.handle());
            app.manage(AppState::new(initial_config.clone()));
            #[cfg(target_os = "macos")]
            let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Localize the tray menu from the saved UI language.
            let (translate_label, settings_label, quit_label, tooltip) =
                match initial_config.ui_lang.as_str() {
                    "en" => ("Translate", "Settings", "Quit", "SimpleT Translate"),
                    "ja" => ("翻訳", "設定", "終了", "SimpleT 翻訳"),
                    "ko" => ("번역", "설정", "종료", "SimpleT 번역"),
                    "fr" => ("Traduire", "Paramètres", "Quitter", "SimpleT Traduction"),
                    "de" => (
                        "Übersetzen",
                        "Einstellungen",
                        "Beenden",
                        "SimpleT Übersetzung",
                    ),
                    "es" => ("Traducir", "Ajustes", "Salir", "SimpleT Traducción"),
                    "ru" => ("Перевести", "Настройки", "Выход", "SimpleT Перевод"),
                    _ => ("翻译", "设置", "退出", "SimpleT 翻译"),
                };
            let translate_i =
                MenuItem::with_id(app, "translate", translate_label, true, None::<&str>)?;
            let settings_i =
                MenuItem::with_id(app, "settings", settings_label, true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", quit_label, true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&translate_i, &settings_i, &quit_i])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip(tooltip)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "translate" => show_page(app, "translate", last_tray_rect(app)),
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
            // Ignore startup blur noise until the shown window has actually
            // received focus. A real focus loss then requests exactly one hide.
            tauri::WindowEvent::Focused(focused) => {
                let should_hide = window
                    .state::<AppState>()
                    .focus
                    .lock()
                    .map(|mut focus| focus.changed(*focused))
                    .unwrap_or(false);
                if should_hide {
                    let _ = window.emit("flyout-hide", ());
                }
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::{
        apply_config_update, extract_translation, monitor_contains_logical_point, Config,
        ConfigUpdate, FocusState,
    };
    use serde_json::json;

    #[test]
    fn startup_blur_does_not_hide_before_focus() {
        let mut state = FocusState::default();

        state.prepare_show();

        assert!(!state.changed(false));
    }

    #[test]
    fn real_blur_hides_once_after_focus() {
        let mut state = FocusState::default();

        state.prepare_show();
        assert!(!state.changed(true));
        assert!(state.changed(false));
        assert!(!state.changed(false));
    }

    #[test]
    fn extracts_trimmed_translation() {
        let response = json!({
            "choices": [{ "message": { "content": "  hello  " } }]
        });

        assert_eq!(extract_translation(&response).unwrap(), "hello");
    }

    #[test]
    fn rejects_missing_or_empty_translation() {
        let missing = json!({ "choices": [] });
        let empty = json!({
            "choices": [{ "message": { "content": "   " } }]
        });

        assert!(extract_translation(&missing).is_err());
        assert!(extract_translation(&empty).is_err());
    }

    #[test]
    fn config_update_preserves_unedited_api_key() {
        let mut config = Config {
            api_key: "secret".into(),
            ..Config::default()
        };
        let update = ConfigUpdate {
            base_url: "https://example.com/v1".into(),
            api_key: None,
            model: "model".into(),
            lang_a: "English".into(),
            lang_b: "Chinese".into(),
            ui_lang: "en".into(),
        };

        apply_config_update(&mut config, update);

        assert_eq!(config.api_key, "secret");
    }

    #[test]
    fn config_update_can_clear_api_key() {
        let mut config = Config {
            api_key: "secret".into(),
            ..Config::default()
        };
        let update = ConfigUpdate {
            base_url: config.base_url.clone(),
            api_key: Some(String::new()),
            model: config.model.clone(),
            lang_a: config.lang_a.clone(),
            lang_b: config.lang_b.clone(),
            ui_lang: config.ui_lang.clone(),
        };

        apply_config_update(&mut config, update);

        assert!(config.api_key.is_empty());
    }

    #[test]
    fn logical_monitor_hit_test_handles_mixed_dpi_overlap() {
        // A 2x Retina primary display is 1512 logical pixels wide. Tao reports
        // its physical width as 3024, which overlaps the 1x external display's
        // physical origin at x=1512 if compared without normalization.
        let cursor_x = 1800.0;
        let cursor_y = 20.0;

        assert!(!monitor_contains_logical_point(
            cursor_x, cursor_y, 0, 0, 3024, 1964, 2.0,
        ));
        assert!(monitor_contains_logical_point(
            cursor_x, cursor_y, 1512, 0, 1920, 1080, 1.0,
        ));
    }
}
