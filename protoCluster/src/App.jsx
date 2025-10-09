import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import "./App.css";

function App() {
  const [selectedFolder, setSelectedFolder] = useState("");
  const [markdownFiles, setMarkdownFiles] = useState([]);
  const [clusters, setClusters] = useState([]);
  const [loading, setLoading] = useState(false);
  const [clustering, setClustering] = useState(false);
  const [error, setError] = useState("");
  const [viewMode, setViewMode] = useState("files"); // "files" or "clusters"

  async function selectFolder() {
    try {
      setError("");
      setLoading(true);
      
      // Open folder selection dialog
      const folderPath = await open({
        directory: true,
        title: "Select folder containing markdown files"
      });
      
      if (folderPath) {
        setSelectedFolder(folderPath);
        
        // Scan for markdown files
        const files = await invoke("scan_markdown_files", { folderPath });
        setMarkdownFiles(files);
      }
    } catch (err) {
      setError(`Error: ${err}`);
    } finally {
      setLoading(false);
    }
  }

  async function performClustering() {
    if (markdownFiles.length === 0) {
      setError("No files to cluster. Please select a folder first.");
      return;
    }

    try {
      setError("");
      setClustering(true);
      
      const result = await invoke("cluster_markdown_files", { 
        files: markdownFiles,
        minSimilarity: 0.1
      });
      
      setClusters(result);
      setViewMode("clusters");
    } catch (err) {
      setError(`Clustering error: ${err}`);
    } finally {
      setClustering(false);
    }
  }

  return (
    <main className="container">
      <h1>Markdown File Clustering App</h1>
      
      <div className="folder-section">
        <div className="controls">
          <button 
            onClick={selectFolder}
            disabled={loading}
            className="folder-button"
          >
            {loading ? "Scanning..." : "Select Folder"}
          </button>
          
          {markdownFiles.length > 0 && (
            <button 
              onClick={performClustering}
              disabled={clustering}
              className="cluster-button"
            >
              {clustering ? "Clustering..." : "Cluster Files"}
            </button>
          )}
        </div>
        
        {selectedFolder && (
          <p className="selected-folder">
            Selected: {selectedFolder}
          </p>
        )}
        
        {markdownFiles.length > 0 && (
          <div className="view-controls">
            <button 
              onClick={() => setViewMode("files")}
              className={viewMode === "files" ? "view-button active" : "view-button"}
            >
              Files ({markdownFiles.length})
            </button>
            {clusters.length > 0 && (
              <button 
                onClick={() => setViewMode("clusters")}
                className={viewMode === "clusters" ? "view-button active" : "view-button"}
              >
                Clusters ({clusters.length})
              </button>
            )}
          </div>
        )}
        
        {error && (
          <p className="error">{error}</p>
        )}
      </div>

      {markdownFiles.length > 0 && viewMode === "files" && (
        <div className="files-section">
          <h2>Found {markdownFiles.length} markdown files:</h2>
          <div className="file-list">
            {markdownFiles.map((file, index) => (
              <div key={index} className="file-item">
                <div className="file-header">
                  <h3 className="file-title">
                    {file.title || file.path.split('/').pop() || 'Untitled'}
                  </h3>
                  <span className="file-path">{file.path}</span>
                </div>
                
                {file.frontmatter_tags.length > 0 && (
                  <div className="tags-section">
                    <span className="tag-label">Frontmatter Tags:</span>
                    <div className="tag-list">
                      {file.frontmatter_tags.map((tag, tagIndex) => (
                        <span key={tagIndex} className="tag frontmatter-tag">
                          {tag}
                        </span>
                      ))}
                    </div>
                  </div>
                )}
                
                {file.hashtags.length > 0 && (
                  <div className="tags-section">
                    <span className="tag-label">Hashtags:</span>
                    <div className="tag-list">
                      {file.hashtags.map((tag, tagIndex) => (
                        <span key={tagIndex} className="tag hashtag">
                          #{tag}
                        </span>
                      ))}
                    </div>
                  </div>
                )}
                
                {file.extracted_keywords.length > 0 && (
                  <div className="tags-section">
                    <span className="tag-label">Extracted Keywords:</span>
                    <div className="tag-list">
                      {file.extracted_keywords.map((keyword, keywordIndex) => (
                        <span key={keywordIndex} className="tag keyword">
                          {keyword}
                        </span>
                      ))}
                    </div>
                  </div>
                )}
                
                {file.all_tags.length === 0 && (
                  <div className="tags-section">
                    <span className="tag-label">No tags found</span>
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {clusters.length > 0 && viewMode === "clusters" && (
        <div className="clusters-section">
          <h2>Found {clusters.length} clusters:</h2>
          <div className="cluster-list">
            {clusters.map((cluster, index) => (
              <div key={index} className="cluster-item">
                <div className="cluster-header">
                  <h3 className="cluster-name">{cluster.name}</h3>
                  <div className="cluster-meta">
                    <span className="cluster-count">{cluster.files.length} files</span>
                    <span className="cluster-similarity">
                      {Math.round(cluster.similarity_score * 100)}% similarity
                    </span>
                  </div>
                </div>
                
                {cluster.shared_tags.length > 0 && (
                  <div className="cluster-tags">
                    <span className="tag-label">Shared Tags:</span>
                    <div className="tag-list">
                      {cluster.shared_tags.map((tag, tagIndex) => (
                        <span key={tagIndex} className="tag shared-tag">
                          {tag}
                        </span>
                      ))}
                    </div>
                  </div>
                )}
                
                <div className="cluster-files">
                  <span className="tag-label">Files in this cluster:</span>
                  <div className="cluster-file-list">
                    {cluster.files.map((file, fileIndex) => (
                      <div key={fileIndex} className="cluster-file-item">
                        <span className="cluster-file-title">
                          {file.title || file.path.split('/').pop() || 'Untitled'}
                        </span>
                        <span className="cluster-file-path">{file.path}</span>
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </main>
  );
}

export default App;
