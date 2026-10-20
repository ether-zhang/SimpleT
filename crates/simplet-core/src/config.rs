use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{fs, io::Write, path::Path};

fn default_ui_lang() -> String {
    "zh".into()
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    #[default]
    Default,
    LiquidGlass,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub lang_a: String,
    pub lang_b: String,
    // Added later; `serde(default)` keeps older config.json (without it) loadable.
    #[serde(default = "default_ui_lang")]
    pub ui_lang: String,
    #[serde(default)]
    pub appearance: Appearance,
}

#[derive(Serialize)]
pub struct ConfigView {
    pub base_url: String,
    pub model: String,
    pub lang_a: String,
    pub lang_b: String,
    pub ui_lang: String,
    pub appearance: Appearance,
    pub api_key_configured: bool,
    pub load_error: Option<String>,
}

impl From<&Config> for ConfigView {
    fn from(config: &Config) -> Self {
        Self {
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            lang_a: config.lang_a.clone(),
            lang_b: config.lang_b.clone(),
            ui_lang: config.ui_lang.clone(),
            appearance: config.appearance,
            api_key_configured: !config.api_key.trim().is_empty(),
            load_error: None,
        }
    }
}

#[derive(Deserialize)]
pub struct ConfigUpdate {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub lang_a: String,
    pub lang_b: String,
    pub ui_lang: String,
    #[serde(default)]
    pub appearance: Option<Appearance>,
}

pub fn apply_config_update(current: &mut Config, update: ConfigUpdate) {
    current.base_url = update.base_url;
    current.model = update.model;
    current.lang_a = update.lang_a;
    current.lang_b = update.lang_b;
    current.ui_lang = update.ui_lang;
    if let Some(appearance) = update.appearance {
        current.appearance = appearance;
    }
    if let Some(api_key) = update.api_key {
        current.api_key = api_key;
    }
}

#[derive(Default)]
pub struct ConfigState {
    pub current: Config,
    pub load_error: Option<String>,
}

impl ConfigState {
    pub fn new(loaded: Result<Config, String>) -> Self {
        match loaded {
            Ok(current) => Self {
                current,
                load_error: None,
            },
            Err(error) => Self {
                current: Config::default(),
                load_error: Some(error),
            },
        }
    }

    pub fn view(&self) -> ConfigView {
        let mut view = ConfigView::from(&self.current);
        view.load_error = self.load_error.clone();
        view
    }

    pub fn save(
        &mut self,
        path: &Path,
        allow_recovery: bool,
        update: impl FnOnce(&mut Config),
    ) -> Result<(), String> {
        if !allow_recovery {
            if let Some(error) = &self.load_error {
                return Err(error.clone());
            }
        }
        let mut next = self.current.clone();
        update(&mut next);
        write_config_to_path(path, &next)?;
        self.current = next;
        self.load_error = None;
        Ok(())
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
            appearance: Appearance::default(),
        }
    }
}

pub fn read_config_from_path(path: &Path) -> Result<Config, String> {
    match fs::read_to_string(path) {
        Ok(json) => serde_json::from_str(&json)
            .map_err(|e| format!("配置文件无法解析，请在设置中检查并保存完整配置：{e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(format!("读取配置失败，请在设置中检查并保存完整配置：{e}")),
    }
}

pub fn write_config_to_path(path: &Path, config: &Config) -> Result<(), String> {
    let dir = path.parent().ok_or("配置文件缺少父目录")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;

    #[cfg(unix)]
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;

    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    // Preserve the old config until the same-directory replacement is ready.
    let mut file = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    file.as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;

    file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.error.to_string())?;

    Ok(())
}
