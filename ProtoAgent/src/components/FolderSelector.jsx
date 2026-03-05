import { open } from "@tauri-apps/plugin-dialog";

export default function FolderSelector({ onFolderSelected, vaultPath, fileCount, compact }) {
  const handleSelectFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Folder",
      });

      if (selected) {
        const path = Array.isArray(selected) ? selected[0] : selected;
        onFolderSelected(path);
      }
    } catch (error) {
      console.error("Error selecting folder:", error);
      alert(`Error selecting folder: ${error}`);
    }
  };

  if (compact) {
    return (
      <div className="folder-selector compact">
        <button onClick={handleSelectFolder} className="select-folder-btn compact">
          {vaultPath ? "Change Folder" : "Open Folder"}
        </button>
        {vaultPath && (
          <p className="vault-path compact" title={vaultPath}>
            {vaultPath.split(/[/\\]/).pop() || vaultPath}
          </p>
        )}
      </div>
    );
  }

  return (
    <div className="folder-selector">
      <button onClick={handleSelectFolder} className="select-folder-btn">
        {vaultPath ? "Change Vault Folder" : "Select Obsidian Vault Folder"}
      </button>
      {vaultPath && (
        <div className="vault-info">
          <p className="vault-path">
            <strong>Vault:</strong> {vaultPath}
          </p>
          {fileCount !== null && (
            <p className="file-count">
              <strong>Markdown files found:</strong> {fileCount}
            </p>
          )}
        </div>
      )}
    </div>
  );
}

