use crate::FileInfo;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use crate::get_openai_api_key;
use crate::scan_markdown_files_impl;
use serde_yaml::Value;
use std::collections::HashMap;
use crate::metadata_parser::extract_frontmatter;
use std::fs;
use crate::pairwise_deduplicate::pairwise_deduplicate;
use crate::metadata_parser::normalize_frontmatter_link;
use crate::metadata_parser::normalize_frontmatter_file_links;
use crate::metadata_parser::relative_path_from_dir;
use crate::metadata_parser::convert_to_wiki_link;
use crate::metadata_parser::wiki_link_common_path;
use crate::semantic_clustering::semantic_clustering;
use crate::semantic_clustering::link_files;

//////////////////////////////////////////////  Global Variables  ///////////////////////////////////////////

static HYPERPLANES: i16= 4;

static NUMBER_OF_HASHTABLES: i16 = 10;

//////////////////////////////////////////////  FileEmbedding  //////////////////////////////////////////////

#[derive(Debug, Clone)]
pub struct FileEmbedding {
    id: usize,
    content: String,
    embeddings: Vec<f32>,
    frontmatter: HashMap<String, serde_yaml::Value>,
    file_info: FileInfo,
    duplicate: bool,
}

impl FileEmbedding {
    pub fn new(id:usize, file_info: FileInfo, content: String, embeddings: Vec<f32>, frontmatter: HashMap<String, serde_yaml::Value>) -> Self {
        Self {
            id,
            file_info,
            content,
            embeddings,
            frontmatter,
            duplicate: false,
        }
    }

    pub fn set_duplicate(&mut self) {
        self.duplicate = true;
    }


    pub fn get_duplicate(&self) -> &bool{
        &self.duplicate 
    }


    pub fn unset_duplicate(&mut self) {
        self.duplicate = false;
    }

    pub fn set_content(&mut self, new_content: String) {
        self.content = new_content;
    }
    

    pub fn get_content(&self) -> &str {
        &self.content
    }

    pub fn get_embeddings(&self) -> &Vec<f32> {
        &self.embeddings
    }

    pub fn get_frontmatter(&mut self) -> &mut HashMap<String, Value> {
        &mut self.frontmatter
    }

    pub fn get_file_info(&self) -> &FileInfo {
        &self.file_info
    }

    pub fn get_name(&self) -> &str {
        &self.file_info.name
    }

    pub fn get_path(&self) -> &str {
        &self.file_info.path
    }

    pub fn get_size(&self) -> u64 {
        self.file_info.size
    }

    pub fn get_modified(&self) -> &Option<String> {
        &self.file_info.modified
    }

    pub fn get_id(&self) -> usize {
        self.id
    }
}


//////////////////////////////////////////////  StringOrRemove  //////////////////////////////////////////////

pub enum StringOrRemove {
    Content(String),
    Remove{ removed: usize},
}

impl StringOrRemove {
    pub fn get_content(&self) -> Option<&str> {
        match self {
            StringOrRemove::Content(content) => Some(content),
            StringOrRemove::Remove { .. } => None,
        }
    }

    pub fn removed_count(&self) -> Option<usize> {
        match self {
            StringOrRemove::Remove { removed } => Some(*removed),
            _ => None,
        }
    }

}

//////////////////////////////////////////////  Embedding  //////////////////////////////////////////////

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Serialize)]
struct EmbeddingRequest {
    input: String,
    model: String, 
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

pub async fn deduplicate_directory(
    directory_path: &String
) -> Result<(HashMap<usize, FileEmbedding>, Vec<HashMap<i16, Vec<usize>>>), String> {

    let files = scan_markdown_files_impl(directory_path);

    match files {
        Ok(files) => {
            deduplicate_files_default(files).await
        },

        Err(_) => {
            Err("Fail to scan files".to_string())
        },
    }
}

async fn get_embedding( client: &Client, 
                        api_key: &str, 
                        text: &str,
                        model: &str,
                        api_url: &str) -> Vec<f32> {

    let request_body = EmbeddingRequest {
        input: text.to_string(),
        model: model.to_string(),
    };

    let response = client
        .post(api_url)
        .bearer_auth(api_key)
        .json(&request_body)
        .send()
        .await
        .expect("Failed to send request");

    let response_json: EmbeddingResponse = response
        .json()
        .await
        .expect("Failed to parse response JSON");

    response_json.data[0].embedding.clone()
}

async fn get_embedding_default( client: &Client, 
                                api_key: &str, 
                                text: &str,
                                model: &str) -> Vec<f32> {

    return get_embedding(client, api_key, text, model, "https://api.openai.com/v1/embeddings").await;
}

pub async fn deduplicate_files_default(
    files: Vec<FileInfo>,
) -> Result<(HashMap<usize, FileEmbedding>, Vec<HashMap<i16, Vec<usize>>>), String> {
    deduplicate_files(files, "text-embedding-3-small").await
}

async fn generate_embedded_file(
    file_id: usize,
    file: &FileInfo,
    api_key: &str,
    client: &Client,
    model: &str,
) -> Result<FileEmbedding, String> {

    match fs::read_to_string(&file.path) {
        Ok(content) => {
            let extracted = extract_frontmatter(&content);

            let content_without_frontmatter = extracted.content;

            let mut frontmatter: HashMap<String, Value> = HashMap::new();

            if let Some(frontmatter_string) = extracted.frontmatter {
                match serde_yaml::from_str::<HashMap<String, Value>>(&frontmatter_string) {
                    Ok(fm) => frontmatter = fm,
                    Err(e) => eprintln!("Failed to parse frontmatter: {}", e),
                }
            }

            normalize_frontmatter_file_links(&mut frontmatter);

            /*
            if let Some(value) = frontmatter.get_mut("related") {
                if let Value::String(s) = value {
                    let links: Vec<String> = normalize_frontmatter_file_links(s);
            
                    *value = Value::Sequence(
                        links.into_iter().map(Value::String).collect()
                    );
                }
            }*/

            let mut embeddings = get_embedding_default(
                client,
                api_key,
                &content_without_frontmatter,
                model,
            ).await;

            normalize_embedding(&mut embeddings);

            Ok(FileEmbedding::new(
                file_id,
                file.clone(),
                content_without_frontmatter,
                embeddings,
                frontmatter,
            ))
        }

        Err(e) => Err(e.to_string()),
    }
}

fn calculate_magnitude(embeddings: &Vec<f32>) -> f32 {
    let mut total_embedding_sum:f32 = 0.0;

    for embedding in embeddings {
        total_embedding_sum += embedding * embedding;
    }   

    total_embedding_sum.sqrt()
}

fn normalize_embedding(embeddings: &mut Vec<f32>) {
    let magnitude = calculate_magnitude(embeddings);

    if magnitude > 0.0 {
        for embedding in embeddings {
            *embedding /= magnitude;
        }
    }
}

fn generate_plane(dimension: &usize) -> Vec<f32> {
    use rand::Rng;
    let mut plane:Vec<f32> = Vec::new();
    for _ in 0..*dimension {
        plane.push(rand::thread_rng().gen_range(-1.0..=1.0));
    }
    plane
}

fn generate_hash_table(
    file_embeddings: &HashMap<usize, FileEmbedding>
) -> HashMap<i16, Vec<usize>> {
    if file_embeddings.is_empty() {
        return HashMap::new();
    }

    let mut hash_buckets: HashMap<i16, Vec<usize>> = HashMap::new();

    // Get dimension from any embedding
    let dimension = file_embeddings
        .values()
        .next()
        .expect("file_embeddings is not empty")
        .get_embeddings()
        .len();

    let mut hyperplanes: Vec<Vec<f32>> = Vec::new();

    for _ in 0..HYPERPLANES {
        let plane: Vec<f32> = generate_plane(&dimension);
        hyperplanes.push(plane);
    }

    for (id, file_embedding) in file_embeddings.iter() {
        let embeddings = file_embedding.get_embeddings();

        let mut hash: i16 = 0;

        for (index, plane) in hyperplanes.iter().enumerate() {
            if is_in_plane(plane, &embeddings) {
                hash += 2_i16.pow(index as u32);
            }
        }

        hash_buckets.entry(hash).or_insert_with(Vec::new).push(*id);
    }

    hash_buckets
}

fn is_in_plane(plane_embedding: &Vec<f32>, text_embedding: &Vec<f32>) -> bool {
    return dot_product(plane_embedding, text_embedding) >= 0.0;
}

fn dot_product(plane_embedding: &Vec<f32>, text_embedding: &Vec<f32>) -> f32 {
    if plane_embedding.len() != text_embedding.len() {
        return -1.0;
    }

    let mut sum: f32 = 0.0;

    for i in 0..plane_embedding.len() {
        sum += plane_embedding[i] * text_embedding[i];
    }

    sum
}


pub fn group_embeddings(
    embeddings: &HashMap<usize, FileEmbedding>
) -> Vec<HashMap<i16, Vec<usize>>> {
    let mut hash_tables: Vec<HashMap<i16, Vec<usize>>> = Vec::new();

    for _ in 0..NUMBER_OF_HASHTABLES {
        let hash_table = generate_hash_table(embeddings);
        hash_tables.push(hash_table);
    }

    hash_tables
}

pub async fn deduplicate_files(
    files: Vec<FileInfo>,
    model: &str
) -> Result<(HashMap<usize, FileEmbedding>, Vec<HashMap<i16, Vec<usize>>>), String> {

    let mut file_embeddings: HashMap<usize, FileEmbedding> = HashMap::new();

    let api_key: String;
    let client = Client::new();

    match get_openai_api_key() {
        Ok(key) => {
            api_key = key;
        },
        Err(_) => {
            return Err("Fail to get API key".to_string());
        }
    }

    let mut file_id: usize = 0;

    for file in &files {
        match generate_embedded_file(file_id, &file, &api_key, &client, model).await {
            Ok(file_embedding) => {
                file_embeddings.insert(file_id, file_embedding);
                file_id += 1;
            }
            Err(_) => {
                println!("Fail to generate embedding for file: {}", &file.name);
            }
        }
    }

    if let (Some(mut file1), Some(mut file2)) =
    (file_embeddings.remove(&0usize), file_embeddings.remove(&1usize))
    {
        link_files(&mut file1, &mut file2);

        file_embeddings.insert(0usize, file1);
        file_embeddings.insert(1usize, file2);
    }

    let mut sematic_locality_hashing: Vec<HashMap<i16, Vec<usize>>> = Vec::new();

    if file_embeddings.len() > 3000 {

        sematic_locality_hashing = group_embeddings(&file_embeddings);

        for table in &sematic_locality_hashing {
            for (_, grouped_files) in table {
                pairwise_deduplicate(&mut file_embeddings, grouped_files);
            }
        }

    } else {

        let v: Vec<usize> = file_embeddings.keys().cloned().collect();

        pairwise_deduplicate(&mut file_embeddings, &v);
    }

    for (key, value) in &file_embeddings {
        if *value.get_duplicate() {
            println!("{:?}", value.get_path());
        }
    }

    Ok((file_embeddings, sematic_locality_hashing))
}

#[cfg(test)]
mod tests {
    use super::*; 

    #[tokio::test]
    async fn test_deduplicate_files() {
        deduplicate_directory(&r"C:\Users\chuon\OneDrive\Desktop\Git Projects\Markdown files testing\Agentic Idea Testing".to_string()).await;
    }


    #[tokio::test]
    async fn semantic_clustering_test() {
        semantic_clustering(&r"C:\Users\chuon\OneDrive\Desktop\Git Projects\Markdown files testing\Agentic Idea Testing".to_string()).await;
    }
    
    
    #[tokio::test]
    async fn test_generate_embedding() {
        let text = "This is a text to test embedding generating";

        match get_openai_api_key() {
            Ok(key) => {
                let client = Client::new();
                let e = get_embedding_default(&client, &key, text, "text-embedding-3-small").await;
                println!("{:?}", e);
            },
            Err (_) => {
                println!("Fail to get API key");
            }
        }
    }

    #[tokio::test]
    async fn test_convert_to_wiki_link() {
        let path1 = r"C:\Users\chuon\OneDrive\Desktop\Git Projects\Markdown files testing\Agentic Idea Testing\climate change 1.md".to_string();
        let path2 = r"C:\Users\chuon\OneDrive\Desktop\Git Projects\Markdown files testing\Agentic Idea Testing\Subfolder 1\Generative AI.md".to_string();

        println!("{}", wiki_link_common_path(&path1, &path2).unwrap());

        println!("{}", convert_to_wiki_link(&wiki_link_common_path(&path1, &path2).unwrap()));
    }

    



}