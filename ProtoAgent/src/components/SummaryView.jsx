import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export default function SummaryView({ summaries, vaultPath, onSaveComplete }) {
  const [isSaving, setIsSaving] = useState(false);
  const [saveStatus, setSaveStatus] = useState(null);

  if (!summaries) {
    return (
      <div className="summary-view">
        <p>No summaries generated yet. Generate summaries to view them here.</p>
      </div>
    );
  }

  const handleSave = async () => {
    if (!vaultPath) {
      alert("No vault path selected");
      return;
    }

    setIsSaving(true);
    setSaveStatus(null);

    try {
      await invoke("save_summary_to_vault", {
        vaultPath,
        summaries,
      });
      setSaveStatus({ success: true, message: "Summaries saved successfully!" });
      if (onSaveComplete) {
        onSaveComplete();
      }
    } catch (error) {
      console.error("Error saving summaries:", error);
      setSaveStatus({ success: false, message: `Error: ${error}` });
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="summary-view">
      <h2>Generated Summaries</h2>
      
      <div className="master-summary">
        <h3>Master Summary</h3>
        <div className="summary-content">
          <pre>{summaries.overview}</pre>
        </div>
      </div>

      <div className="cluster-summaries">
        <h3>Cluster Summaries</h3>
        {summaries.cluster_summaries.map((clusterSummary, index) => (
          <details key={index} className="cluster-summary-card">
            <summary>
              <strong>{clusterSummary.doc_type.toUpperCase()}</strong> - {clusterSummary.file_count} file(s)
            </summary>
            <div className="summary-content">
              <pre>{clusterSummary.summary}</pre>
            </div>
          </details>
        ))}
      </div>

      <div className="save-section">
        <button
          onClick={handleSave}
          disabled={isSaving || !vaultPath}
          className="save-btn"
        >
          {isSaving ? "Saving..." : "Save to Vault"}
        </button>
        {saveStatus && (
          <p className={`save-status ${saveStatus.success ? "success" : "error"}`}>
            {saveStatus.message}
          </p>
        )}
      </div>
    </div>
  );
}

