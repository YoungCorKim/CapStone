//! Pending markdown create/edit proposals with vault-safe path resolution.

use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

pub type SharedProposalStore = Arc<Mutex<HashMap<String, PendingProposal>>>;

#[derive(Debug, Clone)]
pub enum PendingProposal {
    Create {
        absolute_path: String,
        relative_path: String,
        content: String,
    },
    Edit {
        absolute_path: String,
        relative_path: String,
        previous_content: String,
        new_content: String,
    },
}

#[derive(Debug, Serialize, Clone)]
pub struct MarkdownProposalEvent {
    pub id: String,
    pub kind: String,
    pub relative_path: String,
    pub absolute_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_content: Option<String>,
    pub new_content: String,
}

impl PendingProposal {
    pub fn to_event(&self, id: &str) -> MarkdownProposalEvent {
        match self {
            PendingProposal::Create {
                relative_path,
                absolute_path,
                content,
            } => MarkdownProposalEvent {
                id: id.to_string(),
                kind: "create".to_string(),
                relative_path: relative_path.clone(),
                absolute_path: absolute_path.clone(),
                previous_content: None,
                new_content: content.clone(),
            },
            PendingProposal::Edit {
                relative_path,
                absolute_path,
                previous_content,
                new_content,
            } => MarkdownProposalEvent {
                id: id.to_string(),
                kind: "edit".to_string(),
                relative_path: relative_path.clone(),
                absolute_path: absolute_path.clone(),
                previous_content: Some(previous_content.clone()),
                new_content: new_content.clone(),
            },
        }
    }
}

/// Join `relative_path` to `vault_root`, forbid `..`, require `.md` / `.markdown`.
pub fn resolve_markdown_under_vault(
    vault_root: &Path,
    relative_path: &str,
) -> Result<PathBuf, String> {
    let rel = relative_path.trim().trim_start_matches(['/', '\\']);
    if rel.is_empty() {
        return Err("Path is empty".to_string());
    }

    let vault = fs::canonicalize(vault_root)
        .map_err(|e| format!("Could not resolve vault path: {}", e))?;

    let mut out = vault.clone();
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(part) => {
                if part == std::ffi::OsStr::new("..") {
                    return Err("Path cannot contain parent directory components".to_string());
                }
                out.push(part);
            }
            Component::CurDir => {}
            _ => return Err("Invalid path".to_string()),
        }
    }

    let ext = out
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ext != "md" && ext != "markdown" {
        return Err("Path must end with .md or .markdown".to_string());
    }

    if !out.starts_with(&vault) {
        return Err("Path escapes vault directory".to_string());
    }

    Ok(out)
}

pub fn relative_display(vault: &Path, absolute: &Path) -> String {
    absolute
        .strip_prefix(vault)
        .ok()
        .map(|p| p.to_string_lossy().trim_start_matches(['/', '\\']).to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| absolute.to_string_lossy().to_string())
}

#[derive(Debug, Clone)]
pub struct AppliedProposalSnapshot {
    pub absolute_path: String,
    pub relative_path: String,
    pub before_content: String,
    pub after_content: String,
    pub kind_create: bool,
}

pub fn apply_markdown_proposal_impl(
    store: &SharedProposalStore,
    id: &str,
) -> Result<AppliedProposalSnapshot, String> {
    let proposal = {
        let mut map = store.lock().map_err(|e| e.to_string())?;
        map.remove(id).ok_or_else(|| "Proposal not found".to_string())?
    };

    match proposal {
        PendingProposal::Create {
            absolute_path,
            relative_path,
            content,
            ..
        } => {
            let path = Path::new(&absolute_path);
            if path.exists() {
                return Err(
                    "File already exists; reject this proposal or delete the file first.".to_string(),
                );
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(path, &content).map_err(|e| e.to_string())?;
            Ok(AppliedProposalSnapshot {
                absolute_path,
                relative_path,
                before_content: String::new(),
                after_content: content,
                kind_create: true,
            })
        }
        PendingProposal::Edit {
            absolute_path,
            relative_path,
            previous_content,
            new_content,
            ..
        } => {
            let path = Path::new(&absolute_path);
            if !path.exists() {
                return Err("File no longer exists.".to_string());
            }
            let current = fs::read_to_string(path).map_err(|e| e.to_string())?;
            if current != previous_content {
                return Err(
                    "File changed since this proposal was created; reject it and try again."
                        .to_string(),
                );
            }
            fs::write(path, &new_content).map_err(|e| e.to_string())?;
            Ok(AppliedProposalSnapshot {
                absolute_path,
                relative_path,
                before_content: previous_content,
                after_content: new_content,
                kind_create: false,
            })
        }
    }
}

pub fn reject_markdown_proposal_impl(store: &SharedProposalStore, id: &str) -> Result<(), String> {
    let mut map = store.lock().map_err(|e| e.to_string())?;
    map.remove(id)
        .ok_or_else(|| "Proposal not found".to_string())?;
    Ok(())
}

pub fn list_pending_markdown_proposals_impl(
    store: &SharedProposalStore,
) -> Result<Vec<MarkdownProposalEvent>, String> {
    let map = store.lock().map_err(|e| e.to_string())?;
    let mut out: Vec<MarkdownProposalEvent> = map
        .iter()
        .map(|(id, p)| p.to_event(id.as_str()))
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}
