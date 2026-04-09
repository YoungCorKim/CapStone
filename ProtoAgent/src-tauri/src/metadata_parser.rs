use std::path::{Path, PathBuf};
use std::path::Component;
use serde_yaml::Value;
use std::collections::HashMap;
use std::fs;
use regex::Regex;
use crate::FileInfo;
//use locality_sensitive_hashing_deduplicate::get_embedding;
use crate::locality_sensitive_hashing_deduplicate::FileEmbedding;
use crate::scan_markdown_files_impl;

pub struct FileContent {
    pub content: String,
    pub frontmatter: Option<String>,
}


pub fn same_file(p1: &str, p2: &str) -> std::io::Result<bool> {
    let c1 = fs::canonicalize(p1)?;
    let c2 = fs::canonicalize(p2)?;
    Ok(c1 == c2)
}

pub fn convert_to_wiki_link(wiki_path: &String) -> String {
    let file_name_without_extension = Path::new(wiki_path)
                        .file_stem()                 
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();

    format!(
        "[[{}]](<{}>)",
        file_name_without_extension,
        wiki_path.clone()
    )
}


fn extract_wikilinks(content: &str) -> Vec<String> {
    let re = Regex::new(r"\[\[([^\]]+)\]\]").unwrap();

    re.captures_iter(content)
        .map(|cap| {
            cap[1]
                .split('|')
                .next()
                .unwrap()
                .trim()
                .to_string()
        })
        .collect()
}

pub fn wiki_link_common_path(from: &String, to: &String) -> Option<String> {
    let from_path = Path::new(&from);
    let to_path = Path::new(&to);

    let from_components: Vec<_> = from_path.components().collect();
    let to_components: Vec<_> = to_path.components().collect();

    let mut i = 0;
    while i < from_components.len()
        && i < to_components.len()
        && from_components[i] == to_components[i]
    {
        i += 1;
    }
    let mut result = String::new();

    for comp in &to_components[i..] {
        if let Component::Normal(os) = comp {
            if !result.is_empty() {
                result.push('/');
            }
            result.push_str(&os.to_string_lossy());
        }
    }

    Some(result)
}

pub fn get_vault_path(file_path: &Path, vault_root: &Path) -> Option<PathBuf> {
    if file_path.starts_with(vault_root) {
        Some(vault_root.to_path_buf())
    } else {
        None
    }
}

pub fn extract_frontmatter(content: &str) -> FileContent {
    let trimmed = content.trim_start();

    //println!("---------------------------\n{:?}\n\n", content);
    
    if trimmed.starts_with("---") {
        if let Some(end) = trimmed[4..].find("---") {
            let fm_start = 4;
            let fm_end = fm_start + end;

            let frontmatter = trimmed[fm_start..fm_end].to_string();
            let body = trimmed[fm_end + 3..].trim_start().to_string();

            //println!("Found frontmatter {:?}------------------------------------------\n\n", frontmatter);

            return FileContent {
                content: body,
                frontmatter: Some(frontmatter),
            };
        }
    }

    FileContent {
        content: content.to_string(),
        frontmatter: None,
    }
}

pub fn relative_path_from_dir(
    base_dir: String,
    file_path: String,
) -> Result<String, String> {
    let base = Path::new(&base_dir);
    let file = Path::new(&file_path);

    file.strip_prefix(base)
        .map_err(|_| {
            format!(
                "File '{}' is not inside directory '{}'",
                file_path, base_dir
            )
        })?
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Invalid UTF-8 path".to_string())
}



pub fn normalize_frontmatter_file_links(frontmatter: &mut HashMap<String, Value>) {
    if let Some(value) = frontmatter.get_mut("related") {
        if let Value::String(s) = value {

            let links: Vec<String> = s
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            *value = Value::Sequence(
                links.into_iter().map(Value::String).collect()
            );

            return;
        }
    } 
    frontmatter.insert(
        "related".to_string(),
        Value::Sequence(vec![
            Value::String("Example".to_string())
        ])
    );
}

 
pub fn normalize_frontmatter_link(frontmatter: &String) -> String {
    let length = frontmatter.len();

    let bytes = frontmatter.as_bytes();

    let mut normalize_frontmatter = String::new();

    let mut i = 0;

    while i < length {
        if bytes[i] == b'[' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'[' {
                let mut open_quotation_index: i8 = - 1;
                if i > 0 && bytes[i - 1] == b'"'{
                    continue;
                } else {
                    normalize_frontmatter.push('"' as char);

                    open_quotation_index = (normalize_frontmatter.len() as i8) - 1;
                }
                while i < bytes.len() {
                    normalize_frontmatter.push(bytes[i] as char);
                    if bytes[i] == b']' {
                        i+=1; 
                        if i < bytes.len() && bytes[i] == b']' {
                            normalize_frontmatter.push(bytes[i] as char);
                            if i + 1  >= bytes.len() || bytes[i + 1] == b'"' {
                                if i + 1 >= bytes.len() && open_quotation_index > 0{
                                    println!("Remove");
                                    normalize_frontmatter.remove(open_quotation_index as usize);
                                }
                                break;
                            } else {
                                normalize_frontmatter.push('"' as char);
                                break;
                            }
                        }
                    }
                    i+=1;
                }
            } else {
                normalize_frontmatter.push(bytes[i] as char);
            }
        } 
        else {
            normalize_frontmatter.push(bytes[i] as char);
        }
        i+=1;
    }
    normalize_frontmatter
}
