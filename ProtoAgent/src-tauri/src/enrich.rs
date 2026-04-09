use crate::get_openai_api_key;
use crate::metadata_parser::extract_frontmatter;
use crate::semantic_search::load_neighbors;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;

const EXPAND_MODEL: &str = "gpt-4o-mini";

#[derive(Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: &'static str,
    messages: Vec<ChatMessage>,
    temperature: f32,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMsg,
}

#[derive(Deserialize)]
struct ChatMsg {
    content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedFile {
    pub path: String,
    pub content: String,
    pub frontmatter: Option<String>,
}

async fn call_llm(client: &Client, api_key: &str, user_prompt: &str) -> Result<String, String> {
    let body = ChatRequest {
        model: EXPAND_MODEL,
        messages: vec![
            ChatMessage {
                role: "system",
                content: "You follow instructions exactly. Return only the expanded Markdown note body (no YAML frontmatter, no preamble).".to_string(),
            },
            ChatMessage {
                role: "user",
                content: user_prompt.to_string(),
            },
        ],
        temperature: 0.4,
    };

    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("OpenAI request failed: {}", e))?;

    if !response.status().is_success() {
        let text = response
            .text()
            .await
            .unwrap_or_else(|_| "(body unavailable)".to_string());
        return Err(format!("OpenAI error: {}", text));
    }

    let parsed: ChatCompletionResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse OpenAI JSON: {}", e))?;

    parsed
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Empty response from OpenAI".to_string())
}

pub async fn expand_file_details(
    file_path: &str
) -> Result<EnrichedFile, String> {

    let task = r#"
Task:
- Expand the target note with missing details
- Use related notes ONLY if relevant
- Do not overcomplicate the note
- Do not add unnecessary details
- Do not add unnecessary information
- Do not add unnecessary information
"#;

    enrich_file(file_path, task).await
}

pub async fn enrich_file_custom(
    file_path: &str,
    user_task: &str,
) -> Result<EnrichedFile, String> {
    enrich_file(file_path, user_task).await
}

pub async fn enrich_file_with_examples(
    file_path: &str
) -> Result<EnrichedFile, String> {

    let task = r#"
Task:
- Add 1 to 3 concise examples for the most important concepts in the note
- Only add examples for concepts that are not already clearly explained

## Examples
- Example 1: ...
- Example 2: ...
"#;

    enrich_file(file_path, task).await
}

pub async fn detect_missing_knowledge(
    file_path: &str
) -> Result<EnrichedFile, String> {

    let task = r#"
Task:
- Identify missing, incomplete, or unclear knowledge in the note
- Suggest additions to the note to improve its completeness and clarity

## Missing Knowledge
- Missing concept: ...
- Incomplete explanation: ...
- Suggested addition: ...
"#;

    enrich_file(file_path, task).await
}

pub async fn enrich_file_definitions(
    file_path: &str
) -> Result<EnrichedFile, String> {

    let task = r#"
- Identify important concepts, terms, or entities in the note
- Add clear and concise definitions for them
- Only add definitions for terms that are not already explained
- Keep definitions short (1–2 sentences each)

## Definitions
- term: definition
- term: definition
"#;

    enrich_file(file_path, task).await
}

async fn enrich_file(file_path: &str, task: &str) -> Result<EnrichedFile, String> {
    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read file {}: {}", file_path, e))?;

    let extracted = extract_frontmatter(&content);
    let content_without_frontmatter = extracted.content;
    let frontmatter = extracted.frontmatter;

    let neighbors = load_neighbors(file_path)?;

    let mut context = String::new();
    for neighbor in neighbors.iter().take(5) {
        context.push_str("\n\n");
        match fs::read_to_string(&neighbor.path) {
            Ok(neighbor_content) => {
                let n = extract_frontmatter(&neighbor_content);
                context.push_str(&n.content);
            }
            Err(e) => return Err(format!("Failed to read neighbor {}: {}", neighbor.path, e)),
        }
    }

    let prompt = format!(
        r#"
Rules:
- STRICTLY preserve the original meaning of the text
- Take into account the related notes when expanding the note
- Do NOT change, reinterpret, or contradict existing content
- Do NOT remove or rewrite original sentences
- Only ADD new information where appropriate
- Avoid redundancy
- Do not hallucinate

Output:
- Keep the original content intact
- Insert additional details naturally where they fit, or append if needed
- Keep it concise and factual

[Target Note]
{}

[Related Notes]
{}

[Task]
{}
"#,
        content_without_frontmatter, context, task
    );

    let api_key = get_openai_api_key()?;
    let client = Client::new();
    let enriched_body = call_llm(&client, &api_key, &prompt).await?;

    let final_content = match &frontmatter {
        Some(fm) => format!(
            "---\n{}\n---\n\n{}",
            fm.trim_end(),
            enriched_body.trim_start()
        ),
        None => enriched_body,
    };

    Ok(EnrichedFile {
        path: file_path.to_string(),
        content: final_content,
        frontmatter,
    })
}