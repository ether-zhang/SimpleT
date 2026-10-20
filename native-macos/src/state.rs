use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Page {
    #[default]
    Translate,
    History,
    Settings,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TranslationTicket {
    pub id: u64,
    revision: u64,
    pub source: String,
    pub lang_a: String,
    pub lang_b: String,
}

#[derive(Default)]
pub struct Session {
    pub page: Page,
    pub input: String,
    pub output: String,
    pub lang_a: String,
    pub lang_b: String,
    pub busy: bool,
    pub visible: bool,
    pub closing: bool,
    pub generation: u64,
    revision: u64,
    request_id: u64,
}

impl Session {
    pub fn new(lang_a: String, lang_b: String) -> Self {
        Self {
            lang_a,
            lang_b,
            ..Self::default()
        }
    }
    pub fn edit_input(&mut self, input: String) {
        if self.input != input {
            self.revision = self.revision.wrapping_add(1);
            self.input = input;
        }
    }

    pub fn clear_output(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.output.clear();
    }

    pub fn swap(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        std::mem::swap(&mut self.input, &mut self.output);
        std::mem::swap(&mut self.lang_a, &mut self.lang_b);
    }

    pub fn begin_translation(&mut self) -> Option<TranslationTicket> {
        if self.busy {
            return None;
        }
        if self.input.trim().is_empty() {
            self.clear_output();
            return None;
        }
        self.busy = true;
        self.request_id = self.request_id.wrapping_add(1);
        Some(TranslationTicket {
            id: self.request_id,
            revision: self.revision,
            source: self.input.clone(),
            lang_a: self.lang_a.clone(),
            lang_b: self.lang_b.clone(),
        })
    }

    pub fn finish_translation(&mut self, ticket: &TranslationTicket) -> bool {
        if ticket.id != self.request_id {
            return false;
        }
        self.busy = false;
        ticket.revision == self.revision
            && ticket.source == self.input
            && ticket.lang_a == self.lang_a
            && ticket.lang_b == self.lang_b
    }

    pub fn show(&mut self, page: Page) {
        self.page = page;
        self.generation = self.generation.wrapping_add(1);
        self.visible = true;
        self.closing = false;
    }

    pub fn request_hide(&mut self) -> Option<u64> {
        if !self.visible || self.closing {
            return None;
        }
        self.closing = true;
        Some(self.generation)
    }

    pub fn commit_hide(&mut self, generation: u64) -> bool {
        if self.generation != generation || !self.closing {
            return false;
        }
        self.visible = false;
        self.closing = false;
        true
    }
}

#[derive(Default)]
pub struct Catalog {
    pub groups: BTreeMap<String, Vec<String>>,
    pub query: String,
    pub provider: String,
    pub selected: String,
}

impl Catalog {
    pub fn replace(&mut self, models: Vec<String>, saved: &str) {
        self.groups.clear();
        for model in models {
            let provider = model.split_once('/').map_or("", |pair| pair.0).to_owned();
            self.groups.entry(provider).or_default().push(model);
        }
        self.selected = self
            .groups
            .values()
            .flatten()
            .find(|model| *model == saved)
            .cloned()
            .unwrap_or_default();
        self.provider = self
            .selected
            .split_once('/')
            .map_or("", |pair| pair.0)
            .to_owned();
        self.query.clear();
        if !self.groups.contains_key(&self.provider) {
            self.provider = self.groups.keys().next().cloned().unwrap_or_default();
        }
    }

    pub fn providers(&self) -> Vec<String> {
        self.groups
            .keys()
            .filter(|provider| !self.models(provider).is_empty())
            .cloned()
            .collect()
    }

    pub fn models(&self, provider: &str) -> Vec<String> {
        let query = self.query.trim().to_lowercase();
        self.groups
            .get(provider)
            .into_iter()
            .flatten()
            .filter(|model| query.is_empty() || model.to_lowercase().contains(&query))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session {
            input: "hello".into(),
            lang_a: "English".into(),
            lang_b: "Chinese".into(),
            ..Session::default()
        }
    }

    #[test]
    fn clearing_and_retyping_the_same_source_rejects_an_old_result() {
        let mut session = session();
        let ticket = session.begin_translation().unwrap();
        session.edit_input(String::new());
        session.edit_input("hello".into());
        assert!(!session.finish_translation(&ticket));
        assert!(!session.busy);
    }

    #[test]
    fn swapping_or_clearing_output_preserves_the_newer_user_choice() {
        let mut session = session();
        let ticket = session.begin_translation().unwrap();
        session.swap();
        assert!(!session.finish_translation(&ticket));
        session.edit_input("again".into());
        let ticket = session.begin_translation().unwrap();
        session.clear_output();
        assert!(!session.finish_translation(&ticket));
    }

    #[test]
    fn only_one_translation_runs_and_empty_input_does_not_start_a_request() {
        let mut session = session();
        let ticket = session.begin_translation().unwrap();
        assert!(session.begin_translation().is_none());
        assert!(session.finish_translation(&ticket));
        session.edit_input(" \n".into());
        assert!(session.begin_translation().is_none());
        assert!(!session.busy);
    }

    #[test]
    fn stale_closing_callbacks_cannot_hide_a_reopened_panel() {
        let mut session = session();
        session.show(Page::Translate);
        let closing = session.request_hide().unwrap();
        session.show(Page::Settings);
        assert!(!session.commit_hide(closing));
        assert!(session.visible);
        assert_eq!(session.page, Page::Settings);
    }

    #[test]
    fn searching_large_catalogs_keeps_exact_ids_and_browsing_does_not_commit() {
        let mut catalog = Catalog::default();
        let models = (0..1000)
            .map(|index| format!("vendor{}/nested/model-{index}", index / 50))
            .collect();
        catalog.replace(models, "vendor3/nested/model-173");
        assert_eq!(catalog.groups.len(), 20);
        catalog.query = "MODEL-999".into();
        assert_eq!(catalog.providers(), ["vendor19"]);
        assert_eq!(catalog.models("vendor19"), ["vendor19/nested/model-999"]);
        assert_eq!(catalog.selected, "vendor3/nested/model-173");
    }

    #[test]
    fn absent_saved_models_require_explicit_replacement() {
        let mut catalog = Catalog::default();
        catalog.replace(
            vec!["plain-model".into(), "provider/model".into()],
            "removed-model",
        );
        assert!(catalog.selected.is_empty());
        assert_eq!(catalog.groups[""], ["plain-model"]);
    }
}
