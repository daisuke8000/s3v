use std::collections::HashSet;

use super::{App, Mode};

impl App {
    pub(crate) fn enter_filter_mode(mut self) -> Self {
        let all_items = if self.all_items.is_empty() {
            std::mem::take(&mut self.items)
        } else {
            self.all_items
        };
        Self {
            mode: Mode::Filter,
            all_items,
            filter: String::new(),
            ..self
        }
    }

    pub(crate) fn apply_filter(self) -> Self {
        let filtered = if self.filter.is_empty() {
            self.all_items
        } else {
            let escaped = regex::escape(&self.filter);
            let pattern = escaped.replace(r"\*", ".*");
            let re = regex::Regex::new(&format!("(?i){}", pattern)).ok();
            self.all_items
                .iter()
                .filter(|item| re.as_ref().is_none_or(|r| r.is_match(item.name())))
                .cloned()
                .collect()
        };
        Self {
            items: filtered,
            cursor: 0,
            mode: Mode::Normal,
            selected: HashSet::new(),
            all_items: Vec::new(),
            ..self
        }
    }

    pub(crate) fn clear_filter(self) -> Self {
        let items = if self.all_items.is_empty() {
            self.items
        } else {
            self.all_items
        };
        Self {
            items,
            filter: String::new(),
            cursor: 0,
            mode: Mode::Normal,
            selected: HashSet::new(),
            all_items: Vec::new(),
            ..self
        }
    }
}
