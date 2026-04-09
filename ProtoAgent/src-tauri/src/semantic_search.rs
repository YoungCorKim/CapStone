use std::collections::HashMap;
use reqwest::Client;
use crate::FileInfo;
use regex::Regex;
use crate::scan_markdown_files_impl;
use crate::get_openai_api_key;
use crate::locality_sensitive_hashing_deduplicate::{
    FileEmbedding,
    generate_embedded_files,
};
use crate::metadata_parser::{
    extract_frontmatter, 
    get_vault_path,
    same_file,
};
use crate::pairwise_deduplicate::cosine_similarity;
use crate::locality_sensitive_hashing_deduplicate::get_embedding;
use std::{fs, path::Path};
const AI_MODEL: &str = "text-embedding-3-small";

pub async fn vault_semantic_search(
    vault_path: String,
    query: String,
    top_k: usize,
) -> Result<Vec<(FileEmbedding, f32)>, String> {

    let files: Vec<FileInfo> = scan_markdown_files_impl(&vault_path)?;

    let mut file_embeddings: HashMap<usize, FileEmbedding> =
        generate_embedded_files(&files, AI_MODEL).await?;

    semantic_search(query, top_k, &mut file_embeddings).await
}

pub async fn semantic_search(
    query: String,
    top_k: usize,
    file_embeddings: &mut HashMap<usize, FileEmbedding>,
) -> Result<Vec<(FileEmbedding, f32)>, String> {
    let api_key = get_openai_api_key()?;
    let client = Client::new();

    let mut query_embedding = get_embedding(
        &client,
        &api_key,
        &query,
        AI_MODEL,
        "https://api.openai.com/v1/embeddings",
    )
    .await;

    normalize_embedding(&mut query_embedding);

    let mut scores: Vec<(usize, f32)> = Vec::new();

    for (id, file) in file_embeddings.iter() {
        let similarity = cosine_similarity(&query_embedding, file.get_embeddings());
        scores.push((*id, similarity));
    }

    scores.sort_by(|a, b| {
        b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
    });


    Ok(get_top_result(scores, top_k, file_embeddings))
}

pub fn get_top_result(scores:  Vec<(usize, f32)>, top_k: usize, file_embeddings: &mut HashMap<usize, FileEmbedding>) -> Vec<(FileEmbedding, f32)> {
    let mut results: Vec<(FileEmbedding, f32)> = Vec::new();

    for (id, score) in scores.into_iter().take(top_k) {
        if let Some(file) = file_embeddings.get(&id) {
            results.push((file.clone(), score));
        }
    }

    results
}

pub fn load_neighbors(file_path: &str) -> Result<Vec<FileInfo>, String> {
    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let re = Regex::new(r"\[\[([^\]]+)\]\]").unwrap();

    let links: Vec<String> = re.captures_iter(&content)
        .map(|cap| {
            cap[1]
                .split('|')
                .next()
                .unwrap()
                .trim()
                .to_lowercase()
        })
        .collect();

    if links.is_empty() {
        return Ok(vec![]);
    }

    let vault_path = Path::new(file_path)
        .parent()
        .ok_or("Invalid file path")?
        .to_string_lossy()
        .to_string();

    let files = scan_markdown_files_impl(&vault_path)?;

    let mut name_map: HashMap<String, FileInfo> = HashMap::new();

    for file in files {
        if let Some(name) = Path::new(&file.path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
        {
            name_map.insert(name, file);
        }
    }

    let mut neighbors = Vec::new();

    for link in links {
        if let Some(file) = name_map.get(&link) {
            neighbors.push(file.clone());
        }
    }

    Ok(neighbors)
}

pub async fn get_top_related_files(
    top_neighbors: usize,
    file_path: &str,
) -> Result<Vec<(FileEmbedding, f32)>, String> {
    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read file {}: {}", file_path, e))?;

    let query = extract_frontmatter(&content).content;

    let vault_dir = Path::new(file_path)
        .parent()
        .ok_or_else(|| "File has no parent directory".to_string())?;

    let vault_path = vault_dir.to_string_lossy().to_string();

    let result = vault_semantic_search(vault_path, query, top_neighbors + 1).await;

    if let Ok(results) = result {
        let mut neighbors:Vec<(FileEmbedding, f32)> = Vec::new();

        for (file, score) in results.into_iter() {
            if (!same_file(file.get_path(), file_path).unwrap_or(false)) {
                neighbors.push((file, score));
            }
        }

        Ok(neighbors)
    } else {
        return Err(result.unwrap_err());
    }
}

fn normalize_embedding(embeddings: &mut Vec<f32>) {
    let mut sum = 0.0;

    for v in embeddings.iter() {
        sum += v * v;
    }

    let magnitude = sum.sqrt();

    if magnitude > 0.0 {
        for v in embeddings.iter_mut() {
            *v /= magnitude;
        }
    }
}

