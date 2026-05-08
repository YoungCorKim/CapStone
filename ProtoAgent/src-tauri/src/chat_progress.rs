//! Chat UI: cooperative cancel via Rig [`PromptHook`] and progress events to the webview.

use rig::{
    agent::{HookAction, PromptHook, ToolCallHookAction},
    completion::{AssistantContent, CompletionModel, Message},
};
use chrono::Utc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::{fs::OpenOptions, io::Write, path::PathBuf};
use tauri::{AppHandle, Emitter};

use rig::completion::message::UserContent;

pub type SharedChatCancel = Arc<AtomicBool>;

#[derive(Clone)]
pub struct SessionLogger {
    session_id: String,
    inner: Arc<Mutex<SessionLogInner>>,
}

struct SessionLogInner {
    sequence_counter: usize,
    turn_counter: usize,
    initial_prompt: Option<String>,
    readable_events: Vec<String>,
    ndjson_files: Vec<std::fs::File>,
    timeline_files: Vec<std::fs::File>,
    readable_files: Vec<std::fs::File>,
}

impl SessionLogger {
    pub fn new(
        session_id: String,
        ndjson_paths: Vec<PathBuf>,
        timeline_paths: Vec<PathBuf>,
        readable_paths: Vec<PathBuf>,
    ) -> Result<Self, String> {
        let ndjson_files = Self::open_files(ndjson_paths)?;
        let timeline_files = Self::open_files(timeline_paths)?;
        let readable_files = Self::open_files(readable_paths)?;

        Ok(Self {
            session_id,
            inner: Arc::new(Mutex::new(SessionLogInner {
                sequence_counter: 0,
                turn_counter: 0,
                initial_prompt: None,
                readable_events: Vec::new(),
                ndjson_files,
                timeline_files,
                readable_files,
            })),
        })
    }

    pub fn noop(session_id: String) -> Self {
        Self {
            session_id,
            inner: Arc::new(Mutex::new(SessionLogInner {
                sequence_counter: 0,
                turn_counter: 0,
                initial_prompt: None,
                readable_events: Vec::new(),
                ndjson_files: Vec::new(),
                timeline_files: Vec::new(),
                readable_files: Vec::new(),
            })),
        }
    }

    fn open_files(paths: Vec<PathBuf>) -> Result<Vec<std::fs::File>, String> {
        let mut files = Vec::new();
        for path in paths {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
            files.push(file);
        }
        Ok(files)
    }

    fn log_line(&self, mut value: serde_json::Value) {
        let ts = Utc::now().to_rfc3339();
        if let serde_json::Value::Object(ref mut obj) = value {
            obj.entry("session_id".to_string())
                .or_insert(serde_json::Value::String(self.session_id.clone()));
            obj.entry("timestamp".to_string())
                .or_insert(serde_json::Value::String(ts));
        }

        if let Ok(mut inner) = self.inner.lock() {
            for f in &mut inner.ndjson_files {
                let _ = writeln!(f, "{}", value);
                let _ = f.flush();
            }
        }
    }

    fn log_timeline_line(&self, line: &str) {
        if let Ok(mut inner) = self.inner.lock() {
            for f in &mut inner.timeline_files {
                let _ = writeln!(f, "{}", line);
                let _ = f.flush();
            }
        }
    }

    fn append_readable_event(
        &self,
        sequence_index: usize,
        turn: usize,
        line: String,
        graph_hint: Option<String>,
    ) {
        if let Ok(mut inner) = self.inner.lock() {
            inner
                .readable_events
                .push(format!("{sequence_index:03}. [turn {turn}] {line}"));
            if let Some(hint) = graph_hint {
                inner
                    .readable_events
                    .push(format!("      graph_hint: {hint}"));
            }
            let rendered = Self::render_readable(
                &self.session_id,
                inner.initial_prompt.as_deref(),
                &inner.readable_events,
            );
            for f in &mut inner.readable_files {
                let _ = f.set_len(0);
                let _ = f.write_all(rendered.as_bytes());
                let _ = f.flush();
            }
        }
    }

    fn render_readable(
        session_id: &str,
        initial_prompt: Option<&str>,
        events: &[String],
    ) -> String {
        let mut out = String::new();
        out.push_str(&format!("Session: {session_id}\n"));
        out.push_str(&format!("Generated: {}\n\n", Utc::now().to_rfc3339()));
        out.push_str("InitialPrompt:\n");
        out.push_str(initial_prompt.unwrap_or("<none captured yet>"));
        out.push_str("\n\nCallSequence:\n");
        if events.is_empty() {
            out.push_str("  <no events>\n");
        } else {
            for ev in events {
                out.push_str("  ");
                out.push_str(ev);
                out.push('\n');
            }
        }
        out
    }

    pub fn log_prompt(&self, prompt: &str, history_len: usize) {
        let (seq, turn) = if let Ok(mut inner) = self.inner.lock() {
            inner.sequence_counter += 1;
            inner.turn_counter += 1;
            if inner.initial_prompt.is_none() && !prompt.trim().is_empty() {
                inner.initial_prompt = Some(prompt.to_string());
            }
            (inner.sequence_counter, inner.turn_counter)
        } else {
            (0, 0)
        };

        self.log_line(serde_json::json!({
            "phase": "prompt",
            "prompt": prompt,
            "history_len": history_len,
            "sequence_index": seq,
            "turn": turn,
            "event_type": "completion_call",
        }));

        let ts = Utc::now().to_rfc3339();
        let preview = prompt.chars().take(140).collect::<String>();
        self.log_timeline_line(
            format!(
                "[{seq:03}][{ts}][turn {turn}][completion] prompt=\"{}\" history_len={history_len}",
                preview.replace('\n', "\\n")
            )
            .as_str(),
        );
        self.append_readable_event(
            seq,
            turn,
            format!("completion_call history_len={history_len} prompt=\"{}\"", preview.replace('\n', "\\n")),
            Some("orchestrator -> model completion".to_string()),
        );
    }

    pub fn log_tool(
        &self,
        tool_name: &str,
        args: &str,
        tool_call_id: Option<&str>,
        internal_call_id: &str,
    ) {
        let (seq, turn) = if let Ok(mut inner) = self.inner.lock() {
            inner.sequence_counter += 1;
            (inner.sequence_counter, inner.turn_counter)
        } else {
            (0, 0)
        };
        let event_type = if tool_name.ends_with("_agent") {
            "agent_call"
        } else {
            "tool_call"
        };

        self.log_line(serde_json::json!({
            "phase": "tool",
            "tool": tool_name,
            "args": args,
            "tool_call_id": tool_call_id,
            "internal_call_id": internal_call_id,
            "sequence_index": seq,
            "turn": turn,
            "event_type": event_type,
        }));

        let ts = Utc::now().to_rfc3339();
        let args_preview = args.chars().take(120).collect::<String>().replace('\n', "\\n");
        let args_len = args.chars().count();
        self.log_timeline_line(
            format!(
                "[{seq:03}][{ts}][turn {turn}][{event_type}] {tool_name} tool_call_id={} internal_call_id={} args_len={} args_preview=\"{}\"",
                tool_call_id.unwrap_or("-"),
                internal_call_id,
                args_len,
                args_preview
            )
            .as_str(),
        );
        let parent_hint = if tool_name.ends_with("_agent") {
            format!("orchestrator -> {tool_name}")
        } else {
            format!("agent/tool chain -> {tool_name}")
        };
        self.append_readable_event(
            seq,
            turn,
            format!(
                "{event_type} tool={tool_name} tool_call_id={} internal_call_id={} args_len={} args_preview=\"{}\"",
                tool_call_id.unwrap_or("-"),
                internal_call_id,
                args_len,
                args_preview
            ),
            Some(parent_hint),
        );
    }
}

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

fn extract_message_text(prompt: &Message) -> String {
    match prompt {
        Message::User { content } => content
            .iter()
            .filter_map(|item| match item {
                UserContent::Text(text) => Some(text.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Message::Assistant { content, .. } => content
            .iter()
            .filter_map(|item| match item {
                AssistantContent::Text(text) => Some(text.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

#[derive(Clone)]
pub struct ChatUiHook {
    pub app: AppHandle,
    pub cancel: Option<SharedChatCancel>,
    pub logger: SessionLogger,
}

impl<M: CompletionModel> PromptHook<M> for ChatUiHook {
    fn on_completion_call(
        &self,
        prompt: &Message,
        history: &[Message],
    ) -> impl std::future::Future<Output = HookAction> + Send {
        let cancel = self.cancel.clone();
        let app = self.app.clone();
        let logger = self.logger.clone();
        async move {
            if let Some(cancel) = cancel {
                if cancel.load(Ordering::SeqCst) {
                    return HookAction::terminate("User stopped the request");
                }
            }
            // Log the prompt that the orchestrator is sending to the model.
            // Note: this can include sensitive vault text, since we log full raw prompts.
            let prompt_text = extract_message_text(prompt);
            logger.log_prompt(&prompt_text, history.len());

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
        tool_call_id: Option<String>,
        internal_call_id: &str,
        args: &str,
    ) -> impl std::future::Future<Output = ToolCallHookAction> + Send {
        let cancel = self.cancel.clone();
        let app = self.app.clone();
        let logger = self.logger.clone();
        let tool_name = tool_name.to_string();
        let tool_call_id = tool_call_id.clone();
        let internal_call_id = internal_call_id.to_string();
        let args = args.to_string();
        async move {
            if let Some(cancel) = cancel {
                if cancel.load(Ordering::SeqCst) {
                    return ToolCallHookAction::terminate("User stopped the request");
                }
            }
            // Tool calls correspond to delegated specialists/tools invoked by Rig.
            logger.log_tool(
                &tool_name,
                &args,
                tool_call_id.as_deref(),
                &internal_call_id,
            );

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

#[cfg(test)]
mod tests {
    use super::SessionLogger;
    use chrono::Utc;

    #[test]
    fn session_logger_writes_ndjson() {
        let session_id = "test-session".to_string();
        let ts = Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_default();
        let ndjson_path =
            std::env::temp_dir().join(format!("protoagent-session-logger-test-{ts}.ndjson"));
        let timeline_path =
            std::env::temp_dir().join(format!("protoagent-session-logger-test-{ts}.timeline.log"));
        let readable_path =
            std::env::temp_dir().join(format!("protoagent-session-logger-test-{ts}.readable.log"));

        let logger = SessionLogger::new(
            session_id.clone(),
            vec![ndjson_path.clone()],
            vec![timeline_path.clone()],
            vec![readable_path.clone()],
        )
        .expect("test logger should be creatable");

        logger.log_prompt("hello", 2);
        logger.log_tool(
            "markdown_agent",
            r#"{"path":"Notes/a.md"}"#,
            Some("tool-call-1"),
            "internal-1",
        );

        let contents = std::fs::read_to_string(&ndjson_path).expect("log file should exist");
        assert!(contents.contains("\"phase\":\"prompt\""));
        assert!(contents.contains("\"phase\":\"tool\""));
        assert!(contents.contains("markdown_agent"));
        assert!(contents.contains(&session_id));

        let timeline = std::fs::read_to_string(&timeline_path).expect("timeline file should exist");
        assert!(timeline.contains("[001]"));
        assert!(timeline.contains("turn 1"));
        assert!(timeline.contains("markdown_agent"));

        let readable =
            std::fs::read_to_string(&readable_path).expect("readable log should exist");
        assert!(readable.contains("InitialPrompt:"));
        assert!(readable.contains("CallSequence:"));
        assert!(readable.contains("markdown_agent"));
    }
}
