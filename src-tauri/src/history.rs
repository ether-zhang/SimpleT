use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const PAGE_SIZE: usize = 30;
const MAX_HISTORY_ID: u64 = 9_007_199_254_740_991;
const MAX_DATE_MILLIS: u64 = 8_640_000_000_000_000;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct HistoryEntry {
    pub id: u64,
    pub created_at: u64,
    pub source_text: String,
    pub translation: String,
    pub lang_a: String,
    pub lang_b: String,
}

#[derive(Serialize)]
pub struct HistoryPage {
    pub entries: Vec<HistoryEntry>,
    pub has_more: bool,
    pub save_error: Option<String>,
}

#[derive(Deserialize)]
struct HistoryFile {
    version: u32,
    entries: Vec<HistoryEntry>,
}

#[derive(Serialize)]
struct HistoryFileRef<'a> {
    version: u32,
    entries: &'a [HistoryEntry],
}

pub struct HistoryStore {
    path: Result<PathBuf, String>,
    entries: Option<Vec<HistoryEntry>>,
    save_error: Option<String>,
}

impl HistoryStore {
    pub fn new(path: Result<PathBuf, String>) -> Self {
        Self {
            path,
            entries: None,
            save_error: None,
        }
    }

    fn load(&mut self) -> Result<(), String> {
        if self.entries.is_some() {
            return Ok(());
        }
        let path = self.path.as_ref().map_err(Clone::clone)?;
        let entries =
            match fs::read(path) {
                Ok(bytes) => {
                    let file: HistoryFile = serde_json::from_slice(&bytes)
                        .map_err(|error| format!("历史记录无法解析：{error}"))?;
                    if file.version != 1 {
                        return Err("历史记录版本不受支持".into());
                    }
                    if file.entries.first().is_some_and(|entry| entry.id == 0)
                        || file.entries.windows(2).any(|pair| pair[0].id >= pair[1].id)
                    {
                        return Err("历史记录顺序异常，原文件已保留".into());
                    }
                    if file.entries.iter().any(|entry| {
                        entry.id > MAX_HISTORY_ID || entry.created_at > MAX_DATE_MILLIS
                    }) {
                        return Err("历史记录编号或时间异常，原文件已保留".into());
                    }
                    file.entries
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
                Err(error) => return Err(format!("读取历史记录失败：{error}")),
            };
        self.entries = Some(entries);
        Ok(())
    }

    pub fn page(&mut self, before: Option<u64>) -> Result<HistoryPage, String> {
        self.load()?;
        let entries = self.entries.as_ref().unwrap();
        let end = before.map_or(entries.len(), |id| {
            entries.partition_point(|entry| entry.id < id)
        });
        let start = end.saturating_sub(PAGE_SIZE);
        Ok(HistoryPage {
            entries: entries[start..end].to_vec(),
            has_more: start > 0,
            save_error: self.save_error.clone(),
        })
    }

    pub fn append(
        &mut self,
        source_text: String,
        translation: String,
        lang_a: String,
        lang_b: String,
    ) -> Result<HistoryEntry, String> {
        let result = self.append_inner(source_text, translation, lang_a, lang_b);
        self.save_error = result.as_ref().err().cloned();
        result
    }

    fn append_inner(
        &mut self,
        source_text: String,
        translation: String,
        lang_a: String,
        lang_b: String,
    ) -> Result<HistoryEntry, String> {
        self.load()?;
        let entries = self.entries.as_mut().unwrap();
        let id = entries
            .last()
            .map_or(Some(1), |entry| entry.id.checked_add(1))
            .filter(|id| *id <= MAX_HISTORY_ID)
            .ok_or("历史记录编号已用尽")?;
        let entry = HistoryEntry {
            id,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| format!("无法读取系统时间：{error}"))?
                .as_millis()
                .try_into()
                .map_err(|_| "系统时间超出范围")?,
            source_text,
            translation,
            lang_a,
            lang_b,
        };
        entries.push(entry.clone());
        if let Err(error) = write_history(self.path.as_ref().map_err(Clone::clone)?, entries) {
            entries.pop();
            return Err(format!("保存历史记录失败：{error}"));
        }
        Ok(entry)
    }
}

fn write_history(path: &Path, entries: &[HistoryEntry]) -> Result<(), String> {
    let dir = path.parent().ok_or("历史记录文件缺少父目录")?;
    fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())?;

    // Replace atomically so an interrupted write leaves the previous history intact.
    let mut file = tempfile::NamedTempFile::new_in(dir).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    file.as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())?;
    serde_json::to_writer(
        &mut file,
        &HistoryFileRef {
            version: 1,
            entries,
        },
    )
    .map_err(|error| error.to_string())?;
    file.flush().map_err(|error| error.to_string())?;
    file.as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    file.persist(path)
        .map_err(|error| error.error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn append(store: &mut HistoryStore) -> HistoryEntry {
        store
            .append(
                "你好\n世界".into(),
                "Hello\nworld".into(),
                "Chinese".into(),
                "English".into(),
            )
            .unwrap()
    }

    #[test]
    fn records_survive_reopening_with_exact_text_and_languages() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data/history.json");
        let mut store = HistoryStore::new(Ok(path.clone()));
        assert!(store.page(None).unwrap().entries.is_empty());
        let first = append(&mut store);
        let second = append(&mut store);
        let mut reopened = HistoryStore::new(Ok(path));
        assert_eq!(reopened.page(None).unwrap().entries, vec![first, second]);
        assert_eq!(append(&mut reopened).id, 3);
    }

    #[test]
    fn older_pages_stay_in_order_when_new_records_arrive() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let mut store = HistoryStore::new(Ok(path));
        for _ in 0..65 {
            append(&mut store);
        }
        let latest = store.page(None).unwrap();
        assert!(latest.has_more);
        assert_eq!(latest.entries.first().unwrap().id, 36);
        assert_eq!(latest.entries.last().unwrap().id, 65);
        append(&mut store);
        let older = store.page(Some(36)).unwrap();
        assert_eq!(older.entries.first().unwrap().id, 6);
        assert_eq!(older.entries.last().unwrap().id, 35);
        let oldest = store.page(Some(6)).unwrap();
        assert_eq!(oldest.entries.len(), 5);
        assert!(!oldest.has_more);
    }

    #[test]
    fn damaged_or_unsupported_history_is_preserved_and_can_be_retried() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        for original in ["broken", r#"{"version":2,"entries":[]}"#] {
            fs::write(&path, original).unwrap();
            let mut store = HistoryStore::new(Ok(path.clone()));
            assert!(store.page(None).is_err());
            assert!(store
                .append("a".into(), "b".into(), "English".into(), "Chinese".into())
                .is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), original);
            fs::remove_file(&path).unwrap();
            assert!(store.page(None).unwrap().entries.is_empty());
        }
    }

    #[test]
    fn a_failed_save_does_not_create_a_phantom_record() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let mut store = HistoryStore::new(Ok(path.clone()));
        let first = append(&mut store);
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(store
            .append("a".into(), "b".into(), "English".into(), "Chinese".into())
            .is_err());
        let page = store.page(None).unwrap();
        assert_eq!(page.entries, vec![first]);
        assert!(page.save_error.is_some());
        fs::remove_dir(&path).unwrap();
        assert_eq!(append(&mut store).id, 2);
        assert!(store.page(None).unwrap().save_error.is_none());
    }

    #[test]
    fn invalid_order_and_dates_cannot_break_frontend_history_rendering() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let mut store = HistoryStore::new(Ok(path.clone()));
        let first = append(&mut store);
        let second = append(&mut store);
        for entries in [
            vec![second, first.clone()],
            vec![HistoryEntry {
                id: 0,
                ..first.clone()
            }],
            vec![HistoryEntry {
                id: MAX_HISTORY_ID + 1,
                ..first.clone()
            }],
            vec![HistoryEntry {
                created_at: MAX_DATE_MILLIS + 1,
                ..first
            }],
        ] {
            write_history(&path, &entries).unwrap();
            let original = fs::read(&path).unwrap();
            assert!(HistoryStore::new(Ok(path.clone())).page(None).is_err());
            assert_eq!(fs::read(&path).unwrap(), original);
        }
    }

    #[cfg(unix)]
    #[test]
    fn history_file_is_only_readable_by_its_owner() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data/history.json");
        append(&mut HistoryStore::new(Ok(path.clone())));
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
}
