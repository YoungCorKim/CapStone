//! RAG indexing: build a vector store from markdown vault files for dynamic context.

use rig::{
    client::EmbeddingsClient,
    embeddings::EmbeddingsBuilder,
    vector_store::in_memory_store::InMemoryVectorStore,
    Embed,
};
use serde::{Deserialize, Serialize};
use std::fs;

use crate::metadata_parser::extract_frontmatter;
use crate::scan_markdown_files;

/// Document stored in the RAG vector store. Retrieved and shown to the agent as context.
#[derive(Embed, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Default)]
pub struct RagDocument {
    pub path: String,
    pub name: String,
    #[embed]
    pub content: String,
}

/// Build a vector store index from a vault directory.
/// Returns the store (to be cached). Use store.index(embedding_model) to create the index for dynamic_context.
pub async fn build_vault_index(
    vault_path: &str,
    client: &rig::providers::openai::Client,
) -> Result<InMemoryVectorStore<RagDocument>, String> {
    let files = scan_markdown_files(vault_path.to_string())
        .map_err(|e| format!("Failed to scan vault: {}", e))?;

    if files.is_empty() {
        return Ok(InMemoryVectorStore::<RagDocument>::builder().build());
    }

    let model = client.embedding_model(rig::providers::openai::TEXT_EMBEDDING_3_SMALL);

    let mut builder = EmbeddingsBuilder::new(model.clone());

    for file in &files {
        let content = fs::read_to_string(&file.path)
            .map_err(|e| format!("Failed to read {}: {}", file.path, e))?;
        let extracted = extract_frontmatter(&content);
        let doc = RagDocument {
            path: file.path.clone(),
            name: file.name.clone(),
            content: extracted.content.trim().to_string(),
        };
        if !doc.content.is_empty() {
            builder = builder.document(doc).map_err(|e| e.to_string())?;
        }
    }

    let embeddings = builder.build().await.map_err(|e| e.to_string())?;

    if embeddings.is_empty() {
        return Ok(InMemoryVectorStore::<RagDocument>::builder().build());
    }

    let store = InMemoryVectorStore::from_documents_with_id_f(embeddings, |d| d.path.clone());

    Ok(store)
}
