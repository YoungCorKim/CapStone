/*use walkdir::WalkDir;
use std::path::Path;

mod markdown;
mod clustering;
use markdown::{MarkdownFile, parse_markdown_file};
use clustering::{Cluster, cluster_files};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn scan_markdown_files(folder_path: String) -> Result<Vec<MarkdownFile>, String> {
    let path = Path::new(&folder_path);
    
    if !path.exists() {
        return Err("Folder does not exist".to_string());
    }
    
    if !path.is_dir() {
        return Err("Path is not a directory".to_string());
    }
    
    let mut markdown_files = Vec::new();
    
    for entry in WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let file_path = entry.path();
        if let Some(extension) = file_path.extension() {
            if extension == "md" || extension == "markdown" {
                if let Some(path_str) = file_path.to_str() {
                    match parse_markdown_file(path_str) {
                        Ok(parsed_file) => markdown_files.push(parsed_file),
                        Err(e) => {
                            eprintln!("Warning: Failed to parse {}: {}", path_str, e);
                            // Still add the file but with minimal data
                            let mut file = MarkdownFile::new(path_str.to_string());
                            file.all_tags = vec!["untagged".to_string()];
                            markdown_files.push(file);
                        }
                    }
                }
            }
        }
    }
    
    Ok(markdown_files)
}

#[tauri::command]
fn cluster_markdown_files(files: Vec<MarkdownFile>, min_similarity: Option<f64>) -> Result<Vec<Cluster>, String> {
    let similarity_threshold = min_similarity.unwrap_or(0.1); // Lower threshold for more aggressive clustering
    let clusters = cluster_files(files, similarity_threshold);
    Ok(clusters)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![greet, scan_markdown_files, cluster_markdown_files])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}*/

/**************************************************************************************************************************************/.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use lazy_static::lazy_static;
use tauri::command;
use serde::de::DeserializeOwned;

// Keep the Python process globally
lazy_static! {
    static ref PYTHON_BACKEND: Mutex<PythonBackend> = Mutex::new(PythonBackend::new());
}

struct PythonBackend {
    child: Child,
}

impl PythonBackend {
    fn new() -> Self {
        let child = Command::new("python")
            .arg(r"src-python\app.py") // path to your Python script
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("failed to start Python.");

        Self { child }
    }

    /// Generic send function: sends a string to Python and parses the JSON response
    fn send_json<T: DeserializeOwned>(&mut self, data: &str) -> T {
        // Write to Python stdin
        let stdin = self.child.stdin.as_mut().expect("Failed to access stdin");
        stdin.write_all(data.as_bytes()).expect("Failed to write to stdin");
        stdin.write_all(b"\n").expect("Failed to write newline");
        stdin.flush().expect("Failed to flush stdin");

        // Read one line of output from Python stdout
        let stdout = self.child.stdout.as_mut().expect("Failed to access stdout");
        let mut reader = BufReader::new(stdout);
        let mut response = String::new();
        reader.read_line(&mut response).expect("Failed to read line");

        // Parse JSON response into the requested type
        serde_json::from_str(&response).expect("Failed to parse JSON")
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
/*
#[tauri::command]
fn greet(name: &str) -> String {
    let mut backend = PYTHON_BACKEND.lock().unwrap();

    // Build JSON message specifying which Python function to call
    let payload = serde_json::json!({
        "function": "greet",
        "args": { "name": name }
    });

    // Send to Python and get response
    let response: serde_json::Value = backend.send_json(&payload.to_string());

    // Return the result as a string
    response["result"].as_str().unwrap_or("error").to_string()

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running Tauri app");
}

}*/
