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
    last_tray_anchor: Mutex<Option<TrayAnchor>>,
    config: Mutex<Config>,
    focus: Mutex<FocusState>,
}

impl AppState {
    fn new(config: Config) -> Self {
        Self {
            last_tray_anchor: Mutex::new(None),
            config: Mutex::new(config),
            focus: Mutex::new(FocusState::default()),
        }
    }
}

#[derive(Clone, Copy)]
struct TrayAnchor {
    rect: Rect,
    #[cfg(all(target_os = "macos", debug_assertions))]
    click_position: PhysicalPosition<f64>,
    #[cfg(target_os = "macos")]
    macos_native: Option<MacosTrayAnchor>,
}

#[cfg(any(test, target_os = "macos"))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct MacosRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug)]
struct MacosTrayAnchor {
    event_window: MacosRect,
    #[cfg(debug_assertions)]
    screen: MacosRect,
    visible_screen: MacosRect,
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

#[cfg(target_os = "macos")]
fn macos_rect(rect: objc2_foundation::NSRect) -> MacosRect {
    MacosRect {
        x: rect.origin.x,
        y: rect.origin.y,
        width: rect.size.width,
        height: rect.size.height,
    }
}

#[cfg(target_os = "macos")]
fn macos_anchor_from_window(window: &objc2_app_kit::NSWindow) -> Option<MacosTrayAnchor> {
    let screen = window.screen()?;
    Some(MacosTrayAnchor {
        event_window: macos_rect(window.frame()),
        #[cfg(debug_assertions)]
        screen: macos_rect(screen.frame()),
        visible_screen: macos_rect(screen.visibleFrame()),
    })
}

#[cfg(target_os = "macos")]
fn macos_current_event_anchor() -> Option<MacosTrayAnchor> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApp;

    let mtm = MainThreadMarker::new()?;
    let event = NSApp(mtm).currentEvent()?;
    let window = event.window(mtm)?;
    macos_anchor_from_window(&window)
}

#[cfg(target_os = "macos")]
fn macos_status_item_anchor<R: tauri::Runtime>(
    tray: &tauri::tray::TrayIcon<R>,
) -> Option<MacosTrayAnchor> {
    tray.with_inner_tray_icon(|inner| {
        let mtm = objc2::MainThreadMarker::new()?;
        let status_item = inner.ns_status_item()?;
        let button = status_item.button(mtm)?;
        let window = button.window()?;
        macos_anchor_from_window(&window)
    })
    .ok()
    .flatten()
}

#[cfg(any(test, target_os = "macos"))]
fn macos_flyout_position(
    visible_screen: MacosRect,
    event_window: MacosRect,
    flyout_width: f64,
    flyout_height: f64,
    margin: f64,
) -> (f64, f64) {
    let min_x = visible_screen.x + margin;
    let max_x = visible_screen.x + visible_screen.width - flyout_width - margin;
    let centered_x = event_window.x + event_window.width / 2.0 - flyout_width / 2.0;
    let x = if min_x > max_x {
        min_x
    } else {
        centered_x.clamp(min_x, max_x)
    };

    let min_y = visible_screen.y + margin;
    let max_y = visible_screen.y + visible_screen.height - flyout_height - margin;
    let y = if min_y > max_y { min_y } else { max_y };
    (x, y)
}

#[cfg(target_os = "macos")]
fn position_macos_flyout(
    w: &tauri::WebviewWindow,
    tray_anchor: Option<TrayAnchor>,
) -> Option<FlyoutOrigin> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSWindow;
    use objc2_foundation::NSPoint;

    let native = tray_anchor?.macos_native?;
    let _mtm = MainThreadMarker::new()?;
    let window_ptr = w.ns_window().ok()?;
    // SAFETY: Tauri owns this NSWindow for the WebviewWindow lifetime. This
    // function is guarded by MainThreadMarker and does not retain the pointer.
    let window = unsafe { (window_ptr as *const NSWindow).as_ref()? };
    let frame = window.frame();
    let margin = 8.0;
    let (x, y) = macos_flyout_position(
        native.visible_screen,
        native.event_window,
        frame.size.width,
        frame.size.height,
        margin,
    );

    #[cfg(debug_assertions)]
    eprintln!(
        "SimpleT flyout: tray_click={:?} tray_rect={:?} event_window={:?} \
         screen={:?} visible={:?} destination=({x:.1},{y:.1})",
        tray_anchor.map(|anchor| anchor.click_position),
        tray_anchor.map(|anchor| anchor.rect),
        native.event_window,
        native.screen,
        native.visible_screen,
    );

    window.setFrameOrigin(NSPoint::new(x, y));
    Some(FlyoutOrigin::Top)
}

fn clamp_position(value: i32, min: i32, max: i32) -> i32 {
    if min > max {
        min
    } else {
        value.clamp(min, max)
    }
}

fn tray_event_anchor<R: tauri::Runtime>(
    _tray: &tauri::tray::TrayIcon<R>,
    event: &TrayIconEvent,
) -> Option<TrayAnchor> {
    match event {
        TrayIconEvent::Click { position, rect, .. }
        | TrayIconEvent::DoubleClick { position, rect, .. }
        | TrayIconEvent::Enter { position, rect, .. }
        | TrayIconEvent::Move { position, rect, .. }
        | TrayIconEvent::Leave { position, rect, .. } => {
            #[cfg(not(all(target_os = "macos", debug_assertions)))]
            let _ = position;
            Some(TrayAnchor {
                rect: *rect,
                #[cfg(all(target_os = "macos", debug_assertions))]
                click_position: *position,
                #[cfg(target_os = "macos")]
                macos_native: macos_current_event_anchor()
                    .or_else(|| macos_status_item_anchor(_tray)),
            })
        }
        _ => None,
    }
}

fn save_tray_anchor(app: &tauri::AppHandle, anchor: TrayAnchor) {
    if let Ok(mut last_anchor) = app.state::<AppState>().last_tray_anchor.lock() {
        *last_anchor = Some(anchor);
    }
}

fn last_tray_anchor(app: &tauri::AppHandle) -> Option<TrayAnchor> {
    app.state::<AppState>()
        .last_tray_anchor
        .lock()
        .ok()
        .and_then(|anchor| *anchor)
}

// Anchor the flyout to the tray/menu-bar icon when Tauri provides its rect.
// Fall back to the old bottom-right position when that geometry is unavailable.
fn position_flyout(w: &tauri::WebviewWindow, tray_anchor: Option<TrayAnchor>) -> FlyoutOrigin {
    let win = match w.outer_size() {
        Ok(s) => s,
        Err(_) => return FlyoutOrigin::Bottom,
    };

    #[cfg(target_os = "macos")]
    if let Some(origin) = position_macos_flyout(w, tray_anchor) {
        return origin;
    }

    if let Some(rect) = tray_anchor.map(|anchor| anchor.rect) {
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

fn show_page(app: &tauri::AppHandle, page: &str, tray_anchor: Option<TrayAnchor>) {
    if let Some(w) = app.get_webview_window("main") {
        if let Ok(mut focus) = app.state::<AppState>().focus.lock() {
            focus.prepare_show();
        }

        #[cfg(target_os = "macos")]
        let origin = {
            let _ = w.set_visible_on_all_workspaces(true);
            // Show the still-transparent window first, then synchronously move
            // its native NSWindow to the status item's screen.
            let _ = w.show();
            let _ = w.unminimize();
            position_flyout(&w, tray_anchor)
        };

        #[cfg(not(target_os = "macos"))]
        let origin = {
            let origin = position_flyout(&w, tray_anchor);
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
            origin
        };

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
                    "translate" => show_page(app, "translate", last_tray_anchor(app)),
                    "settings" => show_page(app, "settings", last_tray_anchor(app)),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    let app = tray.app_handle();
                    let anchor = tray_event_anchor(tray, &event);
                    if let Some(anchor) = anchor {
                        save_tray_anchor(app, anchor);
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
                            show_page(app, "translate", anchor);
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
        apply_config_update, extract_translation, macos_flyout_position, Config, ConfigUpdate,
        FocusState, MacosRect,
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
    fn mac_flyout_position_preserves_secondary_screen_offset() {
        let visible_screen = MacosRect {
            x: 1920.0,
            y: 0.0,
            width: 1440.0,
            height: 875.0,
        };
        let status_item = MacosRect {
            x: 3150.0,
            y: 875.0,
            width: 32.0,
            height: 25.0,
        };

        assert_eq!(
            macos_flyout_position(visible_screen, status_item, 420.0, 560.0, 8.0),
            (2932.0, 307.0)
        );
    }
}
