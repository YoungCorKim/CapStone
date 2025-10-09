use pulldown_cmark::{Parser, Event};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use yaml_rust::YamlLoader;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MarkdownFile {
    pub path: String,
    pub title: Option<String>,
    pub frontmatter_tags: Vec<String>,
    pub hashtags: Vec<String>,
    pub extracted_keywords: Vec<String>,
    pub all_tags: Vec<String>, // Combined tags and keywords for clustering
}

impl MarkdownFile {
    pub fn new(path: String) -> Self {
        Self {
            path,
            title: None,
            frontmatter_tags: Vec::new(),
            hashtags: Vec::new(),
            extracted_keywords: Vec::new(),
            all_tags: Vec::new(),
        }
    }
}

pub fn parse_markdown_file(file_path: &str) -> Result<MarkdownFile, String> {
    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read file {}: {}", file_path, e))?;

    let mut file = MarkdownFile::new(file_path.to_string());
    
    // Extract frontmatter
    if let Some((frontmatter, _content)) = extract_frontmatter(&content) {
        file.frontmatter_tags = extract_frontmatter_tags(&frontmatter);
    }
    
    // Extract hashtags from content
    file.hashtags = extract_hashtags(&content);
    
    // Extract keywords from content if no explicit tags
    if file.frontmatter_tags.is_empty() && file.hashtags.is_empty() {
        file.extracted_keywords = extract_keywords(&content);
    }
    
    // Extract title from content
    file.title = extract_title(&content);
    
    // Combine all tags for clustering
    file.all_tags = combine_tags(&file.frontmatter_tags, &file.hashtags, &file.extracted_keywords);
    
    Ok(file)
}

fn extract_frontmatter(content: &str) -> Option<(String, String)> {
    if content.starts_with("---\n") {
        if let Some(end_pos) = content.find("\n---\n") {
            let frontmatter = &content[4..end_pos];
            let remaining_content = &content[end_pos + 5..];
            return Some((frontmatter.to_string(), remaining_content.to_string()));
        }
    }
    None
}

fn extract_frontmatter_tags(frontmatter: &str) -> Vec<String> {
    let docs = match YamlLoader::load_from_str(frontmatter) {
        Ok(docs) => docs,
        Err(_) => return Vec::new(),
    };
    
    if docs.is_empty() {
        return Vec::new();
    }
    
    let doc = &docs[0];
    
    // Look for tags field
    if let Some(tags) = doc["tags"].as_vec() {
        return tags.iter()
            .filter_map(|tag| tag.as_str())
            .map(|s| s.to_string())
            .collect();
    }
    
    // Also check for tag (singular)
    if let Some(tag) = doc["tag"].as_str() {
        return vec![tag.to_string()];
    }
    
    Vec::new()
}

fn extract_hashtags(content: &str) -> Vec<String> {
    let hashtag_regex = Regex::new(r"#([a-zA-Z0-9_-]+)").unwrap();
    let mut hashtags = Vec::new();
    
    for cap in hashtag_regex.captures_iter(content) {
        if let Some(tag) = cap.get(1) {
            hashtags.push(tag.as_str().to_string());
        }
    }
    
    hashtags
}

fn extract_keywords(content: &str) -> Vec<String> {
    // Extract text from markdown
    let parser = Parser::new(content);
    let mut text = String::new();
    
    for event in parser {
        match event {
            Event::Text(text_content) => {
                text.push_str(&text_content);
                text.push(' ');
            }
            Event::Code(code) => {
                text.push_str(&code);
                text.push(' ');
            }
            _ => {}
        }
    }
    
    // Extract headers for high-priority keywords
    let header_regex = Regex::new(r"^#{1,6}\s+(.+)$").unwrap();
    let mut header_keywords = Vec::new();
    
    for line in content.lines() {
        if let Some(cap) = header_regex.captures(line) {
            if let Some(header_text) = cap.get(1) {
                let words = extract_words(header_text.as_str());
                header_keywords.extend(words);
            }
        }
    }
    
    // Extract all words from content
    let all_words = extract_words(&text);
    
    // Filter out stop words and generic words
    let stop_words = get_stop_words();
    let generic_words = get_generic_words();
    let filtered_words: Vec<String> = all_words
        .into_iter()
        .filter(|word| {
            let lower = word.to_lowercase();
            !stop_words.contains(&lower) && !generic_words.contains(&lower)
        })
        .collect();
    
    // Count word frequency
    let mut word_counts: HashMap<String, usize> = HashMap::new();
    for word in filtered_words {
        *word_counts.entry(word.to_lowercase()).or_insert(0) += 1;
    }
    
    // Add header keywords with much higher weight (they're most important)
    for word in header_keywords {
        let lower = word.to_lowercase();
        if !stop_words.contains(&lower) && !generic_words.contains(&lower) {
            *word_counts.entry(lower).or_insert(0) += 10; // Much higher weight for headers
        }
    }
    
    // Sort by frequency and take top keywords
    let mut word_freq: Vec<(String, usize)> = word_counts.into_iter().collect();
    word_freq.sort_by(|a, b| b.1.cmp(&a.1));
    
    word_freq
        .into_iter()
        .take(8) // Top 8 keywords (reduced to focus on most important)
        .map(|(word, _)| word)
        .collect()
}

fn extract_words(text: &str) -> Vec<String> {
    let word_regex = Regex::new(r"\b[a-zA-Z]{3,}\b").unwrap();
    word_regex
        .find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect()
}

fn extract_title(content: &str) -> Option<String> {
    // Look for first H1 header
    let header_regex = Regex::new(r"^#\s+(.+)$").unwrap();
    
    for line in content.lines() {
        if let Some(cap) = header_regex.captures(line) {
            if let Some(title) = cap.get(1) {
                return Some(title.as_str().trim().to_string());
            }
        }
    }
    
    None
}

fn combine_tags(frontmatter_tags: &[String], hashtags: &[String], keywords: &[String]) -> Vec<String> {
    let mut all_tags = Vec::new();
    
    // Add frontmatter tags (highest priority)
    all_tags.extend(frontmatter_tags.iter().cloned());
    
    // Add hashtags (medium priority)
    all_tags.extend(hashtags.iter().cloned());
    
    // Add extracted keywords (lowest priority)
    all_tags.extend(keywords.iter().cloned());
    
    // Remove duplicates while preserving order
    let mut seen = std::collections::HashSet::new();
    all_tags.retain(|tag| seen.insert(tag.to_lowercase()));
    
    all_tags
}

fn get_stop_words() -> std::collections::HashSet<String> {
    let stop_words = [
        "the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for", "of", "with", "by",
        "is", "are", "was", "were", "be", "been", "being", "have", "has", "had", "do", "does", "did",
        "will", "would", "could", "should", "may", "might", "must", "can", "this", "that", "these",
        "those", "i", "you", "he", "she", "it", "we", "they", "me", "him", "her", "us", "them",
        "my", "your", "his", "her", "its", "our", "their", "mine", "yours", "hers", "ours", "theirs",
        "am", "are", "is", "was", "were", "be", "been", "being", "have", "has", "had", "having",
        "do", "does", "did", "doing", "will", "would", "could", "should", "may", "might", "must",
        "can", "shall", "ought", "need", "dare", "used", "get", "got", "gotten", "getting",
        "make", "made", "making", "take", "took", "taken", "taking", "come", "came", "coming",
        "go", "went", "gone", "going", "see", "saw", "seen", "seeing", "know", "knew", "known",
        "knowing", "think", "thought", "thinking", "look", "looked", "looking", "want", "wanted",
        "wanting", "give", "gave", "given", "giving", "use", "used", "using", "find", "found",
        "finding", "tell", "told", "telling", "ask", "asked", "asking", "work", "worked", "working",
        "seem", "seemed", "seeming", "feel", "felt", "feeling", "try", "tried", "trying", "leave",
        "left", "leaving", "call", "called", "calling", "move", "moved", "moving", "play", "played",
        "playing", "turn", "turned", "turning", "start", "started", "starting", "show", "showed",
        "shown", "showing", "hear", "heard", "hearing", "let", "letting", "put", "putting", "keep",
        "kept", "keeping", "begin", "began", "begun", "beginning", "help", "helped", "helping",
        "talk", "talked", "talking", "run", "ran", "running", "walk", "walked", "walking", "live",
        "lived", "living", "believe", "believed", "believing", "hold", "held", "holding", "bring",
        "brought", "bringing", "happen", "happened", "happening", "write", "wrote", "written",
        "writing", "sit", "sat", "sitting", "stand", "stood", "standing", "lose", "lost", "losing",
        "pay", "paid", "paying", "meet", "met", "meeting", "include", "included", "including",
        "continue", "continued", "continuing", "set", "setting", "learn", "learned", "learning",
        "change", "changed", "changing", "lead", "led", "leading", "understand", "understood",
        "understanding", "watch", "watched", "watching", "follow", "followed", "following",
        "stop", "stopped", "stopping", "create", "created", "creating", "speak", "spoke", "spoken",
        "speaking", "read", "reading", "allow", "allowed", "allowing", "add", "added", "adding",
        "spend", "spent", "spending", "grow", "grew", "grown", "growing", "open", "opened",
        "opening", "walk", "walked", "walking", "win", "won", "winning", "offer", "offered",
        "offering", "remember", "remembered", "remembering", "love", "loved", "loving", "consider",
        "considered", "considering", "appear", "appeared", "appearing", "buy", "bought", "buying",
        "wait", "waited", "waiting", "serve", "served", "serving", "die", "died", "dying", "send",
        "sent", "sending", "expect", "expected", "expecting", "build", "built", "building", "stay",
        "stayed", "staying", "fall", "fell", "fallen", "falling", "cut", "cutting", "reach",
        "reached", "reaching", "kill", "killed", "killing", "remain", "remained", "remaining"
    ];
    
    stop_words.iter().map(|s| s.to_string()).collect()
}

fn get_generic_words() -> std::collections::HashSet<String> {
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
        "series", "sequence", "sequence", "chain", "chains", "link", "links", "connection",
        "connections", "relation", "relations", "relationship", "relationships", "association",
        "associations", "pattern", "patterns", "structure", "structures", "format", "formats",
        "style", "styles", "design", "designs", "model", "models", "template", "templates",
        "version", "versions", "update", "updates", "change", "changes", "modification",
        "modifications", "improvement", "improvements", "enhancement", "enhancements",
        "feature", "features", "function", "functions", "capability", "capabilities",
        "option", "options", "choice", "choices", "alternative", "alternatives", "option",
        "options", "preference", "preferences", "setting", "settings", "configuration",
        "configurations", "parameter", "parameters", "variable", "variables", "value",
        "values", "amount", "amounts", "number", "numbers", "count", "counts", "total",
        "totals", "sum", "sums", "result", "results", "outcome", "outcomes", "effect",
        "effects", "impact", "impacts", "influence", "influences", "benefit", "benefits",
        "advantage", "advantages", "disadvantage", "disadvantages", "pro", "cons", "pros",
        "positive", "negatives", "good", "bad", "better", "worse", "best", "worst",
        "important", "unimportant", "significant", "insignificant", "relevant", "irrelevant",
        "useful", "useless", "helpful", "unhelpful", "effective", "ineffective", "efficient",
        "inefficient", "successful", "unsuccessful", "working", "broken", "fixed", "complete",
        "incomplete", "finished", "unfinished", "done", "undone", "ready", "unready",
        "available", "unavailable", "possible", "impossible", "likely", "unlikely",
        "certain", "uncertain", "sure", "unsure", "clear", "unclear", "obvious", "unobvious",
        "simple", "complex", "easy", "difficult", "hard", "soft", "strong", "weak",
        "fast", "slow", "quick", "quickly", "slowly", "early", "late", "soon", "later",
        "now", "then", "here", "there", "where", "when", "why", "how", "what", "who",
        "which", "whose", "whom", "wherever", "whenever", "however", "whatever", "whoever",
        "whichever", "somewhere", "anywhere", "everywhere", "nowhere", "sometime", "anytime",
        "everytime", "never", "always", "often", "sometimes", "rarely", "seldom", "usually",
        "normally", "typically", "generally", "commonly", "frequently", "occasionally",
        "regularly", "constantly", "continuously", "permanently", "temporarily", "briefly",
        "quickly", "slowly", "gradually", "suddenly", "immediately", "instantly", "eventually",
        "finally", "ultimately", "basically", "essentially", "fundamentally", "primarily",
        "mainly", "mostly", "largely", "partly", "partially", "completely", "entirely",
        "totally", "fully", "absolutely", "relatively", "comparatively", "approximately",
        "roughly", "exactly", "precisely", "accurately", "correctly", "properly", "rightly",
        "wrongly", "incorrectly", "improperly", "badly", "well", "good", "great", "excellent",
        "outstanding", "amazing", "wonderful", "fantastic", "terrible", "awful", "horrible",
        "disgusting", "beautiful", "ugly", "pretty", "handsome", "attractive", "unattractive",
        "interesting", "boring", "exciting", "dull", "fun", "funny", "serious", "silly",
        "stupid", "smart", "intelligent", "clever", "wise", "foolish", "crazy", "sane",
        "normal", "abnormal", "strange", "weird", "odd", "unusual", "common", "rare",
        "special", "ordinary", "regular", "standard", "typical", "unique", "different",
        "same", "similar", "alike", "unlike", "opposite", "contrary", "reverse", "backward",
        "forward", "ahead", "behind", "before", "after", "during", "while", "since", "until",
        "unless", "if", "unless", "although", "though", "however", "but", "yet", "still",
        "nevertheless", "nonetheless", "moreover", "furthermore", "additionally", "also",
        "too", "as", "well", "either", "neither", "both", "all", "every", "each", "some",
        "any", "no", "none", "nothing", "something", "anything", "everything", "someone",
        "anyone", "everyone", "nobody", "somebody", "anybody", "everybody", "somewhere",
        "anywhere", "everywhere", "nowhere", "sometime", "anytime", "everytime", "never",
        "always", "often", "sometimes", "rarely", "seldom", "usually", "normally", "typically"
    ];
    
    generic_words.iter().map(|s| s.to_string()).collect()
}
