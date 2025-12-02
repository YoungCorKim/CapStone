import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";
import FolderSelector from "./components/FolderSelector";
import ClusterView from "./components/ClusterView";
import SummaryView from "./components/SummaryView";

function App() {
  const [vaultPath, setVaultPath] = useState(null);
  const [files, setFiles] = useState([]);
  const [clusters, setClusters] = useState([]);
  const [summaries, setSummaries] = useState(null);
  const [isScanning, setIsScanning] = useState(false);
  const [isGenerating, setIsGenerating] = useState(false);
  const [error, setError] = useState(null);

  const handleFolderSelected = async (path) => {
    setVaultPath(path);
    setFiles([]);
    setClusters([]);
    setSummaries(null);
    setError(null);
    setIsScanning(true);

    try {
      const scannedFiles = await invoke("scan_markdown_files", { vaultPath: path });
      setFiles(scannedFiles);
      
      if (scannedFiles.length === 0) {
        setError("No markdown files found in the selected vault.");
        setIsScanning(false);
        return;
      }

      // Automatically cluster files after scanning
      const fileClusters = await invoke("cluster_files_by_type", { files: scannedFiles });
      setClusters(fileClusters);
    } catch (err) {
      console.error("Error scanning files:", err);
      setError(`Error scanning files: ${err}`);
    } finally {
      setIsScanning(false);
    }
  };

  const handleGenerateSummaries = async () => {
    if (clusters.length === 0) {
      setError("No clusters available. Please select a vault first.");
      return;
    }

    setIsGenerating(true);
    setError(null);

    try {
      const generatedSummaries = await invoke("generate_summaries", { clusters });
      setSummaries(generatedSummaries);
    } catch (err) {
      console.error("Error generating summaries:", err);
      setError(`Error generating summaries: ${err}`);
    } finally {
      setIsGenerating(false);
    }
  };

  const handleSaveComplete = () => {
    // Could show a success message or refresh state
    console.log("Summaries saved successfully");
  };

  return (
    <main className="container">
      <h1>Obsidian Academic Document Organizer</h1>
      
      {error && (
        <div className="error-message">
          <p>{error}</p>
          <button onClick={() => setError(null)}>Dismiss</button>
        </div>
      )}

      <FolderSelector
        onFolderSelected={handleFolderSelected}
        vaultPath={vaultPath}
        fileCount={files.length}
      />

      {isScanning && (
        <div className="loading">
          <p>Scanning markdown files...</p>
        </div>
      )}

      {vaultPath && files.length > 0 && (
        <ClusterView
          clusters={clusters}
          onGenerateSummaries={handleGenerateSummaries}
          isGenerating={isGenerating}
        />
      )}

      {summaries && (
        <SummaryView
          summaries={summaries}
          vaultPath={vaultPath}
          onSaveComplete={handleSaveComplete}
        />
      )}
    </main>
  );
}

export default App;
