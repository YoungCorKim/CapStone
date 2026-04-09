use crate::semantic_search::vault_semantic_search;
use crate::semantic_search::get_top_related_files;
use crate::semantic_search::load_neighbors;
use crate::enrich;

#[cfg(test)]
mod tests {
    use super::*; 


    use std::io::{self, Write};
    fn prompt_delete() -> bool {
        print!("Delete this file? (y/n): ");
        io::stdout().flush().unwrap(); // ensure prompt prints

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => true,
            _ => false,
        }
    }

    /*#[tokio::test]
    async fn test_deduplicate_files() {
        let dir_path = r"C:\Users\chuon\OneDrive\Desktop\Git Projects\Markdown files testing\Agentic Idea Testing".to_string();
    
        let files_to_delete = get_file_duplications(dir_path.clone()).await;
    
        println!("File duplications detected: {}.\n", files_to_delete.len());
    
        for (i, file) in files_to_delete.iter().enumerate() {
            let relative_path =
                relative_path_from_dir(dir_path.clone(), file.get_path().to_string());
    
            let frontmatter = file.frontmatter.clone();
            let content = file.content.clone();
    
            println!("=== Duplicate #{} ===", i + 1);
            println!("Path: {:?}", relative_path);
    
            if frontmatter.is_empty() {
                println!("Frontmatter: <none>\n");
            } else {
                println!("Frontmatter:\n{:?}\n", frontmatter);
            }
    
            let preview_len: usize = 500;
            let preview: String = content.chars().take(preview_len).collect();
            if content.chars().count() > preview_len {
                println!("Content (first {} chars):\n{}...\n", preview_len, preview);
            } else {
                println!("Content:\n{}\n", preview);
            }

            let result = prompt_delete();

            if result {
                match delete_file(file.get_path()) {
                    Ok(_) => println!("File deleted sucessfully!\n\n"), 
                    Err(_) => println!("Fail to delete file!\n\n"),
                }
            }
        }
    }


    #[tokio::test]
    async fn semantic_clustering_test() {
        let dir_path = r"C:\Users\chuon\OneDrive\Desktop\Git Projects\Markdown files testing\Agentic Idea Testing".to_string();
        let clusters: Result<Vec<(FileEmbedding, FileEmbedding)>, String> =
            get_semantic_file_cluster(dir_path.clone())
            .await;
    
        match clusters {
            Ok(pairs) => {
                println!("Total semantic pairs detected: {}\n", pairs.len());
    
                for (mut file1, mut file2) in pairs {
                    println!("----------------------------------");
                    println!("File A: {}", relative_path_from_dir(dir_path.clone(), file1.get_path().to_string().clone()).unwrap());
                    println!("File B: {}", relative_path_from_dir(dir_path.clone(), file2.get_path().to_string().clone()).unwrap());
    
                    print!("Link these files? (y/n): ");
                    io::stdout().flush().unwrap();
    
                    let mut input = String::new();
                    io::stdin().read_line(&mut input).unwrap();
    
                    match input.trim().to_lowercase().as_str() {
                        "y" | "yes" => {
                            link_files(&mut file1, &mut file2);
                    
                            match save_file(file1.get_path(), &file1.content, &file1.frontmatter) {
                                Ok(_) => println!("Saved {}", file1.get_path()),
                                Err(e) => println!("Failed to save {}: {}", file1.get_path(), e),
                            }
                    
                            match save_file(file2.get_path(), &file2.content, &file2.frontmatter) {
                                Ok(_) => println!("Saved {}", file2.get_path()),
                                Err(e) => println!("Failed to save {}: {}", file2.get_path(), e),
                            }
                    
                            println!("Files linked.\n");
                        }
                    
                        _ => {
                            println!("Skipped.\n");
                        }
                    }
                    println!("\n");
                }
            }
    
            Err(e) => {
                panic!("Semantic clustering failed: {}", e);
            }
        }
    }*/

    #[tokio::test]
    async fn test_load_neighbor_embeddings() {
        let result = load_neighbors(r"C:\Users\chuon\OneDrive\Desktop\Git Projects\Markdown files testing\Agentic Idea Testing\Cooking1.md");
        for file in result.unwrap() {
            println!("File: {}", file.path);
        }
    }

    #[tokio::test]
    async fn test_vault_semantic_search() {
        let dir_path = r"C:\Users\chuon\OneDrive\Desktop\Git Projects\Markdown files testing\Agentic Idea Testing".to_string();
        let result = vault_semantic_search(dir_path, "Cooking".to_string(), 10).await;
        for (file, score) in result.unwrap() {
            println!("File: {}, Score: {}", file.get_path(), score);
        }
    }

    #[tokio::test]
    async fn test_enrich_file() {
        let result = enrich::expand_file_details(r"C:\Users\chuon\OneDrive\Desktop\Git Projects\Markdown files testing\Agentic Idea Testing\Cooking2.md").await;
        match result {
            Ok(enriched) => {
                println!(
                    "{}", enriched.content
                );
            }
            Err(e) => println!("Error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_get_top_neighbors() {
        let result = get_top_related_files(3, r"C:\Users\chuon\OneDrive\Desktop\Git Projects\Markdown files testing\Agentic Idea Testing\Cooking2.md").await;
        for (file, score) in result.unwrap() {
            println!("File: {}, Score: {}", file.get_path(), score);
        }
    }

    #[tokio::test]
    async fn test_enrich_file_definitions() {
        let result = enrich::enrich_file_definitions(r"C:\Users\chuon\OneDrive\Desktop\Git Projects\Markdown files testing\Agentic Idea Testing\Cooking2.md").await;
        match result {
            Ok(enriched) => {
                println!(
                    "{}", enriched.content
                );
            }
            Err(e) => println!("Error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_enrich_file_custom() {
        let result = enrich::enrich_file_custom(r"C:\Users\chuon\OneDrive\Desktop\Git Projects\Markdown files testing\Agentic Idea Testing\Cooking2.md", "Tell me is this a good recipe for cooking a chicken?").await;
        match result {
            Ok(enriched) => {
                println!(
                    "{}", enriched.content
                );
            }
            Err(e) => println!("Error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_enrich_file_with_examples() {
        let result = enrich::enrich_file_with_examples(r"C:\Users\chuon\OneDrive\Desktop\Git Projects\Markdown files testing\Agentic Idea Testing\Cooking2.md").await;
        match result {
            Ok(enriched) => {
                println!(
                    "{}", enriched.content
                );
            }
            Err(e) => println!("Error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_detect_missing_knowledge() {
        let result = enrich::detect_missing_knowledge(r"C:\Users\chuon\OneDrive\Desktop\Git Projects\Markdown files testing\Agentic Idea Testing\Cooking2.md").await;
        match result {
            Ok(enriched) => {
                println!(
                    "{}", enriched.content
                );
            }
            Err(e) => println!("Error: {}", e),
        }
    }
}