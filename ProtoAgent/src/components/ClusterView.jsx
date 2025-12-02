export default function ClusterView({ clusters, onGenerateSummaries, isGenerating }) {
  if (!clusters || clusters.length === 0) {
    return (
      <div className="cluster-view">
        <p>No clusters found. Select a vault and scan files first.</p>
      </div>
    );
  }

  return (
    <div className="cluster-view">
      <h2>Document Clusters</h2>
      <div className="clusters-list">
        {clusters.map((cluster, index) => (
          <div key={index} className="cluster-card">
            <h3 className="cluster-type">
              {cluster.doc_type.charAt(0).toUpperCase() + cluster.doc_type.slice(1)}
            </h3>
            <p className="cluster-count">{cluster.files.length} file(s)</p>
            <details className="cluster-files">
              <summary>View files</summary>
              <ul>
                {cluster.files.map((file, fileIndex) => (
                  <li key={fileIndex} className="file-item">
                    {file.name}
                  </li>
                ))}
              </ul>
            </details>
          </div>
        ))}
      </div>
      <button
        onClick={onGenerateSummaries}
        disabled={isGenerating}
        className="generate-btn"
      >
        {isGenerating ? "Generating..." : "Generate Summaries"}
      </button>
    </div>
  );
}

