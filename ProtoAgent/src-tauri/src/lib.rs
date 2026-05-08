mod agent_tools;
mod chat_progress;
mod markdown_proposals;
mod markdown_tools;
mod version_history;
mod locality_sensitive_hashing_deduplicate;
mod metadata_parser;
mod pairwise_deduplicate;
mod rag_index;
mod semantic_clustering;
mod semantic_search;
use rig::providers::azure::TEXT_EMBEDDING_3_LARGE;
use serde::{Deserialize, Serialize};
use std::fs;
use crate::enrich::EnrichedFile;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use walkdir::WalkDir;
use std::env;
use markdown_proposals::SharedProposalStore;
use version_history::{
    new_shared_store, SharedVersionHistoryStore, VersionHistoryEntry, VersionSource,
};
use rig::{
    client::CompletionClient,
    client::EmbeddingsClient,
    completion::{AssistantContent, Message, Prompt, PromptError},
    providers::openai,
};
use tauri::Emitter;
use tauri::Manager;
use rig::vector_store::in_memory_store::InMemoryVectorStore;
use serde_yaml::Value;
use walkdir::DirEntry as WalkDirEntry;
mod test;
mod enrich;
use std::collections::{HashMap, HashSet};

use crate::chat_progress::{ChatUiHook, SessionLogger, SharedChatCancel};
use chrono::Utc;
use crate::locality_sensitive_hashing_deduplicate::{FileEmbedding, deduplicate_directory, generate_embedded_files};
use semantic_clustering::semantic_clustering_from_file_embeddings;
use locality_sensitive_hashing_deduplicate::group_embeddings;
use crate::pairwise_deduplicate::identify_duplicate_pairs;
use uuid::Uuid;
use rig::completion::message::UserContent;

const TEXT_EMBEDDING_MODEL: &str = "text-embedding-3-small";
const MEMORY_RECENT_TURNS: usize = 10;
const MEMORY_SUMMARY_TRIGGER_MESSAGES: usize = 24;
const MEMORY_SUMMARY_MAX_CHARS: usize = 4000;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub modified: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FilePair {
    pub path_a: String,
    pub name_a: String,
    pub path_b: String,
    pub name_b: String,
}

/// Serializable tree node for `list_directory` (not `walkdir::DirEntry`).
#[derive(Debug, Serialize, Deserialize)]
pub struct VaultDirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<VaultDirEntry>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Cluster {
    pub doc_type: String,
    pub files: Vec<FileInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClusterSummary {
    pub doc_type: String,
    pub file_count: usize,
    pub file_names: Vec<String>,
    pub summary: String,
    pub metadata: SummaryMetadata,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SummaryMetadata {
    pub total_files: usize,
    pub date_range: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MasterSummary {
    pub overview: String,
    pub cluster_summaries: Vec<ClusterSummary>,
    pub total_files: usize,
    pub document_types: Vec<String>,
}

/// Cache for the RAG vector store per vault. Rebuilt when vault changes.
pub struct VaultIndexCache {
    pub vault_path: Option<String>,
    pub store: Option<InMemoryVectorStore<rag_index::RagDocument>>,
}

#[derive(Default, Clone)]
pub struct SessionMemory {
    pub rig_history: Vec<Message>,
    pub rolling_summary: String,
}

fn extract_text_from_message(message: &Message) -> Option<(String, String)> {
    match message {
        Message::User { content } => {
            let text = content
                .iter()
                .filter_map(|item| match item {
                    UserContent::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            if text.trim().is_empty() {
                None
            } else {
                Some(("User".to_string(), text))
            }
        }
        Message::Assistant { content, .. } => {
            let text = content
                .iter()
                .filter_map(|item| match item {
                    AssistantContent::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            if text.trim().is_empty() {
                None
            } else {
                Some(("Assistant".to_string(), text))
            }
        }
    }
}

fn extract_recent_turns(history: &[Message], max_turns: usize) -> Vec<(String, String)> {
    let turns = history
        .iter()
        .filter_map(extract_text_from_message)
        .collect::<Vec<_>>();
    if turns.len() > max_turns {
        turns[turns.len() - max_turns..].to_vec()
    } else {
        turns
    }
}

fn compact_text(input: &str, max_chars: usize) -> String {
    let trimmed = input.trim().replace('\n', " ");
    if trimmed.chars().count() <= max_chars {
        trimmed
    } else {
        let mut out = trimmed.chars().take(max_chars).collect::<String>();
        out.push_str("...");
        out
    }
}

fn append_summary(existing: &str, dropped: &[Message]) -> String {
    let mut lines = Vec::new();
    for (role, text) in dropped.iter().filter_map(extract_text_from_message) {
        lines.push(format!("- {}: {}", role, compact_text(&text, 220)));
    }
    if lines.is_empty() {
        return existing.to_string();
    }

    let mut merged = String::new();
    if !existing.trim().is_empty() {
        merged.push_str(existing.trim());
        merged.push('\n');
    }
    merged.push_str("Additional context from earlier turns:\n");
    merged.push_str(&lines.join("\n"));

    let merged_compact = compact_text(&merged, MEMORY_SUMMARY_MAX_CHARS);
    merged_compact
}

fn compact_session_memory(memory: &mut SessionMemory) {
    if memory.rig_history.len() <= MEMORY_SUMMARY_TRIGGER_MESSAGES {
        return;
    }
    let keep_from = memory
        .rig_history
        .len()
        .saturating_sub(MEMORY_SUMMARY_TRIGGER_MESSAGES);
    let dropped = memory.rig_history[..keep_from].to_vec();
    memory.rolling_summary = append_summary(&memory.rolling_summary, &dropped);
    memory.rig_history = memory.rig_history[keep_from..].to_vec();
}

fn build_memory_preface(rolling_summary: &str, recent_turns: &[(String, String)]) -> String {
    let mut sections = Vec::new();
    if !rolling_summary.trim().is_empty() {
        sections.push(format!("Conversation summary:\n{}", rolling_summary.trim()));
    }
    if !recent_turns.is_empty() {
        let rendered = recent_turns
            .iter()
            .map(|(role, text)| format!("{}: {}", role, compact_text(text, 500)))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("Recent conversation:\n{}", rendered));
    }
    sections.join("\n\n")
}

/// Curated OpenAI chat models for the multi-agent stack (IDs align with `rig` `openai::completion` constants).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatModelOption {
    pub id: String,
    pub label: String,
}

/// `(model_id, display_label)` — keep in sync with [`list_chat_models`].
const CURATED_CHAT_MODELS: &[(&str, &str)] = &[
    (openai::GPT_4O, "GPT-4o"),
    (openai::GPT_4O_MINI, "GPT-4o mini"),
    (openai::GPT_4_TURBO, "GPT-4 Turbo"),
    (openai::GPT_5, "GPT-5"),
    (openai::GPT_5_MINI, "GPT-5 mini"),
    (openai::GPT_5_NANO, "GPT-5 nano"),
    (openai::O3_MINI, "o3-mini"),
    (openai::O4_MINI, "o4-mini"),
];

fn resolve_chat_model(model: Option<String>) -> Result<String, String> {
    let id = model
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| openai::GPT_4O_MINI.to_string());
    if CURATED_CHAT_MODELS
        .iter()
        .any(|(mid, _)| *mid == id.as_str())
    {
        Ok(id)
    } else {
        Err(format!(
            "Unknown chat model \"{}\". Pick a model from the list.",
            id
        ))
    }
}

#[tauri::command]
fn list_chat_models() -> Vec<ChatModelOption> {
    CURATED_CHAT_MODELS
        .iter()
        .map(|(id, label)| ChatModelOption {
            id: (*id).to_string(),
            label: (*label).to_string(),
        })
        .collect()
}

/// How many RAG chunks to inject via [`AgentBuilder::dynamic_context`] (default 20).
const DEFAULT_RAG_CONTEXT_TOP_K: usize = 20;
const MIN_RAG_CONTEXT_TOP_K: usize = 1;
const MAX_RAG_CONTEXT_TOP_K: usize = 50;

fn resolve_rag_context_top_k(k: Option<u32>) -> usize {
    k.map(|n| n as usize)
        .unwrap_or(DEFAULT_RAG_CONTEXT_TOP_K)
        .clamp(MIN_RAG_CONTEXT_TOP_K, MAX_RAG_CONTEXT_TOP_K)
}

// Helper function to read API key from config file
pub fn get_openai_api_key() -> Result<String, String> {
    // First try environment variable
    if let Ok(key) = env::var("OPENAI_API_KEY") {
        if !key.is_empty() && key != "YOUR_OPENAI_API_KEY_HERE" {
            return Ok(key);
        }
    }
    
    // Then try config file (try multiple possible paths)
    let possible_paths = vec![
        "src-tauri/config.toml",
        "config.toml",
        "./config.toml",
    ];
    
    for config_path_str in possible_paths {
        let config_path = Path::new(config_path_str);
        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(config_path) {
                if let Ok(config) = toml::from_str::<toml::Value>(&content) {
                    if let Some(openai) = config.get("openai") {
                        if let Some(api_key) = openai.get("api_key") {
                            if let Some(key_str) = api_key.as_str() {
                                if !key_str.is_empty() && key_str != "YOUR_OPENAI_API_KEY_HERE" {
                                    return Ok(key_str.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    Err("OpenAI API key not found. Please set OPENAI_API_KEY environment variable or configure it in src-tauri/config.toml".to_string())
}

fn map_prompt_outcome(result: Result<String, PromptError>) -> Result<String, String> {
    match result {
        Ok(s) => Ok(s),
        Err(e) => match e {
            // Treat cancel as Err so SessionMemory is not persisted with dangling tool_calls.
            PromptError::PromptCancelled { reason, .. } => {
                Err(format!("Request stopped. {}", reason))
            }
            _ => Err(e.to_string()),
        },
    }
}

/// Internal implementation of ask_agent. Used by both the ask_agent command and generate_summaries.
async fn ask_agent_impl(
    state: Option<&Mutex<VaultIndexCache>>,
    app: tauri::AppHandle,
    prompt: String,
    vault_path: Option<String>,
    chat_model: String,
    rag_context_top_k: usize,
    chat_cancel: Option<SharedChatCancel>,
    chat_history: Option<&mut Vec<Message>>,
) -> Result<String, String> {
    let api_key = get_openai_api_key()?;
    let client: openai::Client = openai::Client::new(api_key).map_err(|e| e.to_string())?;

    // Summary specialist: handles summarization
    let summary_agent = client
        .agent(chat_model.as_str())
        .name("summary_agent")
        .preamble("You are a summarizing agent. Your job is to take the content of one or more \
            markdown files and produce a summary in the form of markdown.")
        .build();

    // Vault specialist: handles find_duplicates and find_related_files
    let find_duplicates = agent_tools::FindDuplicatesTool { app: app.clone() };
    let find_related = agent_tools::FindRelatedFilesTool { app: app.clone() };
    let vault_agent = client
        .agent(chat_model.as_str())
        .name("vault_agent")
        .description("Handles vault operations: finding duplicate markdown files and semantically related files. Call with the vault path and the operation (find duplicates or find related files).")
        .preamble("You are a vault operations specialist. You receive requests to find duplicate files or semantically related files. Use the vault path provided in the request with the appropriate tool (find_duplicates or find_related_files).")
        .tool(find_duplicates)
        .tool(find_related)
        .build();

    let vault_hint = vault_path
        .as_ref()
        .map(|p| format!("The user's vault is at: {}. When they ask to find duplicates or related files, call the vault_agent with the vault path and the operation.", p))
        .unwrap_or_else(|| "No vault is selected. If the user asks to find duplicates or related files, ask them to select a vault folder first.".to_string());

    let proposal_store: SharedProposalStore = app.state::<SharedProposalStore>().inner().clone();
    let propose_create = markdown_tools::ProposeCreateMarkdownTool {
        app: app.clone(),
        store: proposal_store.clone(),
        vault_root: vault_path.clone(),
    };
    let propose_edit = markdown_tools::ProposeEditMarkdownTool {
        app: app.clone(),
        store: proposal_store,
        vault_root: vault_path.clone(),
    };
    let md_vault_hint = vault_path
        .as_ref()
        .map(|p| {
            format!(
                "The vault root for markdown paths is: {}. Use paths relative to this root (e.g. Notes/topic.md).",
                p
            )
        })
        .unwrap_or_else(|| {
            "No vault is selected—you cannot propose markdown file changes until the user selects a vault folder."
                .to_string()
        });
    let markdown_preamble = format!(
        "You are a markdown editing specialist. Use propose_create_markdown for NEW files and propose_edit_markdown to replace the ENTIRE contents of an existing file. Use vault-relative paths ending in .md or .markdown. You only have orchestrator/RAG and user messages—write proposals carefully. When proposing an edit, the tool reads the current file to snapshot previous content for review. Saving to disk only happens when the user clicks Accept in the app's pending proposals panel—you cannot save from chat. Do not ask whether to save, proceed, or approve changes in this conversation; you cannot perform that step. If the user wants different wording, invite them to describe edits so you can submit a revised proposal, or they can Reject and ask again. {}",
        md_vault_hint
    );
    let markdown_agent = client
        .agent(chat_model.as_str())
        .name("markdown_agent")
        .description("Creates or edits vault markdown by submitting proposals. The user saves changes only via Accept in the pending proposals panel—not through chat. Do not ask for save/approval in chat; offer revision guidance instead.")
        .preamble(markdown_preamble.as_str())
        .tool(propose_create)
        .tool(propose_edit)
        .build();

    let coach_preamble = "You are a development coach for people with messy or abundant ideas. Your job is emotional and strategic clarity: help the user decide what to do next when they feel stuck or overwhelmed. \
        Infer or gently ask what stage they are in (exploring options, committing to a direction, executing, or maintaining/revising). \
        Surface gaps, contradictions, or weak areas with empathy—not judgment. \
        Offer concrete, ordered next steps (small wins first). \
        Suggest short creative exercises when they help unblock thinking. \
        Ask focused questions—prefer one or a few at a time. \
        Recommend which notes or themes to revisit; use file names or topics from the orchestrator's context when they appear in the request or retrieved vault documents—do not invent filenames you were not given. \
        You may suggest what new notes or sections could exist or what existing notes could expand—in prose only. You have no tools to write files. When the user wants an actual vault change, tell them the orchestrator can delegate to markdown_agent for proposals they approve in the app. \
        Do not summarize whole vaults unless asked; do not run duplicate-file scans or related-file scans—that is vault_agent. Do not create markdown proposals—that is markdown_agent.";

    let development_coach_agent = client
        .agent(chat_model.as_str())
        .name("development_coach_agent")
        .description("Helps when the user feels stuck, has too many ideas, or needs next steps, exercises, and reflection—not for summarizing notes, scanning the vault, or editing files.")
        .preamble(coach_preamble)
        .build();

    let mut preamble = format!(
        "You are the main orchestrator. The user talks to you. Your job is to understand their intent and delegate to the right specialist. \
        You have four specialists: summary_agent (for summarization), vault_agent (for finding duplicates or related files in the vault), markdown_agent (for creating or editing markdown files via proposals that the user must approve before they are saved), and development_coach_agent (for coaching when the user is overwhelmed, stuck, or needs prioritization, exercises, focused questions, and what to revisit or expand—not for saving files). \
        When the user needs empathy, structure, or \"what should I do next\" with messy ideas, call development_coach_agent. When they need actual vault edits, call markdown_agent. \
        When markdown_agent queues changes, approval happens only in the pending proposals panel in the app—not by replying in chat. Do not ask the user to confirm saving in chat. \
        Routing policy (latency-first): default to exactly ONE specialist per user request. \
        Choose the single best specialist and delegate once. Do NOT chain specialists unless the user explicitly requests multiple outcomes in the same turn. \
        After a successful specialist/tool result, finalize immediately with a concise response and do not re-delegate. \
        Second-agent delegation is allowed only if the first specialist hits a hard blocker (for example missing vault path) or the user explicitly asks for a second distinct outcome. \
        Anti-ping-pong rules: never call development_coach_agent after markdown_agent unless user asks for coaching; never call summary_agent after vault_agent unless user asks for summary; never call vault_agent solely to support markdown edits unless user explicitly requests vault analysis. \
        If the request is ambiguous between specialists, ask ONE short clarifying question instead of calling multiple agents. \
        Hard cap: max 1 agent_call per request unless second-agent criteria are met. \
        Always call the appropriate agent—never do the work yourself. {vault_hint}"
    );

    // RAG: build or retrieve vault index for dynamic context
    let mut agent_builder = client
        .agent(chat_model.as_str())
        .preamble(&preamble)
        .tool(summary_agent)
        .tool(vault_agent)
        .tool(markdown_agent)
        .tool(development_coach_agent);

    if let (Some(state), Some(ref path)) = (state, vault_path) {
        let need_build = {
            let mut cache = state.lock().map_err(|e| e.to_string())?;
            if cache.vault_path.as_ref() != Some(path) {
                cache.vault_path = Some(path.clone());
                cache.store = None;
            }
            cache.store.is_none()
        };
        if need_build {
            let store = rag_index::build_vault_index(path, &client).await?;
            let mut cache = state.lock().map_err(|e| e.to_string())?;
            cache.store = Some(store);
        }
        let store = {
            let cache = state.lock().map_err(|e| e.to_string())?;
            cache.store.clone()
        };
        if let Some(ref store) = store {
            if !store.is_empty() {
                preamble.push_str(" When relevant, use the retrieved vault documents below to answer. Cite file names when you use them.");
                let embedding_model = client.embedding_model(openai::TEXT_EMBEDDING_3_SMALL);
                let index = store.clone().index(embedding_model);
                agent_builder = agent_builder
                    .preamble(&preamble)
                    .dynamic_context(rag_context_top_k, index);
            }
        }
    }

    let agent = agent_builder.build();

    // Create a timestamped NDJSON log file for this prompt session.
    // This is intentionally "best effort": logging should not break the app.
    let session_logger = {
        let session_id = Uuid::new_v4().to_string();
        let timestamp = Utc::now()
            .format("%Y-%m-%dT%H-%M-%S%.3fZ")
            .to_string();

        let ndjson_filename = format!("session_{session_id}_{timestamp}.ndjson");
        let timeline_filename = format!("session_{session_id}_{timestamp}.timeline.log");
        let readable_filename = format!("session_{session_id}_{timestamp}.readable.log");

        // Write logs to the standard app log dir and also repo-local logs for easier discovery.
        let primary_dir = app
            .path()
            .app_log_dir()
            .map_err(|e| e.to_string())
            .unwrap_or_else(|_| std::env::temp_dir().join("protoagent-session-logs"));
        let repo_logs_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|p| p.join("logs"));

        let mut log_dirs = vec![primary_dir];
        if let Some(repo_dir) = repo_logs_dir {
            log_dirs.push(repo_dir);
        }

        let ndjson_paths = log_dirs
            .iter()
            .map(|d| d.join(&ndjson_filename))
            .collect::<Vec<_>>();
        let timeline_paths = log_dirs
            .iter()
            .map(|d| d.join(&timeline_filename))
            .collect::<Vec<_>>();
        let readable_paths = log_dirs
            .iter()
            .map(|d| d.join(&readable_filename))
            .collect::<Vec<_>>();

        let logger =
            SessionLogger::new(session_id.clone(), ndjson_paths, timeline_paths, readable_paths)
            .map_err(|e| e.to_string())
            .unwrap_or_else(|e| {
                eprintln!("Session logging setup failed: {e}");
                // Final fallback: temp-dir only logger. If this also fails, use noop logger.
                let fallback_dir = std::env::temp_dir().join("protoagent-session-logs");
                let fallback_ndjson =
                    fallback_dir.join(format!("session_fallback_{session_id}_{timestamp}.ndjson"));
                let fallback_timeline = fallback_dir.join(format!(
                    "session_fallback_{session_id}_{timestamp}.timeline.log"
                ));
                let fallback_readable = fallback_dir.join(format!(
                    "session_fallback_{session_id}_{timestamp}.readable.log"
                ));

                SessionLogger::new(
                    session_id.clone(),
                    vec![fallback_ndjson],
                    vec![fallback_timeline],
                    vec![fallback_readable],
                )
                .unwrap_or_else(|fallback_err| {
                    eprintln!("Fallback session logging setup failed: {fallback_err}");
                    SessionLogger::noop(session_id.clone())
                })
            });

        logger
    };

    let outcome = if let Some(cancel) = chat_cancel {
        let hook = ChatUiHook {
            app: app.clone(),
            cancel: Some(cancel),
            logger: session_logger,
        };
        if let Some(history) = chat_history {
            agent
                .prompt(&prompt)
                .max_turns(8)
                .with_history(history)
                .with_hook(hook)
                .await
        } else {
            agent
                .prompt(&prompt)
                .max_turns(8)
                .with_hook(hook)
                .await
        }
    } else {
        let hook = ChatUiHook {
            app: app.clone(),
            cancel: None,
            logger: session_logger,
        };
        if let Some(history) = chat_history {
            agent
                .prompt(&prompt)
                .max_turns(8)
                .with_history(history)
                .with_hook(hook)
                .await
        } else {
            agent
                .prompt(&prompt)
                .max_turns(8)
                .with_hook(hook)
                .await
        }
    };

    map_prompt_outcome(outcome)
}

#[tauri::command]
async fn ask_agent(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<VaultIndexCache>>,
    memory_state: tauri::State<'_, Mutex<SessionMemory>>,
    cancel_flag: tauri::State<'_, SharedChatCancel>,
    prompt: String,
    vault_path: Option<String>,
    model: Option<String>,
    rag_context_top_k: Option<u32>,
) -> Result<String, String> {
    let chat_model = resolve_chat_model(model)?;
    let k = resolve_rag_context_top_k(rag_context_top_k);
    cancel_flag.store(false, Ordering::SeqCst);
    let (mut rig_history, rolling_summary) = {
        let memory = memory_state.lock().map_err(|e| e.to_string())?;
        (memory.rig_history.clone(), memory.rolling_summary.clone())
    };
    let recent_turns = extract_recent_turns(&rig_history, MEMORY_RECENT_TURNS);
    let memory_preface = build_memory_preface(&rolling_summary, &recent_turns);
    let effective_prompt = if memory_preface.is_empty() {
        prompt.clone()
    } else {
        format!(
            "{memory_preface}\n\nCurrent user request:\n{prompt}\n\nUse recent conversation context to avoid repeating clarification questions when intent is already established."
        )
    };

    let result = ask_agent_impl(
        Some(&*state),
        app,
        effective_prompt,
        vault_path,
        chat_model,
        k,
        Some(cancel_flag.inner().clone()),
        Some(&mut rig_history),
    )
    .await;

    if result.is_ok() {
        let mut memory = memory_state.lock().map_err(|e| e.to_string())?;
        memory.rig_history = rig_history;
        compact_session_memory(&mut memory);
    }

    result
}



// Helper function to read and combine markdown file contents
fn read_markdown_contents(files: &[FileInfo]) -> Result<String, String> {
    let mut combined_content = String::new();
    
    for file in files {
        match fs::read_to_string(&file.path) {
            Ok(content) => {
                combined_content.push_str(&format!("\n\n--- File: {} ---\n\n", file.name));
                combined_content.push_str(&content);
            }
            Err(e) => {
                return Err(format!("Failed to read file {}: {}", file.path, e));
            }
        }
    }
    
    Ok(combined_content)
}

// Helper function to check if a file is a summary file by reading its frontmatter
fn is_summary_file(file_path: &Path) -> bool {
    if let Ok(content) = fs::read_to_string(file_path) {
        // Check for YAML frontmatter (starts with ---)
        if content.starts_with("---\n") || content.starts_with("---\r\n") {
            // Find the closing --- (can be \n---\n or \r\n---\r\n or \n--- or \r\n---)
            let search_start = if content.starts_with("---\r\n") { 5 } else { 4 };
            if let Some(end_pos) = content[search_start..].find("\n---") {
                let frontmatter = &content[search_start..search_start + end_pos];
                // Check if frontmatter contains summary: true
                if frontmatter.contains("summary:") {
                    // Simple check for summary: true (case-insensitive, handles spaces)
                    let frontmatter_lower = frontmatter.to_lowercase();
                    if frontmatter_lower.contains("summary: true") 
                        || frontmatter_lower.contains("summary:true")
                        || frontmatter_lower.contains("summary:  true") {
                        return true;
                    }
                }
            }
        }
    }
    false
}


fn build_file_info(entry: &WalkDirEntry) -> Result<Option<FileInfo>, String> {
    if entry.file_type().is_file() {
        let file_path = entry.path();

        if let Some(ext) = file_path.extension() {
            if ext == "md" || ext == "markdown" {

                // Skip summary files
                if is_summary_file(file_path) {
                    return Ok(None);
                }

                let metadata = entry
                    .metadata()
                    .map_err(|e| format!("Error reading file metadata: {}", e))?;

                let modified = metadata
                    .modified()
                    .ok()
                    .and_then(|time| {
                        time.duration_since(std::time::UNIX_EPOCH)
                            .ok()
                            .map(|d| d.as_secs().to_string())
                    });

                let name = file_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let path_str = file_path
                    .to_string_lossy()
                    .to_string();

                return Ok(Some(FileInfo {
                    name,
                    path: path_str,
                    size: metadata.len(),
                    modified,
                }));
            }
        }
    }

    Ok(None)
}


fn scan_markdown_files_impl(vault_path: &String) -> Result<Vec<FileInfo>, String> {
    let path = Path::new(&vault_path);
    
    if !path.exists() {
        return Err("Vault path does not exist".to_string());
    }
    
    if !path.is_dir() {
        return Err("Vault path is not a directory".to_string());
    }

    let mut files = Vec::new();
    
    for entry in WalkDir::new(path).follow_links(true) {
        let entry = entry.map_err(|e| format!("Error reading directory: {}", e))?;
        
        if let Some(file_info) = build_file_info(&entry)? {
            files.push(file_info);
        }
    }
    
    Ok(files)
}

fn list_directory_impl(path: &Path) -> Result<Vec<VaultDirEntry>, String> {
    let mut entries = Vec::new();

    let read_dir = fs::read_dir(path).map_err(|e| format!("Failed to read directory: {}", e))?;

    let mut dirs: Vec<VaultDirEntry> = Vec::new();
    let mut files: Vec<VaultDirEntry> = Vec::new();

    for entry in read_dir {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let entry_path = entry.path();
        let name = entry_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        // Skip hidden files/dirs (starting with .)
        if name.starts_with('.') {
            continue;
        }

        let path_str = entry_path.to_string_lossy().to_string();
        let metadata = entry.metadata().map_err(|e| format!("Failed to read metadata: {}", e))?;

        if metadata.is_dir() {
            let children = match list_directory_impl(&entry_path) {
                Ok(c) if !c.is_empty() => Some(c),
                Ok(_) => None,
                Err(_) => None,
            };
            dirs.push(VaultDirEntry {
                name,
                path: path_str,
                is_dir: true,
                children,
            });
        } else if metadata.is_file() {
            if let Some(ext) = entry_path.extension() {
                let ext = ext.to_string_lossy().to_lowercase();
                if ext == "md" || ext == "markdown" {
                    files.push(VaultDirEntry {
                        name,
                        path: path_str,
                        is_dir: false,
                        children: None,
                    });
                }
            }
        }
    }

    // Sort: directories first, then files, alphabetically (case-insensitive)
    dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    entries.append(&mut dirs);
    entries.append(&mut files);

    Ok(entries)
}

#[tauri::command]
fn list_directory(path: String) -> Result<Vec<VaultDirEntry>, String> {
    let path = Path::new(&path);
    if !path.exists() {
        return Err("Path does not exist".to_string());
    }
    if !path.is_dir() {
        return Err("Path is not a directory".to_string());
    }
    list_directory_impl(path)
}

#[tauri::command]
fn read_file(path: String) -> Result<String, String> {
    let path = Path::new(&path);
    if !path.exists() {
        return Err("File does not exist".to_string());
    }
    if !path.is_file() {
        return Err("Path is not a file".to_string());
    }
    fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))
}

#[tauri::command]
fn write_file(path: String, content: String) -> Result<(), String> {
    let path = Path::new(&path);
    fs::write(path, content).map_err(|e| format!("Failed to write file: {}", e))
}

#[tauri::command]
fn scan_markdown_files(vault_path: String) -> Result<Vec<FileInfo>, String> {
    scan_markdown_files_impl(&vault_path)
}

// Search all files that are closest related to the query
// For example, if the query is "Cooking", it will return the files that are most related to Cooking
// Combined with clustering, this allows the user to define a cluster of files that are related to the query
#[tauri::command]
async fn async_semantic_search(
    query: String,
    top_k: usize,
    file_embeddings: &mut HashMap<usize, FileEmbedding>,
) -> Result<Vec<(FileEmbedding, f32)>, String> {

    semantic_search::semantic_search(query, top_k, file_embeddings).await
}

// Search all files in the vault that are closest related to the query
// For example, if the query is "Cooking", it will return the files that are most related to Cooking
// Combined with clustering, this allows the user to define a cluster of files that are related to the query
#[tauri::command]
async fn vault_semantic_search(
    vault_path: String,
    query: String,
    top_k: usize,
) -> Result<Vec<(FileEmbedding, f32)>, String> {
    semantic_search::vault_semantic_search(vault_path, query, top_k).await
}

// Load the files that are linked to the file
// This will search a returns a list of files that are linked to the text of the target file
pub fn load_neighbor_files_from_file(file_path: &str) -> Result<Vec<FileInfo>, String> { 
    semantic_search::load_neighbors(file_path)
}

// Get the top related files to the file
// This will search the vault and return the top related files to the file semantically
// for example, if the file is about "Cooking", it will return the files that are most related the meaning of that file
// Combined with clustering, this allows the user to define a cluster of files that are related to the query
pub async fn get_top_related_files(
    top_neighbors: usize,
    file_path: &str,
) -> Result<Vec<(FileEmbedding, f32)>, String> {
    semantic_search::get_top_related_files(top_neighbors, file_path).await
}

// Enrich the file with examples
pub async fn enrich_file_with_examples(
    file_path: &str,
) -> Result<EnrichedFile, String> {
    enrich::enrich_file_with_examples(file_path).await
}

// Detect missing knowledge in the file
// This will identify missing, incomplete, or unclear knowledge in the note
// and suggest additions to the note to improve its completeness and clarity
pub async fn detect_missing_knowledge(file_path: &str) -> Result<EnrichedFile, String> {
    enrich::detect_missing_knowledge(file_path).await
}

// Enrich the file by explaining the definitions of the terms in the file
pub async fn enrich_file_definitions(
    file_path: &str,
) -> Result<EnrichedFile, String> {
    enrich::enrich_file_definitions(file_path).await
}

// Expand the file with missing details
pub async fn expand_file_details(
    file_path: &str,
) -> Result<EnrichedFile, String> {
    enrich::expand_file_details(file_path).await
}

// Enrich the file with the user's task
// Allows the user to specify a custom type of enrichment for the file
pub async fn enrich_file_custom(
    file_path: &str,
    user_task: &str,
) -> Result<EnrichedFile, String> {
    enrich::enrich_file_custom(file_path, user_task).await
}


pub async fn get_semantic_file_cluster(
    vault_path: String,
) -> Result<Vec<(FileEmbedding, FileEmbedding)>, String> {
    let files = scan_markdown_files(vault_path)
        .map_err(|_| "Fail to scan markdown files".to_string())?;

    let embeddings: HashMap<usize, FileEmbedding> = generate_embedded_files(&files, TEXT_EMBEDDING_MODEL)
        .await
        .map_err(|_| "Fail to load embeddings".to_string())?;

    let semantic_file_grouping = group_embeddings(&embeddings);

    let file_clusters = semantic_clustering_from_file_embeddings(
        &mut embeddings.clone(),
        &semantic_file_grouping,
    );

    let mut seen: HashSet<(usize, usize)> = HashSet::new();

    let mut pairs: Vec<(FileEmbedding, FileEmbedding)> = Vec::new();

    for (&id1, cluster) in file_clusters.iter() {
        for &id2 in cluster.iter() {
            if id1 == id2 {
                continue;
            }

            let key = if id1 < id2 { (id1, id2) } else { (id2, id1) };
            if !seen.insert(key) {
                continue;
            }

            let (a, b) = key;
            let (Some(f1), Some(f2)) = (embeddings.get(&a), embeddings.get(&b)) else {
                continue;
            };

            pairs.push((f1.clone(), f2.clone()));
        }
    }

    Ok(pairs)
}


pub async fn get_file_duplications(vault_path: String) -> Vec<FileEmbedding> {
    match deduplicate_directory(&vault_path).await {
        Ok((file_embeddings, _semantic_hashing)) => {
            let mut to_remove_files: Vec<FileEmbedding> = Vec::new();

            for (_, file) in file_embeddings {
                if *file.get_duplicate() {
                    to_remove_files.push(file);
                }
            }

            to_remove_files
        }
        Err(_) => Vec::new(),
    }
}

#[tauri::command]
async fn get_duplicate_pairs(vault_path: String) -> Result<Vec<FilePair>, String> {
    get_duplicate_pairs_impl(vault_path).await
}

pub async fn get_duplicate_pairs_impl(vault_path: String) -> Result<Vec<FilePair>, String> {
    let files = scan_markdown_files(vault_path.clone())
        .map_err(|_| "Failed to scan markdown files".to_string())?;

    let embeddings = generate_embedded_files(&files, TEXT_EMBEDDING_MODEL)
        .await
        .map_err(|_| "Failed to generate embeddings".to_string())?;

    let mut all_pairs: Vec<(String, String)> = Vec::new();

    if embeddings.len() > 3000 {
        let hash_tables = group_embeddings(&embeddings);
        for table in &hash_tables {
            for (_, grouped_ids) in table {
                let ids: Vec<usize> = grouped_ids.clone();
                let pairs = identify_duplicate_pairs(&embeddings, &ids);
                all_pairs.extend(pairs);
            }
        }
    } else {
        let ids: Vec<usize> = embeddings.keys().cloned().collect();
        all_pairs = identify_duplicate_pairs(&embeddings, &ids);
    }

    // Dedupe and convert to FilePair
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut result = Vec::new();
    for (path_a, path_b) in all_pairs {
        let key = if path_a < path_b {
            (path_a.clone(), path_b.clone())
        } else {
            (path_b.clone(), path_a.clone())
        };
        if !seen.insert(key.clone()) {
            continue;
        }
        let name_a = Path::new(&key.0).file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let name_b = Path::new(&key.1).file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        result.push(FilePair {
            path_a: key.0,
            name_a,
            path_b: key.1,
            name_b,
        });
    }

    Ok(result)
}

#[tauri::command]
async fn get_related_pairs(vault_path: String) -> Result<Vec<FilePair>, String> {
    get_related_pairs_impl(vault_path).await
}

pub async fn get_related_pairs_impl(vault_path: String) -> Result<Vec<FilePair>, String> {
    let pairs = get_semantic_file_cluster(vault_path).await?;
    let mut result = Vec::new();
    for (f1, f2) in pairs {
        result.push(FilePair {
            path_a: f1.get_path().to_string(),
            name_a: f1.get_file_info().name.clone(),
            path_b: f2.get_path().to_string(),
            name_b: f2.get_file_info().name.clone(),
        });
    }
    Ok(result)
}

fn add_related_link_impl(path_a: &str, path_b: &str) -> Result<(), String> {
    use crate::metadata_parser::{extract_frontmatter, wiki_link_common_path, convert_to_wiki_link};
    use crate::metadata_parser::normalize_frontmatter_file_links;

    let content_a = fs::read_to_string(path_a).map_err(|e| format!("Failed to read {}: {}", path_a, e))?;
    let content_b = fs::read_to_string(path_b).map_err(|e| format!("Failed to read {}: {}", path_b, e))?;

    let extracted_a = extract_frontmatter(&content_a);
    let extracted_b = extract_frontmatter(&content_b);

    let mut fm_a: HashMap<String, Value> = extracted_a
        .frontmatter
        .as_ref()
        .and_then(|s| serde_yaml::from_str(s).ok())
        .unwrap_or_default();
    let mut fm_b: HashMap<String, Value> = extracted_b
        .frontmatter
        .as_ref()
        .and_then(|s| serde_yaml::from_str(s).ok())
        .unwrap_or_default();

    normalize_frontmatter_file_links(&mut fm_a);
    normalize_frontmatter_file_links(&mut fm_b);

    let path_a_s = path_a.to_string();
    let path_b_s = path_b.to_string();

    let wiki_to_b = wiki_link_common_path(&path_a_s, &path_b_s)
        .map(|p| convert_to_wiki_link(&p))
        .ok_or_else(|| "Could not compute relative path".to_string())?;
    let wiki_to_a = wiki_link_common_path(&path_b_s, &path_a_s)
        .map(|p| convert_to_wiki_link(&p))
        .ok_or_else(|| "Could not compute relative path".to_string())?;

    if !fm_a.contains_key("related") {
        fm_a.insert("related".to_string(), Value::Sequence(vec![]));
    }
    if let Some(Value::Sequence(seq)) = fm_a.get_mut("related") {
        let link_val = Value::String(wiki_to_b);
        if !seq.contains(&link_val) {
            seq.push(link_val);
        }
    }

    if !fm_b.contains_key("related") {
        fm_b.insert("related".to_string(), Value::Sequence(vec![]));
    }
    if let Some(Value::Sequence(seq)) = fm_b.get_mut("related") {
        let link_val = Value::String(wiki_to_a);
        if !seq.contains(&link_val) {
            seq.push(link_val);
        }
    }

    save_file(path_a, &extracted_a.content, &fm_a)
        .map_err(|e| format!("Failed to save {}: {}", path_a, e))?;
    save_file(path_b, &extracted_b.content, &fm_b)
        .map_err(|e| format!("Failed to save {}: {}", path_b, e))?;

    Ok(())
}

#[tauri::command]
fn add_related_link(path_a: String, path_b: String) -> Result<(), String> {
    add_related_link_impl(&path_a, &path_b)
}

#[tauri::command]
fn delete_file_cmd(path: String) -> Result<(), String> {
    delete_file(&path)
}

pub fn delete_file(path: &str) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("Failed to delete file {}: {}", path, e)),
    }
}


pub fn save_file(
    path: &str,
    content: &str,
    frontmatter: &HashMap<String, Value>,
) -> Result<(), std::io::Error> {

    let yaml = serde_yaml::to_string(frontmatter).unwrap();

    //println!("{:?}", path);
    //println!("{:?}\n\n", frontmatter);

    let file_data = format!(
        "---\n{}---\n\n{}",
        yaml,
        content
    );

    fs::write(path, file_data)?;

    Ok(())
}


#[tauri::command]
fn cluster_files_by_type(files: Vec<FileInfo>) -> Result<Vec<Cluster>, String> {
    use regex::Regex;
    
    let syllabus_pattern = Regex::new(r"(?i)syllabus").unwrap();
    let assignment_pattern = Regex::new(r"(?i)(assignment|homework|hw|problem\s+set)").unwrap();
    let schedule_pattern = Regex::new(r"(?i)(schedule|calendar|timeline)").unwrap();
    let notes_pattern = Regex::new(r"(?i)(notes|lecture|class\s+notes)").unwrap();
    
    let mut clusters: std::collections::HashMap<String, Vec<FileInfo>> = std::collections::HashMap::new();
    
    for file in files {
        let mut doc_type = "other".to_string();
        
        // Check filename
        if syllabus_pattern.is_match(&file.name) {
            doc_type = "syllabus".to_string();
        } else if assignment_pattern.is_match(&file.name) {
            doc_type = "assignment".to_string();
        } else if schedule_pattern.is_match(&file.name) {
            doc_type = "schedule".to_string();
        } else if notes_pattern.is_match(&file.name) {
            doc_type = "notes".to_string();
        } else {
            // Check file content (first 500 chars)
            if let Ok(content) = fs::read_to_string(&file.path) {
                let preview = content.chars().take(500).collect::<String>();
                
                if syllabus_pattern.is_match(&preview) {
                    doc_type = "syllabus".to_string();
                } else if assignment_pattern.is_match(&preview) {
                    doc_type = "assignment".to_string();
                } else if schedule_pattern.is_match(&preview) {
                    doc_type = "schedule".to_string();
                } else if notes_pattern.is_match(&preview) {
                    doc_type = "notes".to_string();
                }
            }
        }
        
        clusters.entry(doc_type).or_insert_with(Vec::new).push(file);
    }
    
    let mut result: Vec<Cluster> = clusters
        .into_iter()
        .map(|(doc_type, files)| Cluster { doc_type, files })
        .collect();
    
    result.sort_by(|a, b| a.doc_type.cmp(&b.doc_type));
    
    Ok(result)
}

#[tauri::command]
async fn generate_summaries(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<VaultIndexCache>>,
    clusters: Vec<Cluster>,
    model: Option<String>,
) -> Result<MasterSummary, String> {
    let chat_model = resolve_chat_model(model)?;
    let mut cluster_summaries = Vec::new();
    let mut total_files = 0;
    let mut document_types = Vec::new();
    
    // Generate summaries for each cluster
    for cluster in &clusters {
        total_files += cluster.files.len();
        document_types.push(cluster.doc_type.clone());
        
        let file_names: Vec<String> = cluster.files.iter().map(|f| f.name.clone()).collect();
        
        // Read markdown contents for this cluster
        let markdown_content = read_markdown_contents(&cluster.files)?;
        
        // Call OpenAI Assistant to generate summary
        let summary = match ask_agent_impl(Some(&*state), app.clone(), "Summarize: ".to_owned() + &markdown_content, None, chat_model.clone(), DEFAULT_RAG_CONTEXT_TOP_K, None, None).await {
            Ok(s) => s,
            Err(e) => {
                // Fallback to simple summary if API call fails
                eprintln!("Warning: Failed to generate AI summary for cluster {}: {}. Using fallback summary.", cluster.doc_type, e);
                format!(
                    "This cluster contains {} {} document(s).\n\nFiles included:\n{}",
                    cluster.files.len(),
                    cluster.doc_type,
                    file_names.iter().map(|n| format!("- {}", n)).collect::<Vec<_>>().join("\n")
                )
            }
        };
        
        cluster_summaries.push(ClusterSummary {
            doc_type: cluster.doc_type.clone(),
            file_count: cluster.files.len(),
            file_names,
            summary,
            metadata: SummaryMetadata {
                total_files: cluster.files.len(),
                date_range: None,
            },
        });
    }
    
    // Generate master summary by combining all cluster summaries
    let master_content = format!(
        "Cluster Summaries:\n\n{}",
        cluster_summaries.iter()
            .map(|cs| format!("## {}\n\n{}\n", cs.doc_type.to_uppercase(), cs.summary))
            .collect::<Vec<_>>()
            .join("\n\n")
    );

    let overview = match ask_agent_impl(Some(&*state), app, "Summarize: ".to_owned() + &master_content, None, chat_model, DEFAULT_RAG_CONTEXT_TOP_K, None, None).await {
        Ok(s) => s,
        Err(e) => {
            // Fallback to simple overview if API call fails
            eprintln!("Warning: Failed to generate AI master summary: {}. Using fallback overview.", e);
            format!(
                "Master Summary\n\nTotal files processed: {}\nDocument types found: {}\n\nThis vault contains academic documents organized into {} categories.",
                total_files,
                document_types.join(", "),
                cluster_summaries.len()
            )
        }
    };
    
    Ok(MasterSummary {
        overview,
        cluster_summaries,
        total_files,
        document_types,
    })
}

#[tauri::command]
fn save_summary_to_vault(vault_path: String, summaries: MasterSummary) -> Result<(), String> {
    let vault = Path::new(&vault_path);
    
    if !vault.exists() {
        return Err("Vault path does not exist".to_string());
    }
    
    // Save cluster summaries
    for cluster_summary in &summaries.cluster_summaries {
        let filename = format!("_summary_{}.md", cluster_summary.doc_type);
        let file_path = vault.join(&filename);
        
        let body_content = format!(
            "# {} Summary\n\n**Type**: {}\n**File Count**: {}\n\n## Summary\n\n{}\n\n## Files Included\n\n{}\n",
            cluster_summary.doc_type.to_uppercase(),
            cluster_summary.doc_type,
            cluster_summary.file_count,
            cluster_summary.summary,
            cluster_summary.file_names.iter().map(|n| format!("- {}", n)).collect::<Vec<_>>().join("\n")
        );
        
        // Add frontmatter tag to mark this as a summary file
        let content = format!("---\nsummary: true\n---\n\n{}", body_content);
        
        fs::write(&file_path, content)
            .map_err(|e| format!("Failed to write {}: {}", filename, e))?;
    }
    
    // Save master summary
    let master_path = vault.join("_master_summary.md");
    let master_body = format!(
        "# Master Summary\n\n{}\n\n## Document Types\n\n{}\n\n## Cluster Summaries\n\n{}",
        summaries.overview,
        summaries.document_types.iter().map(|t| format!("- {}", t)).collect::<Vec<_>>().join("\n"),
        summaries.cluster_summaries.iter().map(|cs| format!("### {}\n\n{}\n", cs.doc_type.to_uppercase(), cs.summary)).collect::<Vec<_>>().join("\n\n")
    );
    
    // Add frontmatter tag to mark this as a summary file
    let master_content = format!("---\nsummary: true\n---\n\n{}", master_body);
    
    fs::write(&master_path, master_content)
        .map_err(|e| format!("Failed to write master summary: {}", e))?;
    
    Ok(())
}

#[tauri::command]
fn cancel_chat(cancel_flag: tauri::State<'_, SharedChatCancel>) {
    cancel_flag.store(true, Ordering::SeqCst);
}

fn normalize_version_history_path_filter(
    file_path: Option<String>,
) -> Result<Option<String>, String> {
    match file_path {
        None => Ok(None),
        Some(p) => {
            let pb = Path::new(&p);
            if pb.exists() {
                let c = fs::canonicalize(pb).map_err(|e| e.to_string())?;
                Ok(Some(c.to_string_lossy().to_string()))
            } else {
                Ok(Some(p))
            }
        }
    }
}

fn record_proposal_snapshot(
    vh: &SharedVersionHistoryStore,
    snap: &markdown_proposals::AppliedProposalSnapshot,
    batch_id: Option<String>,
) -> Result<(), String> {
    let source = if snap.kind_create {
        VersionSource::ProposalCreate
    } else {
        VersionSource::ProposalEdit
    };
    let mut inner = vh.lock().map_err(|e| e.to_string())?;
    inner.add_entry(
        snap.relative_path.clone(),
        snap.absolute_path.clone(),
        snap.before_content.clone(),
        snap.after_content.clone(),
        source,
        batch_id,
    );
    Ok(())
}

/// Reverts disk to `entry.before_content` (state before that accepted change). For
/// `proposal_create` rows with empty `before_content`, deletes the file instead. Then removes
/// the history row.
fn apply_revert_snapshot(
    app: &tauri::AppHandle,
    vh: &SharedVersionHistoryStore,
    snapshot_entry: &VersionHistoryEntry,
    removed_entry_id: &str,
) -> Result<(), String> {
    let path = Path::new(&snapshot_entry.absolute_path);

    let undo_create = snapshot_entry.source == "proposal_create"
        && snapshot_entry.before_content.is_empty();

    if undo_create {
        if path.exists() {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    } else {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(path, &snapshot_entry.before_content).map_err(|e| e.to_string())?;
    }

    {
        let mut inner = vh.lock().map_err(|e| e.to_string())?;
        inner.remove_entry_by_id(removed_entry_id)?;
    }

    app.emit(
        "vault-tree-changed",
        serde_json::json!({ "path": snapshot_entry.absolute_path.clone() }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn apply_markdown_proposal(
    app: tauri::AppHandle,
    store: tauri::State<'_, SharedProposalStore>,
    version_history_store: tauri::State<'_, SharedVersionHistoryStore>,
    proposal_id: String,
) -> Result<String, String> {
    let snap =
        markdown_proposals::apply_markdown_proposal_impl(&store, &proposal_id)?;
    record_proposal_snapshot(&version_history_store, &snap, None)?;
    app.emit(
        "vault-tree-changed",
        serde_json::json!({ "path": snap.absolute_path.clone() }),
    )
    .map_err(|e| e.to_string())?;
    Ok(snap.absolute_path)
}

/// Apply several pending markdown proposals sharing one batch id in version history.
#[tauri::command]
fn apply_markdown_proposals_batch(
    app: tauri::AppHandle,
    store: tauri::State<'_, SharedProposalStore>,
    version_history_store: tauri::State<'_, SharedVersionHistoryStore>,
    proposal_ids: Vec<String>,
) -> Result<Vec<String>, String> {
    if proposal_ids.is_empty() {
        return Err("No proposals selected".to_string());
    }
    let batch_id = Uuid::new_v4().to_string();
    let mut paths = Vec::new();
    for id in proposal_ids {
        let snap =
            markdown_proposals::apply_markdown_proposal_impl(&store, &id)?;
        record_proposal_snapshot(&version_history_store, &snap, Some(batch_id.clone()))?;
        paths.push(snap.absolute_path.clone());
        app.emit(
            "vault-tree-changed",
            serde_json::json!({ "path": snap.absolute_path.clone() }),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(paths)
}

#[tauri::command]
fn list_version_history(
    version_history_store: tauri::State<'_, SharedVersionHistoryStore>,
    file_path: Option<String>,
    batch_id: Option<String>,
) -> Result<Vec<VersionHistoryEntry>, String> {
    let file_key = normalize_version_history_path_filter(file_path)?;
    let inner = version_history_store
        .lock()
        .map_err(|e| e.to_string())?;
    Ok(inner.list_entries(file_key.as_deref(), batch_id.as_deref()))
}

#[tauri::command]
fn restore_file_version(
    app: tauri::AppHandle,
    version_history_store: tauri::State<'_, SharedVersionHistoryStore>,
    version_id: String,
) -> Result<String, String> {
    let entry = {
        let inner = version_history_store
            .lock()
            .map_err(|e| e.to_string())?;
        inner
            .get_entry(&version_id)
            .ok_or_else(|| "Version not found.".to_string())?
    };
    apply_revert_snapshot(
        &app,
        &version_history_store,
        &entry,
        version_id.as_str(),
    )?;
    Ok(entry.absolute_path)
}

#[tauri::command]
fn restore_batch(
    app: tauri::AppHandle,
    version_history_store: tauri::State<'_, SharedVersionHistoryStore>,
    batch_id: String,
) -> Result<Vec<String>, String> {
    let ordered_ids = {
        let inner = version_history_store
            .lock()
            .map_err(|e| e.to_string())?;
        inner.batch_ids_in_order(&batch_id)
    };
    if ordered_ids.is_empty() {
        return Err("Batch not found or has no recorded versions.".to_string());
    }
    let mut paths = Vec::new();
    for id in ordered_ids {
        let entry = {
            let inner = version_history_store
                .lock()
                .map_err(|e| e.to_string())?;
            inner
                .get_entry(&id)
                .ok_or_else(|| format!("Missing history entry: {}", id))?
        };
        let id_rm = id.clone();
        apply_revert_snapshot(
            &app,
            &version_history_store,
            &entry,
            id_rm.as_str(),
        )?;
        paths.push(entry.absolute_path);
    }
    Ok(paths)
}

#[tauri::command]
fn clear_version_history(
    version_history_store: tauri::State<'_, SharedVersionHistoryStore>,
) -> Result<(), String> {
    let mut inner = version_history_store
        .lock()
        .map_err(|e| e.to_string())?;
    inner.clear_all();
    Ok(())
}

#[tauri::command]
fn reject_markdown_proposal(
    store: tauri::State<'_, SharedProposalStore>,
    proposal_id: String,
) -> Result<(), String> {
    markdown_proposals::reject_markdown_proposal_impl(&store, &proposal_id)
}

#[tauri::command]
fn list_pending_markdown_proposals(
    store: tauri::State<'_, SharedProposalStore>,
) -> Result<Vec<markdown_proposals::MarkdownProposalEvent>, String> {
    markdown_proposals::list_pending_markdown_proposals_impl(&store)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()  
    .with_max_level(tracing::Level::TRACE)  
    .init();

    dotenvy::dotenv().ok();
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(VaultIndexCache {
            vault_path: None,
            store: None,
        }))
        .manage(Mutex::new(SessionMemory::default()))
        .manage(Arc::new(AtomicBool::new(false)) as SharedChatCancel)
        .manage(std::sync::Arc::new(std::sync::Mutex::new(
            std::collections::HashMap::<String, markdown_proposals::PendingProposal>::new(),
        )) as SharedProposalStore)
        .manage(new_shared_store())
        .invoke_handler(tauri::generate_handler![
            ask_agent,
            cancel_chat,
            list_chat_models,
            list_directory,
            read_file,
            write_file,
            get_duplicate_pairs,
            get_related_pairs,
            add_related_link,
            delete_file_cmd,
            scan_markdown_files,
            cluster_files_by_type,
            generate_summaries,
            save_summary_to_vault,
            apply_markdown_proposal,
            apply_markdown_proposals_batch,
            reject_markdown_proposal,
            list_pending_markdown_proposals,
            list_version_history,
            restore_file_version,
            restore_batch,
            clear_version_history,

        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
