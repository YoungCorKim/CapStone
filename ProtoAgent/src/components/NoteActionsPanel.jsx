import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

const ENRICH_ACTIONS = [
  {
    mode: "expand_details",
    label: "Enrich",
    title: "Expand the current note with missing details",
  },
  {
    mode: "add_examples",
    label: "Add examples",
    title: "Add concise examples for important concepts",
  },
  {
    mode: "detect_missing_knowledge",
    label: "Missing knowledge",
    title: "Suggest additions for incomplete or unclear parts",
  },
  {
    mode: "add_definitions",
    label: "Definitions",
    title: "Add short definitions for important terms",
  },
];

export default function NoteActionsPanel({ vaultPath, filePath }) {
  const [busyAction, setBusyAction] = useState(null);
  const [error, setError] = useState(null);
  const [notice, setNotice] = useState(null);
  const [nextActions, setNextActions] = useState("");
  const [customTask, setCustomTask] = useState("");
  const [customOpen, setCustomOpen] = useState(false);

  if (!filePath) return null;

  const canRun = Boolean(vaultPath && filePath && !busyAction);

  const queueEnrichment = async (mode, task = null) => {
    if (!canRun) return;
    setBusyAction(mode);
    setError(null);
    setNotice(null);
    try {
      const proposal = await invoke("propose_enrich_file", {
        vaultPath,
        filePath,
        mode,
        customTask: task,
      });
      setNotice(`Queued proposal for ${proposal.relative_path}`);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusyAction(null);
    }
  };

  const handleSuggestNextActions = async () => {
    if (!canRun) return;
    setBusyAction("next_actions");
    setError(null);
    setNotice(null);
    try {
      const result = await invoke("suggest_next_actions", { filePath });
      setNextActions(result);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusyAction(null);
    }
  };

  const handleCustomSubmit = () => {
    const task = customTask.trim();
    if (!task) {
      setError("Custom enrichment needs instructions.");
      return;
    }
    queueEnrichment("custom", task);
  };

  return (
    <section className="note-actions-panel" aria-label="Note actions">
      <div className="note-actions-header">
        <div>
          <div className="note-actions-title">Note Actions</div>
          <div className="note-actions-subtitle">
            Enrich this note or ask for concrete next steps.
          </div>
        </div>
        {busyAction && (
          <span className="note-actions-status" aria-live="polite">
            Working...
          </span>
        )}
      </div>

      <div className="note-actions-buttons">
        {ENRICH_ACTIONS.map((action) => (
          <button
            key={action.mode}
            type="button"
            className="note-actions-btn"
            onClick={() => queueEnrichment(action.mode)}
            disabled={!canRun}
            title={action.title}
          >
            {busyAction === action.mode ? "Queueing..." : action.label}
          </button>
        ))}
        <button
          type="button"
          className="note-actions-btn secondary"
          onClick={handleSuggestNextActions}
          disabled={!canRun}
          title="Suggest what to do next based on this note"
        >
          {busyAction === "next_actions" ? "Thinking..." : "Next actions"}
        </button>
        <button
          type="button"
          className="note-actions-btn secondary"
          onClick={() => setCustomOpen((open) => !open)}
          disabled={Boolean(busyAction)}
        >
          Custom
        </button>
      </div>

      {customOpen && (
        <div className="note-actions-custom">
          <textarea
            value={customTask}
            onChange={(e) => setCustomTask(e.target.value)}
            placeholder="Describe how to enrich this note..."
            rows={2}
            disabled={Boolean(busyAction)}
          />
          <button
            type="button"
            className="note-actions-btn"
            onClick={handleCustomSubmit}
            disabled={!canRun || !customTask.trim()}
          >
            Queue custom enrichment
          </button>
        </div>
      )}

      {notice && <div className="note-actions-notice">{notice}</div>}
      {error && <div className="note-actions-error">{error}</div>}

      {nextActions && (
        <div className="note-actions-result">
          <div className="note-actions-result-title">Suggested Next Actions</div>
          <pre>{nextActions}</pre>
        </div>
      )}
    </section>
  );
}
