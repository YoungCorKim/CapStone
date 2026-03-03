use crate::{MasterSummary, locality_sensitive_hashing_deduplicate::FileEmbedding};
use serde_yaml::Value;
use std::collections::HashMap;
use chrono::Utc;


static THRESHOLD: f32 = 0.85;


fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }
    dot / (mag_a * mag_b)
}

fn merge_values(target: &mut Value, incoming: Value) {
    match (target, incoming) {

        (Value::Mapping(map1), Value::Mapping(map2)) => {
            for (key, value2) in map2 {
                match map1.get_mut(&key) {
                    Some(value1) => merge_values(value1, value2),
                    None => {
                        map1.insert(key, value2);
                    }
                }
            }
        }

        (Value::Sequence(seq1), Value::Sequence(seq2)) => {
            for item in seq2 {
                if !seq1.contains(&item) {
                    seq1.push(item);
                }
            }
        }

        (value1, Value::Sequence(mut seq)) => {
            if !seq.contains(value1) {
                seq.push(value1.clone());
            }
            *value1 = Value::Sequence(seq);
        }

        (Value::Sequence(seq1), scalar) => {
            if !seq1.contains(&scalar) {
                seq1.push(scalar);
            }
        }

        (value1, value2) => {
            if value1 != &value2 {
                *value1 = Value::Sequence(vec![value1.clone(), value2]);
            }
        }
    }
}

pub fn merge_frontmatter(fm_1: &mut HashMap<String, serde_yaml::Value>, fm_2: &HashMap<String, serde_yaml::Value>) {
    if fm_2.is_empty() { return; }

    for (key, fm_2_value) in fm_2 {
        match fm_1.get_mut(key) {
            Some (fm_1_value) => {
                if key == "created" {
                    if let (Value::String(created_date_1), Value::String(created_date_2)) =
                        (fm_1_value.clone(), fm_2_value.clone())
                    {
                        if created_date_2 < created_date_1 {
                            *fm_1_value = Value::String(created_date_2);
                        }
                    }
                } else {
                    merge_values(fm_1_value, fm_2_value.clone());
                }
            } None => {
                fm_1.insert(key.clone(), fm_2_value.clone());

            }
        }
    }

    let today = Utc::now().date_naive().to_string();
    fm_1.insert("updated".to_string(), Value::String(today));
}

pub fn pairwise_deduplicate(master_file_list: &mut Vec<FileEmbedding>, to_deduplicate_list: &Vec<usize>) -> Result<bool, String> {
    let empty_string = "".to_string();

    let mut duplicate_tracker: HashMap<usize, usize> = HashMap::new();

    for i in 0..to_deduplicate_list.len() {

        let id_1 = to_deduplicate_list[i];

        let file_1 = &master_file_list[id_1];

        if *file_1.get_duplicate() { continue; }

        let embedding_file_1 = &file_1.get_embeddings();

        for j in (i + 1)..to_deduplicate_list.len() {

            let id_2 = to_deduplicate_list[j];

            let file_2 = &master_file_list[id_2];

            if *file_2.get_duplicate() { continue; }

            let embedding_file_2 = &file_2.get_embeddings();
            
            let similarity = cosine_similarity( embedding_file_1, embedding_file_2);

            if similarity >= THRESHOLD {
                let id_1 = file_1.get_id().clone();

                let id_2 = file_2.get_id().clone();

                duplicate_tracker.insert(id_2, id_1);
            }
        }


    }

    for (remove_id, keep_id) in duplicate_tracker {

        if remove_id == keep_id {
            continue;
        }

        if *master_file_list[remove_id].get_duplicate() {
            continue;
        } else if *master_file_list[keep_id].get_duplicate() {
            master_file_list[remove_id].set_duplicate();
            continue;
        }


        let (keep_file, remove_file) = if keep_id < remove_id {
            let (left, right) = master_file_list.split_at_mut(remove_id);
        (&mut left[keep_id], &mut right[0])
        } else {
            let (left, right) = master_file_list.split_at_mut(keep_id);
            (&mut right[0], &mut left[remove_id])
        };

        let keep_file_frontmatter = keep_file.get_frontmatter();

        let delete_file_frontmatter = remove_file.get_frontmatter();

        merge_frontmatter(keep_file_frontmatter, delete_file_frontmatter);

        remove_file.set_duplicate();
    }

    Ok(true)
}