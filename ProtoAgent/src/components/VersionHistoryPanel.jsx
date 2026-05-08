import { useState, useEffect, useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const SNIPPET_LEN = 1800;

function clip(text) {
  if (text == null || text === "") return "(empty)";
  if (text.length <= SNIPPET_LEN) return text;
  return `${text.slice(0, SNIPPET_LEN)}\n\n… (${text.length} chars; truncated)`;
}

function toggleSet(set, id) {
  const next = new Set(set);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  return next;
}

export default function VersionHistoryPanel({
  currentFilePath,
  onOpenFile,
}) {
  const [entries, setEntries] = useState([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [groupBy, setGroupBy] = useState("file");
  const [expanded, setExpanded] = useState(() => new Set());
  const [onlyCurrentFile, setOnlyCurrentFile] = useState(false);
  const [busyId, setBusyId] = useState(null);
  const [busyBatchId, setBusyBatchId] = useState(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const filePath =
        onlyCurrentFile && currentFilePath ? currentFilePath : null;
      const list = await invoke("list_version_history", {
        filePath,
        batchId: null,
      });
      setEntries(Array.isArray(list) ? list : []);
    } catch (e) {
      setError(String(e));
      setEntries([]);
    } finally {
      setLoading(false);
    }
  }, [onlyCurrentFile, currentFilePath]);

  useEffect(() => {
    load();
  }, [load]);

  useEffect(() => {
    const unlisten = listen("vault-tree-changed", () => {
      load();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [load]);

  const grouped = useMemo(() => {
    if (groupBy === "file") {
      const m = new Map();
      for (const e of entries) {
        const key = e.absolute_path || e.relative_path;
        if (!m.has(key)) m.set(key, []);
        m.get(key).push(e);
      }
      return Array.from(m.entries()).map(([key, list]) => ({
        key,
        label: list[0]?.relative_path || key,
        entries: list,
        batchId: null,
      }));
    }
    const m = new Map();
    for (const e of entries) {
      const bid = e.batch_id || "__none__";
      if (!m.has(bid)) m.set(bid, []);
      m.get(bid).push(e);
    }
    return Array.from(m.entries()).map(([bid, list]) => ({
      key: bid,
      label:
        bid === "__none__"
          ? "No batch (single applies / restores)"
          : `Batch ${bid.slice(0, 8)}…`,
      batchId: bid === "__none__" ? null : bid,
      entries: list,
    }));
  }, [entries, groupBy]);

  const handleClear = async () => {
    if (
      !window.confirm(
        "Clear all in-memory version history? This cannot be undone for the current session."
      )
    ) {
      return;
    }
    try {
      await invoke("clear_version_history");
      await load();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleRestoreVersion = async (entry) => {
    if (
      !window.confirm(
        `Revert this change? The file will match its content before this edit (create entries remove the file).\n\n${entry.relative_path}`
      )
    ) {
      return;
    }
    setBusyId(entry.id);
    setError(null);
    try {
      const path = await invoke("restore_file_version", {
        versionId: entry.id,
      });
      if (typeof path === "string" && path && onOpenFile) {
        onOpenFile(path);
      }
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyId(null);
    }
  };

  const handleRestoreBatch = async (batchId) => {
    if (!batchId) return;
    if (
      !window.confirm(
        "Revert this batch? Each item is undone in order: edits go back to pre-change content; created files may be removed."
      )
    ) {
      return;
    }
    setBusyBatchId(batchId);
    setError(null);
    try {
      const paths = await invoke("restore_batch", { batchId });
      if (Array.isArray(paths) && paths.length && onOpenFile) {
        onOpenFile(paths[paths.length - 1]);
      }
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyBatchId(null);
    }
  };

  return (
    <div className="version-history-panel">
      <div className="version-history-toolbar">
        <div className="version-history-toolbar-row">
          <span className="version-history-label">Group</span>
          <div className="version-history-segments">
            <button
              type="button"
              className={`version-history-seg ${groupBy === "file" ? "active" : ""}`}
              onClick={() => setGroupBy("file")}
            >
              By file
            </button>
            <button
              type="button"
              className={`version-history-seg ${groupBy === "batch" ? "active" : ""}`}
              onClick={() => setGroupBy("batch")}
            >
              By batch
            </button>
          </div>
          <button
            type="button"
            className="version-history-refresh"
            onClick={() => load()}
            disabled={loading}
            title="Reload"
          >
            {loading ? "…" : "↻"}
          </button>
          <button
            type="button"
            className="version-history-clear"
            onClick={handleClear}
            title="Clear all history for this session"
          >
            Clear
          </button>
        </div>
        <label className="version-history-filter">
          <input
            type="checkbox"
            checked={onlyCurrentFile}
            onChange={(ev) => setOnlyCurrentFile(ev.target.checked)}
            disabled={!currentFilePath}
          />
          Current file only
        </label>
      </div>
      {!currentFilePath && onlyCurrentFile && (
        <p className="version-history-hint">Open a file to filter by path.</p>
      )}
      {error && (
        <div className="version-history-error">
          <p>{error}</p>
        </div>
      )}
      {entries.length === 0 && !loading && (
        <p className="version-history-empty">
          No version snapshots yet. Accept a markdown proposal to record history.
        </p>
      )}
      <ul className="version-history-groups">
        {grouped.map((group) => (
          <li key={group.key} className="version-history-group">
            <div className="version-history-group-head">
              <span className="version-history-group-title" title={group.key}>
                {group.label}{" "}
                <span className="version-history-count">
                  ({group.entries.length})
                </span>
              </span>
              {group.batchId && (
                <button
                  type="button"
                  className="version-history-batch-restore"
                  disabled={busyBatchId === group.batchId}
                  onClick={() => handleRestoreBatch(group.batchId)}
                >
                  {busyBatchId === group.batchId ? "Reverting…" : "Revert batch"}
                </button>
              )}
            </div>
            <ul className="version-history-entries">
              {[...group.entries]
                .slice()
                .reverse()
                .map((entry) => (
                  <li key={entry.id} className="version-history-entry">
                    <div className="version-history-entry-row">
                      <button
                        type="button"
                        className="version-history-expand"
                        onClick={() =>
                          setExpanded((s) => toggleSet(s, entry.id))
                        }
                        aria-expanded={expanded.has(entry.id)}
                      >
                        {expanded.has(entry.id) ? "▼" : "▶"}
                      </button>
                      <div className="version-history-meta">
                        <span className="version-history-time">
                          {entry.created_at_rfc3339}
                        </span>
                        <span className="version-history-source">
                          {entry.source}
                        </span>
                        {groupBy === "file" && entry.batch_id && (
                          <span
                            className="version-history-batch-tag"
                            title={entry.batch_id}
                          >
                            batch
                          </span>
                        )}
                        {groupBy === "batch" && (
                          <span
                            className="version-history-path"
                            title={entry.absolute_path}
                          >
                            {entry.relative_path}
                          </span>
                        )}
                      </div>
                      <button
                        type="button"
                        className="version-history-restore-one"
                        disabled={busyId === entry.id}
                        onClick={() => handleRestoreVersion(entry)}
                      >
                        {busyId === entry.id ? "…" : "Revert"}
                      </button>
                    </div>
                    {expanded.has(entry.id) && (
                      <div className="version-history-preview">
                        <details open>
                          <summary>Before</summary>
                          <pre className="version-history-pre">
                            {clip(entry.before_content)}
                          </pre>
                        </details>
                        <details open>
                          <summary>After</summary>
                          <pre className="version-history-pre">
                            {clip(entry.after_content)}
                          </pre>
                        </details>
                      </div>
                    )}
                  </li>
                ))}
            </ul>
          </li>
        ))}
      </ul>
    </div>
  );
}
