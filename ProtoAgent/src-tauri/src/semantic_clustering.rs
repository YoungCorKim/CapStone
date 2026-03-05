use std::collections::{HashMap, HashSet};
use serde_yaml::Value;
use crate::metadata_parser::wiki_link_common_path;
use crate::metadata_parser::normalize_frontmatter_file_links;
use crate::metadata_parser::convert_to_wiki_link;
use crate::save_file;

use crate::locality_sensitive_hashing_deduplicate::{
    deduplicate_directory, group_embeddings, FileEmbedding,
};
use crate::pairwise_deduplicate::cosine_similarity;

static THRESHOLD: f32 = 0.65;

pub async fn semantic_clustering(
    directory_path: &String,
) -> Result<(HashMap<usize, HashSet<usize>>, HashMap<usize, FileEmbedding>), String> {
    match deduplicate_directory(directory_path).await {
        Ok((mut file_embeddings, semantic_locality_hashing)) => {
            let (cluster_map) = semantic_clustering_from_file_embeddings(
                &mut file_embeddings,
                &semantic_locality_hashing,
            );

            Ok((cluster_map, file_embeddings))
        }
        Err(e) => Err(e),
    }
}

fn cluster_helper(
    semantic_locality_hashing: &Vec<HashMap<i16, Vec<usize>>>,
    file_embeddings: &HashMap<usize, FileEmbedding>,
) -> HashMap<usize, HashSet<usize>> {
    let mut clusters: HashMap<usize, HashSet<usize>> = HashMap::new();

    for table in semantic_locality_hashing {
        for (_bucket, grouped_files) in table {
            for i in 0..grouped_files.len() {
                let id_1 = grouped_files[i];
                let Some(file_1) = file_embeddings.get(&id_1) else {
                    continue;
                };

                let embedding_1 = file_1.get_embeddings();

                for j in (i + 1)..grouped_files.len() {
                    let id_2 = grouped_files[j];
                    let Some(file_2) = file_embeddings.get(&id_2) else {
                        continue;
                    };

                    let embedding_2 = file_2.get_embeddings();
                    let similarity = cosine_similarity(embedding_1, embedding_2);

                    if similarity >= THRESHOLD {
                        /*println!("Cluster:");
                        println!("{:?}", file_1.get_path());
                        println!("{:?}", file_2.get_path());*/
                        clusters
                            .entry(id_1)
                            .or_insert_with(HashSet::new)
                            .insert(id_2);
                        clusters
                            .entry(id_2)
                            .or_insert_with(HashSet::new)
                            .insert(id_1);
                    }
                }
            }
        }
    }

    clusters
}

pub fn semantic_clustering_from_file_embeddings<'a>(
    file_embeddings: &mut HashMap<usize, FileEmbedding>,
    semantic_locality_hashing: &Vec<HashMap<i16, Vec<usize>>>,
) -> (HashMap<usize, HashSet<usize>>) {
    let mut cluster_map: HashMap<usize, HashSet<usize>> = HashMap::new();

    if semantic_locality_hashing.is_empty() {
        let semantic_grouping = group_embeddings(file_embeddings);

        if semantic_grouping.is_empty() {
            return cluster_map;
        }

        cluster_map = cluster_helper(&semantic_grouping, file_embeddings);
    } else {
        cluster_map = cluster_helper(semantic_locality_hashing, file_embeddings);
    }

    /*link_grouped_file(&cluster_map,  file_embeddings);

    for (_id, file) in file_embeddings.iter_mut() {
        let content: String = file.get_content().to_string();
        let path: String = file.get_path().to_string();
        let fm = file.get_frontmatter().clone();
        save_file(&path, &content, &fm);
    }*/

    cluster_map
}

fn link_grouped_file(
    file_clusters: &HashMap<usize, HashSet<usize>>,
    master_list: &mut HashMap<usize, FileEmbedding>,
) {
    let mut pairs: Vec<(usize, usize)> = Vec::new();

    for (file_id, cluster) in file_clusters {
        for other_file_id in cluster {
            if file_id == other_file_id {
                continue;
            }
            pairs.push((*file_id, *other_file_id));
        }
    }

    for (id_1, id_2) in pairs {
        if id_1 == id_2 {
            continue;
        }

        let file_1 = master_list.remove(&id_1);
        let file_2 = master_list.remove(&id_2);

        let (mut file_1, mut file_2) = match (file_1, file_2) {
            (Some(f1), Some(f2)) => (f1, f2),
            (maybe_f1, maybe_f2) => {
                if let Some(f1) = maybe_f1 {
                    master_list.insert(id_1, f1);
                }
                if let Some(f2) = maybe_f2 {
                    master_list.insert(id_2, f2);
                }
                continue;
            }
        };

        link_files(&mut file_1, &mut file_2);

        master_list.insert(id_1, file_1);

        master_list.insert(id_2, file_2);
    }
}

fn link_helper(frontmatter: &mut HashMap<String, Value>, wiki_link: String) {
    if !frontmatter.contains_key("related") {
        frontmatter.insert("related".to_string(), Value::Sequence(vec![]));
    }

    let links = frontmatter.get_mut("related").unwrap();

    match links {
        Value::Sequence(seq) => {
            let link_value = Value::String(wiki_link);
    
            if !seq.contains(&link_value) {
                seq.push(link_value);
            }
        }
        _ => {
            println!("Not a sequence");
            normalize_frontmatter_file_links(frontmatter);
        }
    }
    
}

pub fn link_files(file_1: &mut FileEmbedding, file_2: &mut FileEmbedding) -> bool {
    let path1 = file_1.get_path(); // assume &str or &String
    let path2 = file_2.get_path();

    let relative_path_1 = match wiki_link_common_path(&path2.to_string(), &path1.to_string()) {
        Some(p) => p,
        None => {
            println!("wiki_link_common_path failed for file2 -> file1: {} -> {}", path2, path1);
            return false;
        }
    };

    let relative_path_2 = match wiki_link_common_path(&path1.to_string(), &path2.to_string()) {
        Some(p) => p,
        None => {
            println!("wiki_link_common_path failed for file1 -> file2: {} -> {}", path1, path2);
            return false;
        }
    };

    let file_1_wiki_link = convert_to_wiki_link(&relative_path_1);

    let file_2_wiki_link = convert_to_wiki_link(&relative_path_2);

    let frontmatter1 = file_1.get_frontmatter();

    let frontmatter2 = file_2.get_frontmatter();

    link_helper( frontmatter1, file_2_wiki_link);

    link_helper( frontmatter2, file_1_wiki_link);

    true
}

