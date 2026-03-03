use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

use rig::providers::openai;
use rig::client::CompletionClient;
use rig::completion::Prompt;

#[derive(Debug, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub modified: Option<String>,
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

//RIG agents call
#[tauri::command]
async fn ask_agent(prompt: String) -> Result<String, String> {
    //let client: ollama::Client = ollama::Client::new(Nothing).unwrap();
    let client: openai::Client = openai::Client::new(std::env::var("OPENAI_API_KEY").unwrap()).map_err(|e| e.to_string())?;
    

    let summary_agent = client
        .agent(openai::GPT_4O_MINI)
        .name("summary_agent")
        .preamble("Your a summarizing agent, whose job is to take the content of one or more 
            mardown files and produce a summary in the form of a markdown. Also make sure to add a note that at the top
            marking that it has been summarized by you the summarizing agent.
        ")
        .build();

    let agent = client
        .agent(openai::GPT_4O_MINI)
        .preamble("You are a helpful agent. When you need to summarize something, you MUST call the summary_agent tool with the exact text to summarize. Do NOT output tool definitions or schemas - execute the tool directly.")  
        .tool(summary_agent)
        .build();
    agent
        .prompt(&prompt)
        .max_turns(3)
        .await
        .map_err(|e| e.to_string())


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

#[tauri::command]
fn scan_markdown_files(vault_path: String) -> Result<Vec<FileInfo>, String> {
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
async fn generate_summaries(clusters: Vec<Cluster>) -> Result<MasterSummary, String> {
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
        let summary = match ask_agent("Summarize: ".to_owned() + &markdown_content).await {
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
    
    let overview = match ask_agent("Summarize: ".to_owned() + &master_content).await {
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
        .invoke_handler(tauri::generate_handler![
            scan_markdown_files,
            cluster_files_by_type,
            generate_summaries,
            save_summary_to_vault
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
