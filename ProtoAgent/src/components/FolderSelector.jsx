import { open } from "@tauri-apps/plugin-dialog";

export default function FolderSelector({ onFolderSelected, vaultPath, fileCount }) {
  const handleSelectFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Obsidian Vault Folder",
      });
      
      if (selected) {
        // Handle both string (single path) and array (multiple paths) cases
        const path = Array.isArray(selected) ? selected[0] : selected;
        onFolderSelected(path);
      }
    } catch (error) {
      console.error("Error selecting folder:", error);
      alert(`Error selecting folder: ${error}`);
    }
  };

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

