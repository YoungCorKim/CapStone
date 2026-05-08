//! Chat UI: cooperative cancel via Rig [`PromptHook`] and progress events to the webview.

use rig::{
    agent::{HookAction, PromptHook, ToolCallHookAction},
    completion::{CompletionModel, Message},
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

pub type SharedChatCancel = Arc<AtomicBool>;

pub fn tool_status_message(tool_name: &str) -> &'static str {
    match tool_name {
        "summary_agent" => "Summarizing…",
        "vault_agent" => "Vault lookup…",
        "markdown_agent" => "Drafting markdown proposal…",
        "development_coach_agent" => "Coaching: next steps & exercises…",
        "find_duplicates" => "Scanning for duplicate files…",
        "find_related_files" => "Finding related files…",
        "propose_create_markdown" => "Queueing new file proposal…",
        "propose_edit_markdown" => "Queueing edit proposal…",
        _ => "Running tool…",
    }
}

#[derive(Clone)]
pub struct ChatUiHook {
    pub app: AppHandle,
    pub cancel: SharedChatCancel,
}

impl<M: CompletionModel> PromptHook<M> for ChatUiHook {
    fn on_completion_call(
        &self,
        _prompt: &Message,
        _history: &[Message],
    ) -> impl std::future::Future<Output = HookAction> + Send {
        let cancel = self.cancel.clone();
        let app = self.app.clone();
        async move {
            if cancel.load(Ordering::SeqCst) {
                return HookAction::terminate("User stopped the request");
            }
            let _ = app.emit(
                "chat-progress",
                serde_json::json!({
                    "phase": "completion",
                    "message": "Calling model…",
                }),
            );
            HookAction::cont()
        }
    }

    fn on_tool_call(
        &self,
        tool_name: &str,
        _tool_call_id: Option<String>,
        _internal_call_id: &str,
        _args: &str,
    ) -> impl std::future::Future<Output = ToolCallHookAction> + Send {
        let cancel = self.cancel.clone();
        let app = self.app.clone();
        let tool_name = tool_name.to_string();
        async move {
            if cancel.load(Ordering::SeqCst) {
                return ToolCallHookAction::terminate("User stopped the request");
            }
            let msg = tool_status_message(&tool_name);
            let _ = app.emit(
                "chat-progress",
                serde_json::json!({
                    "phase": "tool",
                    "tool": tool_name,
                    "message": msg,
                }),
            );
            ToolCallHookAction::cont()
        }
    }
}
