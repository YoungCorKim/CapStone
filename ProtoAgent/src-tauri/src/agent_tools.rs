//! RIG agent tools for find_duplicates and find_related_files.
//! When the AI calls these tools, they run the scan and emit a scan-result event to the frontend.

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use std::fmt;
use tauri::{AppHandle, Emitter};

use crate::{get_duplicate_pairs_impl, get_related_pairs_impl};

#[derive(Debug)]
pub struct ScanToolError(pub String);

impl fmt::Display for ScanToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ScanToolError {}

#[derive(Debug, Serialize)]
pub struct ScanToolOutput {
    pub message: String,
    pub pair_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct FindDuplicatesArgs {
    pub vault_path: String,
}

#[derive(Debug, Deserialize)]
pub struct FindRelatedArgs {
    pub vault_path: String,
}

pub struct FindDuplicatesTool {
    pub app: AppHandle,
}

impl Tool for FindDuplicatesTool {
    const NAME: &'static str = "find_duplicates";

    type Error = ScanToolError;
    type Args = FindDuplicatesArgs;
    type Output = ScanToolOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Scan a vault folder for duplicate markdown files (near-identical content). Results are shown in the app's scan panel.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "vault_path": {
                        "type": "string",
                        "description": "Absolute path to the vault folder to scan"
                    }
                },
                "required": ["vault_path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let pairs = get_duplicate_pairs_impl(args.vault_path.clone())
            .await
            .map_err(|e| ScanToolError(e))?;
        let count = pairs.len();
        let payload = serde_json::json!({
            "type": "duplicates",
            "pairs": pairs
        });
        self.app
            .emit("scan-result", payload)
            .map_err(|e| ScanToolError(e.to_string()))?;
        Ok(ScanToolOutput {
            message: format!("Found {} duplicate pair(s). Results shown in the scan panel.", count),
            pair_count: count,
        })
    }
}

pub struct FindRelatedFilesTool {
    pub app: AppHandle,
}

impl Tool for FindRelatedFilesTool {
    const NAME: &'static str = "find_related_files";

    type Error = ScanToolError;
    type Args = FindRelatedArgs;
    type Output = ScanToolOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Scan a vault folder for semantically related markdown files. Results are shown in the app's scan panel where the user can add wiki links between them.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "vault_path": {
                        "type": "string",
                        "description": "Absolute path to the vault folder to scan"
                    }
                },
                "required": ["vault_path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let pairs = get_related_pairs_impl(args.vault_path.clone())
            .await
            .map_err(|e| ScanToolError(e))?;
        let count = pairs.len();
        let payload = serde_json::json!({
            "type": "related",
            "pairs": pairs
        });
        self.app
            .emit("scan-result", payload)
            .map_err(|e| ScanToolError(e.to_string()))?;
        Ok(ScanToolOutput {
            message: format!("Found {} related file pair(s). Results shown in the scan panel.", count),
            pair_count: count,
        })
    }
}
