mod agent_tools;
mod locality_sensitive_hashing_deduplicate;
mod metadata_parser;
mod pairwise_deduplicate;
mod rag_index;
mod semantic_clustering;

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use walkdir::WalkDir;
use std::env;
use rig::{client::CompletionClient, client::EmbeddingsClient, completion::Prompt, providers::openai};
use rig::vector_store::in_memory_store::InMemoryVectorStore;
use serde_yaml::Value;
use std::collections::{HashMap, HashSet};

use crate::locality_sensitive_hashing_deduplicate::{FileEmbedding, deduplicate_directory, generate_embedded_files};
use semantic_clustering::semantic_clustering_from_file_embeddings;
use locality_sensitive_hashing_deduplicate::group_embeddings;
use crate::pairwise_deduplicate::identify_duplicate_pairs;

const TEXT_EMBEDDING_MODEL: &str = "text-embedding-3-small";

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

#[derive(Debug, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<DirEntry>>,
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
/// Internal implementation of ask_agent. Used by both the ask_agent command and generate_summaries.
async fn ask_agent_impl(
    state: Option<&Mutex<VaultIndexCache>>,
    app: tauri::AppHandle,
    prompt: String,
    vault_path: Option<String>,
) -> Result<String, String> {
    let api_key = get_openai_api_key()?;
    let client: openai::Client = openai::Client::new(api_key).map_err(|e| e.to_string())?;

    // Summary specialist: handles summarization
    let summary_agent = client
        .agent(openai::GPT_4O_MINI)
        .name("summary_agent")
        .preamble("You are a summarizing agent. Your job is to take the content of one or more \
            markdown files and produce a summary in the form of markdown. Add a note at the top \
            marking that it has been summarized by you the summarizing agent.")
        .build();

    // Vault specialist: handles find_duplicates and find_related_files
    let find_duplicates = agent_tools::FindDuplicatesTool { app: app.clone() };
    let find_related = agent_tools::FindRelatedFilesTool { app: app.clone() };
    let vault_agent = client
        .agent(openai::GPT_4O_MINI)
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

    let mut preamble = format!(
        "You are the main orchestrator. The user talks to you. Your job is to understand their intent and delegate to the right specialist. \
        You have two specialists: summary_agent (for summarization) and vault_agent (for finding duplicates or related files in the vault). \
        Always call the appropriate agent—never do the work yourself. {vault_hint}"
    );

    // RAG: build or retrieve vault index for dynamic context
    let mut agent_builder = client
        .agent(openai::GPT_4O_MINI)
        .preamble(&preamble)
        .tool(summary_agent)
        .tool(vault_agent);

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
                agent_builder = agent_builder.preamble(&preamble).dynamic_context(3, index);
            }
        }
    }

    let agent = agent_builder.build();
    agent
        .prompt(&prompt)
        .max_turns(5)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn ask_agent(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<VaultIndexCache>>,
    prompt: String,
    vault_path: Option<String>,
) -> Result<String, String> {
    ask_agent_impl(Some(&*state), app, prompt, vault_path).await
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
        
        if entry.file_type().is_file() {
            let file_path = entry.path();
            //println!("Path: {:?}", &file_path);
            
            if let Some(ext) = file_path.extension() {
                if ext == "md" || ext == "markdown" {
                    // Skip files marked as summaries
                    if is_summary_file(file_path) {
                        continue;
                    }
                    
                    let metadata = entry.metadata().map_err(|e| format!("Error reading file metadata: {}", e))?;
                    
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
                    
                    files.push(FileInfo {
                        name,
                        path: path_str,
                        size: metadata.len(),
                        modified,
                    });
                }
            }
        }
    }
    
    Ok(files)
}

fn list_directory_impl(path: &Path) -> Result<Vec<DirEntry>, String> {
    let mut entries = Vec::new();

    let read_dir = fs::read_dir(path).map_err(|e| format!("Failed to read directory: {}", e))?;

    let mut dirs: Vec<DirEntry> = Vec::new();
    let mut files: Vec<DirEntry> = Vec::new();

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
            dirs.push(DirEntry {
                name,
                path: path_str,
                is_dir: true,
                children,
            });
        } else if metadata.is_file() {
            if let Some(ext) = entry_path.extension() {
                let ext = ext.to_string_lossy().to_lowercase();
                if ext == "md" || ext == "markdown" {
                    files.push(DirEntry {
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
fn list_directory(path: String) -> Result<Vec<DirEntry>, String> {
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
) -> Result<MasterSummary, String> {
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
        let summary = match ask_agent_impl(Some(&*state), app.clone(), "Summarize: ".to_owned() + &markdown_content, None).await {
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

    let overview = match ask_agent_impl(Some(&*state), app, "Summarize: ".to_owned() + &master_content, None).await {
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
        .invoke_handler(tauri::generate_handler![
            ask_agent,
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
            save_summary_to_vault
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
