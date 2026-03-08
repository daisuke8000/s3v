use crate::command::Command;
use crate::s3::S3Item;

use super::App;

impl App {
    pub(crate) fn toggle_selection(mut self) -> Self {
        if self.selected.contains(&self.cursor) {
            self.selected.remove(&self.cursor);
        } else {
            self.selected.insert(self.cursor);
        }
        self
    }

    pub(crate) fn toggle_select_all(mut self) -> Self {
        if self.selected.len() == self.items.len() {
            self.selected.clear();
        } else {
            self.selected = (0..self.items.len()).collect();
        }
        self
    }

    pub fn selected_items(&self) -> Vec<&S3Item> {
        self.selected
            .iter()
            .filter_map(|&i| self.items.get(i))
            .collect()
    }

    pub(crate) fn start_download(self) -> (Self, Vec<Command>) {
        let bucket = match &self.current_path.bucket {
            Some(b) => b.clone(),
            None => return (self, vec![]),
        };
        let key = self.selected_item().and_then(|item| match item {
            S3Item::File { key, .. } => Some(key.clone()),
            _ => None,
        });
        let key = match key {
            Some(k) => k,
            None => return (self, vec![]),
        };
        let destination = dirs::download_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        (
            self,
            vec![Command::Download {
                bucket,
                key,
                destination,
            }],
        )
    }
}
