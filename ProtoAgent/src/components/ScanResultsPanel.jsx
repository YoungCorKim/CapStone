import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export default function ScanResultsPanel({ results, onClose, onFileOpen }) {
  const [deleting, setDeleting] = useState(null);
  const [linking, setLinking] = useState(null);
  const [error, setError] = useState(null);

  if (!results) return null;

  const { type, pairs } = results;
  const isDuplicates = type === "duplicates";

  const handleDelete = async (pathToDelete) => {
    setDeleting(pathToDelete);
    setError(null);
    try {
      await invoke("delete_file_cmd", { path: pathToDelete });
      onClose?.();
    } catch (err) {
      setError(String(err));
    } finally {
      setDeleting(null);
    }
  };

  const handleAddLink = async (pathA, pathB) => {
    const key = `${pathA}|${pathB}`;
    setLinking(key);
    setError(null);
    try {
      await invoke("add_related_link", { pathA, pathB });
      onClose?.();
    } catch (err) {
      setError(String(err));
    } finally {
      setLinking(null);
    }
  };

  return (
    <div className="scan-results-panel">
      <div className="scan-results-header">
        <span className="scan-results-title">
          {isDuplicates ? `Duplicates (${pairs.length})` : `Related Files (${pairs.length})`}
        </span>
        <button className="scan-results-close" onClick={onClose} title="Close">
          ×
        </button>
      </div>
      {error && (
        <div className="scan-results-error">
          <p>{error}</p>
        </div>
      )}
      <div className="scan-results-list">
        {pairs.length === 0 ? (
          <p className="scan-results-empty">
            {isDuplicates ? "No duplicates found" : "No related files found"}
          </p>
        ) : (
          pairs.map((pair, i) => (
            <div key={i} className="scan-results-pair">
              <div className="scan-results-files">
                <button
                  className="scan-results-file"
                  onClick={() => onFileOpen?.(pair.path_a)}
                  title={pair.path_a}
                >
                  {pair.name_a}
                </button>
                <span className="scan-results-vs">↔</span>
                <button
                  className="scan-results-file"
                  onClick={() => onFileOpen?.(pair.path_b)}
                  title={pair.path_b}
                >
                  {pair.name_b}
                </button>
              </div>
              <div className="scan-results-actions">
                {isDuplicates ? (
                  <>
                    <button
                      className="scan-results-btn delete"
                      onClick={() => handleDelete(pair.path_a)}
                      disabled={deleting !== null}
                      title="Keep B, delete A"
                    >
                      {deleting === pair.path_a ? "..." : "Keep B"}
                    </button>
                    <button
                      className="scan-results-btn delete"
                      onClick={() => handleDelete(pair.path_b)}
                      disabled={deleting !== null}
                      title="Keep A, delete B"
                    >
                      {deleting === pair.path_b ? "..." : "Keep A"}
                    </button>
                  </>
                ) : (
                  <button
                    className="scan-results-btn link"
                    onClick={() => handleAddLink(pair.path_a, pair.path_b)}
                    disabled={linking !== null}
                  >
                    {linking === `${pair.path_a}|${pair.path_b}` ? "..." : "Add link"}
                  </button>
                )}
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
