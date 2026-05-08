//! In-memory version history snapshots (lost on app restart).

use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub type SharedVersionHistoryStore = Arc<Mutex<VersionHistoryInner>>;

pub fn new_shared_store() -> SharedVersionHistoryStore {
    Arc::new(Mutex::new(VersionHistoryInner::default()))
}

#[derive(Debug, Clone)]
pub enum VersionSource {
    ProposalCreate,
    ProposalEdit,
}

impl VersionSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            VersionSource::ProposalCreate => "proposal_create",
            VersionSource::ProposalEdit => "proposal_edit",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VersionHistoryEntry {
    pub id: String,
    pub created_at_rfc3339: String,
    pub relative_path: String,
    pub absolute_path: String,
    pub before_content: String,
    pub after_content: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
}

pub struct VersionHistoryInner {
    /// Insertion order (oldest first).
    order: Vec<String>,
    by_id: HashMap<String, VersionHistoryEntry>,
    by_abs_path: HashMap<String, Vec<String>>,
    by_batch: HashMap<String, Vec<String>>,
}

impl Default for VersionHistoryInner {
    fn default() -> Self {
        Self {
            order: Vec::new(),
            by_id: HashMap::new(),
            by_abs_path: HashMap::new(),
            by_batch: HashMap::new(),
        }
    }
}

impl VersionHistoryInner {
    pub fn add_entry(
        &mut self,
        relative_path: String,
        absolute_path: String,
        before_content: String,
        after_content: String,
        source: VersionSource,
        batch_id: Option<String>,
    ) -> VersionHistoryEntry {
        let id = Uuid::new_v4().to_string();
        let created_at_rfc3339 = chrono::Utc::now().to_rfc3339();
        let entry = VersionHistoryEntry {
            id: id.clone(),
            created_at_rfc3339,
            relative_path,
            absolute_path: absolute_path.clone(),
            before_content,
            after_content,
            source: source.as_str().to_string(),
            batch_id: batch_id.clone(),
        };

        self.order.push(id.clone());
        self.by_id.insert(id.clone(), entry.clone());
        self.by_abs_path.entry(absolute_path).or_default().push(id.clone());
        if let Some(bid) = batch_id {
            self.by_batch.entry(bid).or_default().push(id);
        }

        entry
    }

    /// Drop a snapshot and update indexes (`order`, `by_abs_path`, `by_batch`).
    pub fn remove_entry_by_id(&mut self, id: &str) -> Result<(), String> {
        let entry = self
            .by_id
            .remove(id)
            .ok_or_else(|| "Version entry was already removed.".to_string())?;

        self.order.retain(|x| x != id);

        if let Some(ids) = self.by_abs_path.get_mut(&entry.absolute_path) {
            ids.retain(|x| x != id);
            if ids.is_empty() {
                self.by_abs_path.remove(&entry.absolute_path);
            }
        }

        if let Some(bid) = &entry.batch_id {
            if let Some(ids) = self.by_batch.get_mut(bid) {
                ids.retain(|x| x != id);
                if ids.is_empty() {
                    self.by_batch.remove(bid);
                }
            }
        }

        Ok(())
    }

    pub fn list_entries(
        &self,
        file_path_filter: Option<&str>,
        batch_id_filter: Option<&str>,
    ) -> Vec<VersionHistoryEntry> {
        match (file_path_filter, batch_id_filter) {
            (_, Some(batch)) => self
                .by_batch
                .get(batch)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|id| self.by_id.get(&id).cloned())
                .collect(),
            (Some(abs), None) => self
                .by_abs_path
                .get(abs)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|id| self.by_id.get(&id).cloned())
                .collect(),
            (None, None) => self
                .order
                .iter()
                .filter_map(|id| self.by_id.get(id).cloned())
                .collect(),
        }
    }

    pub fn get_entry(&self, id: &str) -> Option<VersionHistoryEntry> {
        self.by_id.get(id).cloned()
    }

    /// Entry ids for a batch, in the order they were recorded.
    pub fn batch_ids_in_order(&self, batch_id: &str) -> Vec<String> {
        self.by_batch
            .get(batch_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn clear_all(&mut self) {
        *self = Self::default();
    }
}
