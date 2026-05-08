//! RIG tools that queue markdown create/edit proposals (no disk write until user applies).

use crate::markdown_proposals::{
    PendingProposal, SharedProposalStore, resolve_markdown_under_vault, relative_display,
};
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

#[derive(Debug)]
pub struct MarkdownToolError(pub String);

impl fmt::Display for MarkdownToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MarkdownToolError {}

#[derive(Debug, Serialize)]
pub struct MarkdownToolOutput {
    pub proposal_id: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct ProposeCreateMarkdownArgs {
    pub relative_path: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ProposeEditMarkdownArgs {
    pub relative_path: String,
    pub new_content: String,
}

pub struct ProposeCreateMarkdownTool {
    pub app: AppHandle,
    pub store: SharedProposalStore,
    pub vault_root: Option<String>,
}

impl Tool for ProposeCreateMarkdownTool {
    const NAME: &'static str = "propose_create_markdown";

    type Error = MarkdownToolError;
    type Args = ProposeCreateMarkdownArgs;
    type Output = MarkdownToolOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Queue a proposal to create a new markdown file under the vault. Nothing is written until the user approves in the app. Use a vault-relative path (e.g. notes/topic.md).".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "relative_path": {
                        "type": "string",
                        "description": "Path relative to vault root, must end with .md or .markdown"
                    },
                    "content": {
                        "type": "string",
                        "description": "Full markdown body for the new file"
                    }
                },
                "required": ["relative_path", "content"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let vault = self
            .vault_root
            .as_ref()
            .ok_or_else(|| MarkdownToolError("No vault is selected. Ask the user to choose a vault folder first.".to_string()))?;

        let vault_path = Path::new(vault);
        let resolved = resolve_markdown_under_vault(vault_path, &args.relative_path)
            .map_err(MarkdownToolError)?;

        if resolved.exists() {
            return Err(MarkdownToolError(format!(
                "File already exists at {}. Use propose_edit_markdown to change it.",
                resolved.display()
            )));
        }

        let rel_display = relative_display(vault_path, &resolved);
        let absolute_path = resolved.to_string_lossy().to_string();

        let proposal = PendingProposal::Create {
            absolute_path: absolute_path.clone(),
            relative_path: rel_display.clone(),
            content: args.content.clone(),
        };

        let id = Uuid::new_v4().to_string();
        let event = proposal.to_event(&id);
        {
            let mut map = self.store.lock().map_err(|e| MarkdownToolError(e.to_string()))?;
            map.insert(id.clone(), proposal);
        }
        self.app
            .emit("markdown-proposal", event)
            .map_err(|e| MarkdownToolError(e.to_string()))?;

        Ok(MarkdownToolOutput {
            proposal_id: id.clone(),
            message: format!(
                "Queued proposal {} to create {}. The file is NOT written until the user approves in the pending proposals panel.",
                id, rel_display
            ),
        })
    }
}

pub struct ProposeEditMarkdownTool {
    pub app: AppHandle,
    pub store: SharedProposalStore,
    pub vault_root: Option<String>,
}

impl Tool for ProposeEditMarkdownTool {
    const NAME: &'static str = "propose_edit_markdown";

    type Error = MarkdownToolError;
    type Args = ProposeEditMarkdownArgs;
    type Output = MarkdownToolOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Queue a proposal to replace the full contents of an existing markdown file. Previous content is snapshotted for review. Nothing is written until the user approves.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "relative_path": {
                        "type": "string",
                        "description": "Path relative to vault root, must end with .md or .markdown"
                    },
                    "new_content": {
                        "type": "string",
                        "description": "Complete new file contents (full replacement)"
                    }
                },
                "required": ["relative_path", "new_content"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let vault = self
            .vault_root
            .as_ref()
            .ok_or_else(|| MarkdownToolError("No vault is selected. Ask the user to choose a vault folder first.".to_string()))?;

        let vault_path = Path::new(vault);
        let resolved = resolve_markdown_under_vault(vault_path, &args.relative_path)
            .map_err(MarkdownToolError)?;

        if !resolved.is_file() {
            return Err(MarkdownToolError(format!(
                "File does not exist at {}. Use propose_create_markdown for new files.",
                resolved.display()
            )));
        }

        let previous_content =
            fs::read_to_string(&resolved).map_err(|e| MarkdownToolError(e.to_string()))?;

        let rel_display = relative_display(vault_path, &resolved);
        let absolute_path = resolved.to_string_lossy().to_string();

        let proposal = PendingProposal::Edit {
            absolute_path: absolute_path.clone(),
            relative_path: rel_display.clone(),
            previous_content,
            new_content: args.new_content.clone(),
        };

        let id = Uuid::new_v4().to_string();
        let event = proposal.to_event(&id);
        {
            let mut map = self.store.lock().map_err(|e| MarkdownToolError(e.to_string()))?;
            map.insert(id.clone(), proposal);
        }
        self.app
            .emit("markdown-proposal", event)
            .map_err(|e| MarkdownToolError(e.to_string()))?;

        Ok(MarkdownToolOutput {
            proposal_id: id.clone(),
            message: format!(
                "Queued proposal {} to edit {}. The file is NOT changed until the user approves in the pending proposals panel.",
                id, rel_display
            ),
        })
    }
}
