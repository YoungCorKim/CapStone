import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

function FileTreeItem({ entry, depth, openFilePath, onFileSelect, expandedDirs, onToggleDir, onContextMenu }) {
  const isExpanded = expandedDirs.has(entry.path);
  const hasChildren = entry.children && entry.children.length > 0;

  const handleClick = () => {
    if (entry.is_dir) {
      onToggleDir(entry.path);
    } else {
      onFileSelect(entry.path);
    }
  };

  const handleContextMenu = (e) => {
    e.preventDefault();
    onContextMenu?.(e, entry);
  };

  const isSelected = !entry.is_dir && openFilePath === entry.path;

  return (
    <div className="file-tree-item">
      <div
        className={`file-tree-row ${isSelected ? "selected" : ""}`}
        style={{ paddingLeft: `${depth * 12 + 8}px` }}
        onClick={handleClick}
        onContextMenu={handleContextMenu}
      >
        <span className="file-tree-icon">
          {entry.is_dir ? (isExpanded ? "▾" : "▸") : "•"}
        </span>
        <span className="file-tree-name" title={entry.path}>
          {entry.name}
        </span>
      </div>
      {entry.is_dir && isExpanded && hasChildren && (
        <div className="file-tree-children">
          {entry.children.map((child) => (
            <FileTreeItem
              key={child.path}
              entry={child}
              depth={depth + 1}
              openFilePath={openFilePath}
              onFileSelect={onFileSelect}
              expandedDirs={expandedDirs}
              onToggleDir={onToggleDir}
              onContextMenu={onContextMenu}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export default function ProjectExplorer({ vaultPath, openFilePath, onFileSelect, onScanResults }) {
  const [tree, setTree] = useState([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [expandedDirs, setExpandedDirs] = useState(new Set());
  const [contextMenu, setContextMenu] = useState(null);
  const [scanLoading, setScanLoading] = useState(false);
  const [scanError, setScanError] = useState(null);

  const scanPathFor = (entry) =>
    entry.is_dir ? entry.path : entry.path.replace(/[/\\][^/\\]+$/, "") || vaultPath;

  const handleContextMenu = useCallback((e, entry) => {
    e.preventDefault();
    setContextMenu({
      x: e.clientX,
      y: e.clientY,
      entry,
    });
  }, []);

  const closeContextMenu = useCallback(() => setContextMenu(null), []);

  const runScan = useCallback(
    async (type) => {
      if (!contextMenu || !onScanResults) return;
      const path = scanPathFor(contextMenu.entry);
      setScanLoading(true);
      setScanError(null);
      closeContextMenu();
      try {
        const pairs =
          type === "duplicates"
            ? await invoke("get_duplicate_pairs", { vaultPath: path })
            : await invoke("get_related_pairs", { vaultPath: path });
        onScanResults(type, pairs);
      } catch (err) {
        setScanError(String(err));
      } finally {
        setScanLoading(false);
      }
    },
    [contextMenu, onScanResults, closeContextMenu, vaultPath]
  );

  useEffect(() => {
    const handler = () => closeContextMenu();
    if (contextMenu) {
      window.addEventListener("click", handler);
      return () => window.removeEventListener("click", handler);
    }
  }, [contextMenu, closeContextMenu]);

  useEffect(() => {
    if (!vaultPath) {
      setTree([]);
      setError(null);
      return;
    }

    setLoading(true);
    setError(null);
    invoke("list_directory", { path: vaultPath })
      .then((entries) => {
        setTree(entries);
        setExpandedDirs(new Set([vaultPath]));
      })
      .catch((err) => {
        setError(String(err));
        setTree([]);
      })
      .finally(() => setLoading(false));
  }, [vaultPath]);

  const handleToggleDir = (path) => {
    setExpandedDirs((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  };

  if (!vaultPath) {
    return (
      <div className="project-explorer-empty">
        <p>Select a folder to browse files</p>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="project-explorer-loading">
        <p>Loading...</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="project-explorer-error">
        <p>{error}</p>
      </div>
    );
  }

  if (tree.length === 0) {
    return (
      <div className="project-explorer-empty">
        <p>No markdown files found</p>
      </div>
    );
  }

  return (
    <div className="project-explorer">
      {scanLoading && (
        <div className="project-explorer-scan-loading">Scanning...</div>
      )}
      {scanError && (
        <div className="project-explorer-scan-error">
          {scanError}
          <button type="button" onClick={() => setScanError(null)} aria-label="Dismiss">
            ×
          </button>
        </div>
      )}
      <div className="file-tree">
        {tree.map((entry) => (
          <FileTreeItem
            key={entry.path}
            entry={entry}
            depth={0}
            openFilePath={openFilePath}
            onFileSelect={onFileSelect}
            expandedDirs={expandedDirs}
            onToggleDir={handleToggleDir}
            onContextMenu={onScanResults ? handleContextMenu : undefined}
          />
        ))}
      </div>
      {contextMenu && (
        <div
          className="explorer-context-menu"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          <button
            type="button"
            onClick={() => runScan("duplicates")}
          >
            Find duplicates
          </button>
          <button
            type="button"
            onClick={() => runScan("related")}
          >
            Find related files
          </button>
        </div>
      )}
    </div>
  );
}
