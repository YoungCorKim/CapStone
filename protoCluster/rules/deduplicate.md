# Semantic Embedding Deduplication Pipeline

## Overview
This process performs multi-level deduplication using semantic embeddings.  
It starts with individual Markdown files, removes redundant or similar content at the **sentence**, **paragraph**, and **file** levels, and reconstructs unified documents while preserving logical flow and context.

---
role: semantic-deduplication-agent
purpose: >
  You are an AI agent responsible for performing semantic deduplication across Markdown documents. 
  Use embeddings and cosine similarity to group, merge, and unify similar content.
instructions:
  - Focus on meaning, not syntax.
  - Use dynamic thresholds for similarity.
  - Summarize near-duplicates into concise unified sentences.
  - Preserve logical order and trace original source.
---

## 1. Preprocessing

1. **Input**: Individual Markdown (`.md`) files.  
2. **Remove Markdown Formatting**:  
   Strip Markdown symbols (e.g., `#`, `*`, `>`, code blocks) to reduce noise.  
3. **Normalize Text**:  
   - Convert to lowercase.  
   - Remove extra whitespace.  
   - Optionally apply lemmatization or stopword removal for cleaner semantics.

---

## 2. Sentence-Level Processing

1. **Sentence Segmentation**  
   - Split text into sentences using an NLP library such as spaCy or NLTK.  

2. **Generate Sentence Embeddings**  
   - Use OpenAI embeddings (e.g., `text-embedding-3-small` or `text-embedding-3-large`).  
   - Store each sentence and its embedding vector in a hash table (or vector database).  

3. **Compute Similarity**  
   - Use **cosine similarity** to compare embeddings.  
   - Define a similarity threshold, e.g., `0.9`.  
   - Sentences with similarity above the threshold are considered semantically identical.

4. **Deduplicate Sentences**  
   - Group sentences based on semantic similarity.  
   - Keep only one representative per group.  
   - For slightly different sentences (similarity 0.8–0.9), summarize into a clean unified version.

5. **Output**:  
   A pool of unique sentences, each linked back to:
   - Original file  
   - Original paragraph  
   - Sentence index (for reconstruction)

---

## 3. Paragraph-Level Reconstruction

1. **Rebuild Paragraphs**  
   - Reconstruct paragraphs using their unique sentences.  
   - Preserve the original logical flow and structure.

2. **Generate Paragraph Embeddings**  
   - Compute semantic embeddings for each paragraph.

3. **Compare Paragraphs**  
   - If cosine similarity > **0.9** → mark as duplicate.  
   - If similarity between **0.8–0.9**, merge using an AI summarizer:
     - Retain as many original sentences as possible.  
     - Preserve flow and intent.

4. **Output**:  
   Unified, deduplicated paragraphs linked to original file locations.

---

## 4. File-Level Comparison and Merging

1. **Compute File Embeddings**  
   - Generate embeddings representing the entire file content.

2. **Compare Files**  
   - If two files have cosine similarity above the threshold → mark as duplicates and merge.  
   - If below the threshold → perform paragraph-level comparison.

3. **Merge Files**  
   - Combine unique paragraphs while maintaining the original order.  
   - Rebuild into a single unified Markdown file.

---

## 5. Final Output

- A **unified Markdown file** containing:
  - Unique, semantically deduplicated sentences and paragraphs.  
  - Preserved logical structure.  
  - Metadata linking back to the original sources.

---

## 🧩 Optional Enhancements

- Use **vector databases** (e.g., FAISS, Chroma, Milvus) for scalable similarity search.  
- Add **dynamic thresholding** or **clustering (DBSCAN)** to reduce O(n²) comparisons.  
- Implement **caching** for embeddings to speed up re-runs.  
- Keep an **audit trail** of merged content for traceability.  
- Integrate a **rule manager** for domain-specific exceptions (e.g., skip "References").  