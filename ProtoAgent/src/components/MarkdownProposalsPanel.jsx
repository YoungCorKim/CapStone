import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

const PREVIEW_MAX = 8000;

function clip(text) {
  if (text == null || text === "") return "(empty)";
  if (text.length <= PREVIEW_MAX) return text;
  return `${text.slice(0, PREVIEW_MAX)}\n\n… (${text.length} chars total; truncated)`;
}

export default function MarkdownProposalsPanel({
  proposals,
  onRemove,
  onAppliedOpenFile,
  onClosePanel,
}) {
  const [busyId, setBusyId] = useState(null);
  const [acceptAllBusy, setAcceptAllBusy] = useState(false);
  const [error, setError] = useState(null);

  const handleAccept = async (id) => {
    setBusyId(id);
    setError(null);
    try {
      const path = await invoke("apply_markdown_proposal", { proposalId: id });
      if (typeof path === "string" && path) {
        onAppliedOpenFile?.(path);
      }
      onRemove(id);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusyId(null);
    }
  };

  const handleAcceptAll = async () => {
    if (proposals.length < 2) return;
    if (
      !window.confirm(
        `Accept all ${proposals.length} proposals in one batch? They will share one version-history batch id.`
      )
    ) {
      return;
    }
    setAcceptAllBusy(true);
    setError(null);
    try {
      const proposalIds = proposals.map((p) => p.id);
      const paths = await invoke("apply_markdown_proposals_batch", {
        proposalIds,
      });
      const last =
        Array.isArray(paths) && paths.length
          ? paths[paths.length - 1]
          : null;
      if (typeof last === "string" && last) {
        onAppliedOpenFile?.(last);
      }
      proposalIds.forEach((id) => onRemove(id));
    } catch (err) {
      setError(String(err));
    } finally {
      setAcceptAllBusy(false);
    }
  };

  const handleReject = async (id) => {
    setBusyId(id);
    setError(null);
    try {
      await invoke("reject_markdown_proposal", { proposalId: id });
      onRemove(id);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusyId(null);
    }
  };

  if (!proposals.length) return null;

  return (
    <div className="markdown-proposals-panel">
      <div className="markdown-proposals-header">
        <div className="markdown-proposals-header-main">
          <span className="markdown-proposals-title">
            Pending markdown ({proposals.length})
          </span>
          {proposals.length > 1 && (
            <button
              type="button"
              className="markdown-proposals-accept-all"
              disabled={acceptAllBusy || busyId != null}
              onClick={handleAcceptAll}
              title="Accept all proposals as one batch"
            >
              {acceptAllBusy ? "Accepting…" : "Accept all"}
            </button>
          )}
        </div>
        <button
          type="button"
          className="markdown-proposals-close"
          onClick={onClosePanel}
          title="Hide panel (proposals stay queued)"
        >
          ×
        </button>
      </div>
      {error && (
        <div className="markdown-proposals-error">
          <p>{error}</p>
        </div>
      )}
      <div className="markdown-proposals-list">
        {proposals.map((p) => (
          <div key={p.id} className="markdown-proposals-card">
            <div className="markdown-proposals-card-head">
              <span className={`markdown-proposals-kind markdown-proposals-kind-${p.kind}`}>
                {p.kind === "create" ? "Create" : "Edit"}
              </span>
              <span className="markdown-proposals-path" title={p.absolute_path}>
                {p.relative_path}
              </span>
            </div>
            {p.kind === "edit" && p.previous_content != null && (
              <details className="markdown-proposals-details">
                <summary>Previous content</summary>
                <pre className="markdown-proposals-pre">{clip(p.previous_content)}</pre>
              </details>
            )}
            <details className="markdown-proposals-details" open={p.kind === "create"}>
              <summary>{p.kind === "create" ? "Content" : "Proposed content"}</summary>
              <pre className="markdown-proposals-pre">{clip(p.new_content)}</pre>
            </details>
            <div className="markdown-proposals-actions">
              <button
                type="button"
                className="markdown-proposals-btn reject"
                disabled={busyId === p.id}
                onClick={() => handleReject(p.id)}
              >
                Reject
              </button>
              <button
                type="button"
                className="markdown-proposals-btn accept"
                disabled={busyId === p.id}
                onClick={() => handleAccept(p.id)}
              >
                {busyId === p.id ? "…" : "Accept"}
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
