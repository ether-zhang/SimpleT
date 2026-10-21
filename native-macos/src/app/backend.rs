use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use dispatch2::{DispatchQueue, DispatchRetained};
use simplet_core::{
    config::{
        apply_config_update, read_config_from_path, Appearance, Config, ConfigState, ConfigUpdate,
    },
    history::HistoryStore,
};

use super::{post, Action};
use crate::state::TranslationTicket;

pub enum Change {
    Full(ConfigUpdate),
    Appearance(Appearance),
    Locale(String),
    Languages(String, String),
}

pub struct Backend {
    pub config: Mutex<ConfigState>,
    history: Mutex<HistoryStore>,
    pub queue: DispatchRetained<DispatchQueue>,
    pub runtime: tokio::runtime::Runtime,
    pub config_path: PathBuf,
    pub fixture: bool,
}

impl Backend {
    pub fn new() -> Result<Arc<Self>, String> {
        let args: Vec<_> = std::env::args_os().collect();
        let fixture = args.iter().any(|arg| arg == "--fixture");
        let support = PathBuf::from(objc2_foundation::NSHomeDirectory().to_string())
            .join("Library/Application Support");
        let project = Path::new(env!("CARGO_MANIFEST_DIR"));
        let directory = data_directory(&args, &support, project, fixture);
        let mut sources = Vec::new();
        if !fixture && !args.iter().any(|arg| arg == "--data-dir") {
            // Preserve this machine's earlier preview profile when moving the
            // app out of the checkout. Other Macs do not need that source path.
            sources.push(project.join("target/preview-data"));
        }
        sources.push(support.join("com.etherzhang.simplet"));
        prepare_directory(&directory, fixture, &sources)?;
        let config_path = directory.join("config.json");
        let state = ConfigState::new(read_config_from_path(&config_path));
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Arc::new(Self {
            config: Mutex::new(state),
            history: Mutex::new(HistoryStore::new(Ok(directory.join("history.json")))),
            queue: DispatchQueue::new("com.etherzhang.simplet.native.persistence", None),
            runtime,
            config_path,
            fixture,
        }))
    }

    pub fn snapshot(&self) -> Config {
        self.config.lock().unwrap().current.clone()
    }

    pub fn save(self: &Arc<Self>, change: Change, version: u64, key_revision: u64) {
        let backend = self.clone();
        self.queue.exec_async(move || {
            let full = matches!(&change, Change::Full(_));
            let result = backend
                .config
                .lock()
                .map_err(|_| "配置状态不可用".to_owned())
                .and_then(|mut state| {
                    state.save(&backend.config_path, full, |config| match change {
                        Change::Full(update) => apply_config_update(config, update),
                        Change::Appearance(appearance) => config.appearance = appearance,
                        Change::Locale(locale) => config.ui_lang = locale,
                        Change::Languages(a, b) => {
                            config.lang_a = a;
                            config.lang_b = b;
                        }
                    })?;
                    Ok(state.current.clone())
                });
            post(Action::Saved {
                version,
                key_revision,
                full,
                result,
            });
        });
    }

    pub fn translate(self: &Arc<Self>, ticket: TranslationTicket) {
        let backend = self.clone();
        let config = self.snapshot();
        self.runtime.spawn(async move {
            let result = simplet_core::translation::translate(
                &config,
                &ticket.source,
                &ticket.lang_a,
                &ticket.lang_b,
            )
            .await;
            match result {
                Ok(translation) => {
                    let persistence = backend.clone();
                    backend.queue.exec_async(move || {
                        let saved = persistence
                            .history
                            .lock()
                            .map_err(|_| "历史记录状态不可用".to_string())
                            .and_then(|mut store| {
                                store.append(
                                    ticket.source.clone(),
                                    translation.clone(),
                                    ticket.lang_a.clone(),
                                    ticket.lang_b.clone(),
                                )
                            });
                        post(Action::Translated {
                            ticket,
                            result: Ok(translation),
                            saved: Some(saved),
                        });
                    });
                }
                Err(error) => post(Action::Translated {
                    ticket,
                    result: Err(error),
                    saved: None,
                }),
            }
        });
    }

    pub fn models(self: &Arc<Self>, version: u64, url: String, draft_key: Option<String>) {
        let saved = self.snapshot();
        self.runtime.spawn(async move {
            let result = simplet_core::models::list_models(
                &url,
                draft_key.as_deref(),
                &saved.base_url,
                &saved.api_key,
            )
            .await;
            post(Action::Models { version, result });
        });
    }

    pub fn history(self: &Arc<Self>, version: u64, before: Option<u64>) {
        let backend = self.clone();
        self.queue.exec_async(move || {
            let result = backend
                .history
                .lock()
                .map_err(|_| "历史记录状态不可用".to_owned())
                .and_then(|mut history| history.page(before));
            post(Action::History {
                version,
                older: before.is_some(),
                result,
            });
        });
    }
}

fn data_directory(args: &[OsString], support: &Path, project: &Path, fixture: bool) -> PathBuf {
    args.windows(2)
        .find(|pair| pair[0] == "--data-dir")
        .map(|pair| PathBuf::from(&pair[1]))
        .unwrap_or_else(|| {
            if fixture {
                project.join("target/fixture-data")
            } else {
                support.join("com.etherzhang.simplet.native-preview")
            }
        })
}

fn prepare_directory(directory: &Path, fixture: bool, sources: &[PathBuf]) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())?;
    if fixture {
        let config_path = directory.join("config.json");
        if !config_path.exists() {
            simplet_core::config::write_config_to_path(&config_path, &Config::default())?;
        }
        let history_path = directory.join("history.json");
        if !history_path.exists() {
            let entries:Vec<_>=(1..=600).map(|id|simplet_core::history::HistoryEntry {
                id,created_at:1_760_000_000_000+id*60_000,source_text:format!("原生界面测试 {id}\n这是一段用于核对文本布局、滚动和内存占用的示例。"),
                translation:format!("Native preview {id}\nA sample for checking text layout, scrolling and memory."),lang_a:"Chinese".into(),lang_b:"English".into(),
            }).collect();
            fs::write(
                &history_path,
                serde_json::to_vec(&serde_json::json!({"version":1,"entries":entries})).unwrap(),
            )
            .map_err(|error| error.to_string())?;
            fs::set_permissions(history_path, fs::Permissions::from_mode(0o600))
                .map_err(|error| error.to_string())?;
        }
        return Ok(());
    }
    // Work on a one-time private copy. Two running frontends must never race to
    // replace the same JSON files while the native implementation is evaluated.
    for name in ["config.json", "history.json"] {
        let destination = directory.join(name);
        if !destination.exists() {
            if let Some(original) = sources
                .iter()
                .map(|source| source.join(name))
                .find(|path| path.is_file())
            {
                fs::copy(original, &destination).map_err(|error| error.to_string())?;
                fs::set_permissions(destination, fs::Permissions::from_mode(0o600))
                    .map_err(|error| error.to_string())?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn installed_profile_is_independent_of_the_build_checkout() {
        let support = Path::new("/Users/test/Library/Application Support");
        let expected = support.join("com.etherzhang.simplet.native-preview");
        for project in ["/build/machine/checkout", "/nonexistent/source/tree"] {
            assert_eq!(
                data_directory(&[], support, Path::new(project), false),
                expected
            );
        }
        assert_eq!(
            data_directory(&[], support, Path::new("/checkout"), true),
            Path::new("/checkout/target/fixture-data")
        );
    }

    #[test]
    fn explicit_data_directory_still_overrides_both_modes() {
        let args = vec![
            "simplet-native".into(),
            "--data-dir".into(),
            "/test/custom profile".into(),
        ];
        for fixture in [false, true] {
            assert_eq!(
                data_directory(
                    &args,
                    Path::new("/support"),
                    Path::new("/checkout"),
                    fixture
                ),
                Path::new("/test/custom profile")
            );
        }
    }

    #[test]
    fn migration_copies_missing_files_once_and_preserves_both_sources() {
        let root = tempfile::tempdir().unwrap();
        let legacy = root.path().join("legacy");
        let original = root.path().join("original");
        let installed = root.path().join("installed");
        fs::create_dir(&legacy).unwrap();
        fs::create_dir(&original).unwrap();
        fs::write(legacy.join("config.json"), b"preview config").unwrap();
        fs::write(original.join("config.json"), b"original config").unwrap();
        fs::write(original.join("history.json"), b"original history").unwrap();
        let sources = vec![legacy.clone(), original.clone()];
        prepare_directory(&installed, false, &sources).unwrap();
        assert_eq!(
            fs::read(installed.join("config.json")).unwrap(),
            b"preview config"
        );
        assert_eq!(
            fs::read(installed.join("history.json")).unwrap(),
            b"original history"
        );
        fs::write(installed.join("config.json"), b"installed edits").unwrap();
        prepare_directory(&installed, false, &sources).unwrap();
        assert_eq!(
            fs::read(installed.join("config.json")).unwrap(),
            b"installed edits"
        );
        assert_eq!(
            fs::read(legacy.join("config.json")).unwrap(),
            b"preview config"
        );
        assert_eq!(
            fs::read(original.join("config.json")).unwrap(),
            b"original config"
        );
        assert_eq!(
            fs::metadata(&installed).unwrap().permissions().mode() & 0o777,
            0o700
        );
        for name in ["config.json", "history.json"] {
            assert_eq!(
                fs::metadata(installed.join(name))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}
