import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

function pathsEffectivelyEqual(a, b) {
  if (!a || !b) return false;
  if (a === b) return true;
  const na = String(a).replace(/\\/g, "/");
  const nb = String(b).replace(/\\/g, "/");
  return na === nb;
}

export default function MarkdownEditor({ filePath, vaultPath }) {
  const [content, setContent] = useState("");
  const [originalContent, setOriginalContent] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState(null);

  const loadFile = useCallback(async () => {
    if (!filePath) return;
    setLoading(true);
    setError(null);
    try {
      const text = await invoke("read_file", { path: filePath });
      setContent(text);
      setOriginalContent(text);
    } catch (err) {
      setError(String(err));
      setContent("");
      setOriginalContent("");
    } finally {
      setLoading(false);
    }
  }, [filePath]);

  useEffect(() => {
    loadFile();
  }, [loadFile]);

  useEffect(() => {
    if (!filePath) return undefined;
    const unlisten = listen("vault-tree-changed", (event) => {
      const p = event?.payload?.path;
      if (typeof p === "string" && pathsEffectivelyEqual(p, filePath)) {
        loadFile();
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [filePath, loadFile]);

  const hasUnsavedChanges = content !== originalContent;

  const handleSave = async () => {
    if (!filePath || !hasUnsavedChanges) return;
    setSaving(true);
    setError(null);
    try {
      await invoke("write_file", { path: filePath, content });
      setOriginalContent(content);
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  const handleKeyDown = (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault();
      handleSave();
    }
  };

  if (!filePath) {
    return (
      <div className="markdown-editor-empty">
        <p>Open a file from the explorer to edit</p>
      </div>
    );
  }

  const filename = filePath.split(/[/\\]/).pop();

  return (
    <div className="markdown-editor">
      <div className="editor-header">
        <span className="editor-filename">{filename}</span>
        {hasUnsavedChanges && <span className="editor-unsaved">●</span>}
        <button
          className="editor-save-btn"
          onClick={handleSave}
          disabled={!hasUnsavedChanges || saving}
          title="Save (Ctrl+S)"
        >
          {saving ? "Saving..." : "Save"}
        </button>
      </div>
      {error && (
        <div className="editor-error">
          <p>{error}</p>
        </div>
      )}
      {loading ? (
        <div className="editor-loading">
          <p>Loading...</p>
        </div>
      ) : (
        <textarea
          className="editor-textarea"
          value={content}
          onChange={(e) => setContent(e.target.value)}
          onKeyDown={handleKeyDown}
          spellCheck={false}
          placeholder="Start typing..."
        />
      )}
    </div>
  );
}
