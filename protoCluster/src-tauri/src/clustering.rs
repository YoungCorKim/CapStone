use crate::markdown::MarkdownFile;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, serde::Serialize)]
pub struct Cluster {
    pub id: usize,
    pub name: String,
    pub shared_tags: Vec<String>,
    pub files: Vec<MarkdownFile>,
    pub similarity_score: f64,
}

impl Cluster {
    pub fn new(id: usize, name: String) -> Self {
        Self {
            id,
            name,
            shared_tags: Vec::new(),
            files: Vec::new(),
            similarity_score: 0.0,
        }
    }
}

pub fn cluster_files(files: Vec<MarkdownFile>, min_similarity: f64) -> Vec<Cluster> {
    if files.is_empty() {
        return Vec::new();
    }

    // Use a simpler but more effective clustering approach
    let mut clusters: Vec<Cluster> = Vec::new();
    let mut file_assignments: Vec<Option<usize>> = vec![None; files.len()];

    for (file_idx, file) in files.iter().enumerate() {
        if file_assignments[file_idx].is_some() {
            continue; // Already assigned to a cluster
        }

        let mut best_cluster_idx = None;
        let mut best_similarity = 0.0;

        // Check if this file can join any existing cluster
        for (cluster_idx, cluster) in clusters.iter().enumerate() {
            let file_tags: HashSet<String> = file.all_tags.iter().cloned().collect();
            let cluster_tags: HashSet<String> = cluster.shared_tags.iter().cloned().collect();
            let similarity = calculate_similarity(&file_tags, &cluster_tags);

            if similarity >= min_similarity && similarity > best_similarity {
                best_cluster_idx = Some(cluster_idx);
                best_similarity = similarity;
            }
        }

        if let Some(cluster_idx) = best_cluster_idx {
            // Add file to existing cluster
            clusters[cluster_idx].files.push(file.clone());
            file_assignments[file_idx] = Some(cluster_idx);
            
            // Recalculate cluster properties
            clusters[cluster_idx].shared_tags = calculate_shared_tags(&clusters[cluster_idx].files);
            clusters[cluster_idx].similarity_score = calculate_cluster_similarity(&clusters[cluster_idx].files);
            clusters[cluster_idx].name = generate_cluster_name(&clusters[cluster_idx].shared_tags, &clusters[cluster_idx].files);
        } else {
            // Create new cluster for this file
            let mut new_cluster = Cluster::new(clusters.len(), format!("Cluster {}", clusters.len() + 1));
            new_cluster.files.push(file.clone());
            new_cluster.shared_tags = file.all_tags.clone();
            new_cluster.similarity_score = 1.0;
            new_cluster.name = generate_cluster_name(&new_cluster.shared_tags, &new_cluster.files);
            
            file_assignments[file_idx] = Some(clusters.len());
            clusters.push(new_cluster);
        }
    }

    // Sort clusters by size (largest first) and then by similarity
    clusters.sort_by(|a, b| {
        b.files.len().cmp(&a.files.len())
            .then(b.similarity_score.partial_cmp(&a.similarity_score).unwrap_or(std::cmp::Ordering::Equal))
    });

    clusters
}

fn calculate_similarity(tags1: &HashSet<String>, tags2: &HashSet<String>) -> f64 {
    if tags1.is_empty() && tags2.is_empty() {
        return 0.0;
    }

    let intersection: HashSet<_> = tags1.intersection(tags2).collect();
    let union: HashSet<_> = tags1.union(tags2).collect();

    if union.is_empty() {
        return 0.0;
    }

    // Use Jaccard similarity but also give bonus for any overlap
    let jaccard_similarity = intersection.len() as f64 / union.len() as f64;
    
    // Give bonus points for having any shared tags (even if small overlap)
    let overlap_bonus = if intersection.len() > 0 { 0.2 } else { 0.0 };
    
    // Cap the result at 1.0
    (jaccard_similarity + overlap_bonus).min(1.0)
}

fn calculate_shared_tags(files: &[MarkdownFile]) -> Vec<String> {
    if files.is_empty() {
        return Vec::new();
    }

    if files.len() == 1 {
        return files[0].all_tags.clone();
    }

    // Find tags that appear in all files
    let mut shared_tags: HashSet<String> = files[0].all_tags.iter().cloned().collect();
    
    for file in files.iter().skip(1) {
        let file_tags: HashSet<String> = file.all_tags.iter().cloned().collect();
        shared_tags = shared_tags.intersection(&file_tags).cloned().collect();
    }

    // If no tags are shared by all files, find the most common tags
    if shared_tags.is_empty() {
        let mut tag_counts: HashMap<String, usize> = HashMap::new();
        
        for file in files {
            for tag in &file.all_tags {
                *tag_counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }

        // Get tags that appear in at least half of the files
        let threshold = (files.len() + 1) / 2;
        shared_tags = tag_counts
            .into_iter()
            .filter(|(_, count)| *count >= threshold)
            .map(|(tag, _)| tag)
            .collect();
    }

    let mut result: Vec<String> = shared_tags.into_iter().collect();
    result.sort();
    result
}

fn calculate_cluster_similarity(files: &[MarkdownFile]) -> f64 {
    if files.len() <= 1 {
        return 1.0;
    }

    let mut total_similarity = 0.0;
    let mut comparisons = 0;

    for i in 0..files.len() {
        for j in (i + 1)..files.len() {
            let tags1: HashSet<String> = files[i].all_tags.iter().cloned().collect();
            let tags2: HashSet<String> = files[j].all_tags.iter().cloned().collect();
            total_similarity += calculate_similarity(&tags1, &tags2);
            comparisons += 1;
        }
    }

    if comparisons > 0 {
        total_similarity / comparisons as f64
    } else {
        1.0
    }
}

fn generate_cluster_name(shared_tags: &[String], files: &[MarkdownFile]) -> String {
    if !shared_tags.is_empty() {
        // Filter out generic words and prioritize topic-specific tags
        let generic_words = get_generic_words_for_naming();
        let topic_tags: Vec<&String> = shared_tags
            .iter()
            .filter(|tag| !generic_words.contains(&tag.to_lowercase()))
            .collect();
        
        if !topic_tags.is_empty() {
            // Use the most common topic-specific tag
            let mut tag_counts: HashMap<String, usize> = HashMap::new();
            
            for file in files {
                for tag in &file.all_tags {
                    if topic_tags.contains(&tag) {
                        *tag_counts.entry(tag.clone()).or_insert(0) += 1;
                    }
                }
            }

            if let Some((most_common_tag, _)) = tag_counts.iter().max_by_key(|(_, count)| *count) {
                return most_common_tag.clone();
            }
        }
        
        // If no topic-specific tags, use the most common shared tag
        let mut tag_counts: HashMap<String, usize> = HashMap::new();
        
        for file in files {
            for tag in &file.all_tags {
                if shared_tags.contains(tag) {
                    *tag_counts.entry(tag.clone()).or_insert(0) += 1;
                }
            }
        }

        if let Some((most_common_tag, _)) = tag_counts.iter().max_by_key(|(_, count)| *count) {
            return most_common_tag.clone();
        }
    }

    // Fallback: try to extract topic from file titles
    if let Some(first_file) = files.first() {
        if let Some(title) = &first_file.title {
            // Extract meaningful words from title
            let words: Vec<&str> = title.split_whitespace().collect();
            let topic_words: Vec<&str> = words
                .iter()
                .filter(|word| {
                    let lower = word.to_lowercase();
                    !get_generic_words_for_naming().contains(&lower) && word.len() > 2
                })
                .map(|s| *s)
                .collect();
            
            if !topic_words.is_empty() {
                return format!("{} & {} more", topic_words.join(" "), files.len() - 1);
            }
            
            return format!("{} & {} more", title, files.len() - 1);
        }
    }

    format!("Cluster of {} files", files.len())
}

fn get_generic_words_for_naming() -> std::collections::HashSet<String> {
    let generic_words = [
        "note", "notes", "idea", "ideas", "thought", "thoughts", "day", "time", "way", "ways",
        "thing", "things", "stuff", "item", "items", "point", "points", "tip", "tips", "hint",
        "hints", "example", "examples", "case", "cases", "type", "types", "kind", "kinds",
        "sort", "sorts", "form", "forms", "part", "parts", "piece", "pieces", "bit", "bits",
        "section", "sections", "area", "areas", "topic", "topics", "subject", "subjects",
        "matter", "matters", "issue", "issues", "problem", "problems", "question", "questions",
        "answer", "answers", "solution", "solutions", "method", "methods", "approach", "approaches",
        "technique", "techniques", "strategy", "strategies", "plan", "plans", "step", "steps",
        "process", "processes", "system", "systems", "tool", "tools", "resource", "resources",
        "information", "data", "content", "text", "document", "documents", "file", "files",
        "page", "pages", "post", "posts", "article", "articles", "entry", "entries", "record",
        "records", "list", "lists", "set", "sets", "group", "groups", "collection", "collections",
        "series", "sequence", "chain", "chains", "link", "links", "connection", "connections",
        "relation", "relations", "relationship", "relationships", "association", "associations",
        "pattern", "patterns", "structure", "structures", "format", "formats", "style", "styles",
        "design", "designs", "model", "models", "template", "templates", "version", "versions",
        "update", "updates", "change", "changes", "modification", "modifications", "improvement",
        "improvements", "enhancement", "enhancements", "feature", "features", "function",
        "functions", "capability", "capabilities", "option", "options", "choice", "choices",
        "alternative", "alternatives", "preference", "preferences", "setting", "settings",
        "configuration", "configurations", "parameter", "parameters", "variable", "variables",
        "value", "values", "amount", "amounts", "number", "numbers", "count", "counts", "total",
        "totals", "sum", "sums", "result", "results", "outcome", "outcomes", "effect", "effects",
        "impact", "impacts", "influence", "influences", "benefit", "benefits", "advantage",
        "advantages", "disadvantage", "disadvantages", "pro", "cons", "pros", "positive",
        "negatives", "good", "bad", "better", "worse", "best", "worst", "important", "unimportant",
        "significant", "insignificant", "relevant", "irrelevant", "useful", "useless", "helpful",
        "unhelpful", "effective", "ineffective", "efficient", "inefficient", "successful",
        "unsuccessful", "working", "broken", "fixed", "complete", "incomplete", "finished",
        "unfinished", "done", "undone", "ready", "unready", "available", "unavailable",
        "possible", "impossible", "likely", "unlikely", "certain", "uncertain", "sure", "unsure",
        "clear", "unclear", "obvious", "unobvious", "simple", "complex", "easy", "difficult",
        "hard", "soft", "strong", "weak", "fast", "slow", "quick", "quickly", "slowly", "early",
        "late", "soon", "later", "now", "then", "here", "there", "where", "when", "why", "how",
        "what", "who", "which", "whose", "whom", "wherever", "whenever", "however", "whatever",
        "whoever", "whichever", "somewhere", "anywhere", "everywhere", "nowhere", "sometime",
        "anytime", "everytime", "never", "always", "often", "sometimes", "rarely", "seldom",
        "usually", "normally", "typically", "generally", "commonly", "frequently", "occasionally",
        "regularly", "constantly", "continuously", "permanently", "temporarily", "briefly",
        "quickly", "slowly", "gradually", "suddenly", "immediately", "instantly", "eventually",
        "finally", "ultimately", "basically", "essentially", "fundamentally", "primarily",
        "mainly", "mostly", "largely", "partly", "partially", "completely", "entirely", "totally",
        "fully", "absolutely", "relatively", "comparatively", "approximately", "roughly",
        "exactly", "precisely", "accurately", "correctly", "properly", "rightly", "wrongly",
        "incorrectly", "improperly", "badly", "well", "good", "great", "excellent", "outstanding",
        "amazing", "wonderful", "fantastic", "terrible", "awful", "horrible", "disgusting",
        "beautiful", "ugly", "pretty", "handsome", "attractive", "unattractive", "interesting",
        "boring", "exciting", "dull", "fun", "funny", "serious", "silly", "stupid", "smart",
        "intelligent", "clever", "wise", "foolish", "crazy", "sane", "normal", "abnormal",
        "strange", "weird", "odd", "unusual", "common", "rare", "special", "ordinary", "regular",
        "standard", "typical", "unique", "different", "same", "similar", "alike", "unlike",
        "opposite", "contrary", "reverse", "backward", "forward", "ahead", "behind", "before",
        "after", "during", "while", "since", "until", "unless", "if", "unless", "although",
        "though", "however", "but", "yet", "still", "nevertheless", "nonetheless", "moreover",
        "furthermore", "additionally", "also", "too", "as", "well", "either", "neither", "both",
        "all", "every", "each", "some", "any", "no", "none", "nothing", "something", "anything",
        "everything", "someone", "anyone", "everyone", "nobody", "somebody", "anybody",
        "everybody", "somewhere", "anywhere", "everywhere", "nowhere", "sometime", "anytime",
        "everytime", "never", "always", "often", "sometimes", "rarely", "seldom", "usually",
        "normally", "typically", "and", "or", "but", "in", "on", "at", "to", "for", "of", "with",
        "by", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had", "do",
        "does", "did", "will", "would", "could", "should", "may", "might", "must", "can"
    ];
    
    generic_words.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::MarkdownFile;

    fn create_test_file(path: &str, tags: Vec<String>) -> MarkdownFile {
        MarkdownFile {
            path: path.to_string(),
            title: Some(path.to_string()),
            frontmatter_tags: Vec::new(),
            hashtags: Vec::new(),
            extracted_keywords: Vec::new(),
            all_tags: tags,
        }
    }

    #[test]
    fn test_similarity_calculation() {
        let tags1: HashSet<String> = ["rust", "programming"].iter().cloned().collect();
        let tags2: HashSet<String> = ["rust", "systems"].iter().cloned().collect();
        
        let similarity = calculate_similarity(&tags1, &tags2);
        assert!((similarity - 0.333).abs() < 0.01); // 1/3 similarity
    }

    #[test]
    fn test_clustering() {
        let files = vec![
            create_test_file("file1.md", vec!["rust".to_string(), "programming".to_string()]),
            create_test_file("file2.md", vec!["rust".to_string(), "systems".to_string()]),
            create_test_file("file3.md", vec!["python".to_string(), "data".to_string()]),
        ];

        let clusters = cluster_files(files, 0.3);
        assert_eq!(clusters.len(), 2); // Should create 2 clusters
    }
}
