import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import FolderSelector from "./FolderSelector";
import ProjectExplorer from "./ProjectExplorer";
import MarkdownEditor from "./MarkdownEditor";
import ChatPanel from "./ChatPanel";
import ScanResultsPanel from "./ScanResultsPanel";
import MarkdownProposalsPanel from "./MarkdownProposalsPanel";

export default function AppLayout() {
  const [vaultPath, setVaultPath] = useState(null);
  const [explorerCollapsed, setExplorerCollapsed] = useState(false);
  const [chatCollapsed, setChatCollapsed] = useState(false);
  const [openFilePath, setOpenFilePath] = useState(null);
  const [scanResults, setScanResults] = useState(null);
  const [resultsPanelCollapsed, setResultsPanelCollapsed] = useState(true);
  const [markdownProposals, setMarkdownProposals] = useState([]);
  const [markdownPanelCollapsed, setMarkdownPanelCollapsed] = useState(false);

  const handleFolderSelected = (path) => {
    setVaultPath(path);
    setOpenFilePath(null);
  };

  useEffect(() => {
    const unlisten = listen("scan-result", (event) => {
      setScanResults(event.payload);
      setResultsPanelCollapsed(false);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const unlisten = listen("markdown-proposal", (event) => {
      const payload = event.payload;
      setMarkdownProposals((prev) => {
        const next = [...prev];
        const idx = next.findIndex((p) => p.id === payload.id);
        if (idx >= 0) next[idx] = payload;
        else next.push(payload);
        return next;
      });
      setMarkdownPanelCollapsed(false);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    if (!vaultPath) {
      setMarkdownProposals([]);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const pending = await invoke("list_pending_markdown_proposals");
        if (
          cancelled ||
          !Array.isArray(pending) ||
          pending.length === 0
        ) {
          return;
        }
        setMarkdownProposals(pending);
        setMarkdownPanelCollapsed(false);
      } catch {
        /* ignore */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [vaultPath]);

  const handleRemoveProposal = (id) => {
    setMarkdownProposals((prev) => prev.filter((p) => p.id !== id));
  };

  const handleCloseResults = () => {
    setResultsPanelCollapsed(true);
    setScanResults(null);
  };

  return (
    <div className="app-layout">
      <div className="app-layout-main">
      {/* Left sidebar - Project Explorer */}
      <aside
        className={`sidebar sidebar-left ${explorerCollapsed ? "collapsed" : ""}`}
        onClick={explorerCollapsed ? () => setExplorerCollapsed(false) : undefined}
        role={explorerCollapsed ? "button" : undefined}
        tabIndex={explorerCollapsed ? 0 : undefined}
        onKeyDown={explorerCollapsed ? (e) => e.key === "Enter" && setExplorerCollapsed(false) : undefined}
        title={explorerCollapsed ? "Expand Explorer (click to expand)" : undefined}
      >
        <div className="sidebar-header">
          <span className="sidebar-title">Explorer</span>
          <button
            className="sidebar-toggle"
            onClick={(e) => {
              e.stopPropagation();
              setExplorerCollapsed(!explorerCollapsed);
            }}
            title={explorerCollapsed ? "Expand" : "Collapse"}
          >
            {explorerCollapsed ? "›" : "‹"}
          </button>
        </div>
        {!explorerCollapsed && (
          <div className="sidebar-content">
            <FolderSelector
              onFolderSelected={handleFolderSelected}
              vaultPath={vaultPath}
              compact
            />
            <ProjectExplorer
              vaultPath={vaultPath}
              openFilePath={openFilePath}
              onFileSelect={setOpenFilePath}
              onScanResults={(type, pairs) => {
                setScanResults({ type, pairs });
                setResultsPanelCollapsed(false);
              }}
            />
          </div>
        )}
      </aside>

      {/* Main content - Markdown Editor */}
      <main className="main-content">
        <MarkdownEditor
          filePath={openFilePath}
          vaultPath={vaultPath}
        />
      </main>

      {/* Right sidebar - Chat Panel */}
      <aside
        className={`sidebar sidebar-right ${chatCollapsed ? "collapsed" : ""}`}
        onClick={chatCollapsed ? () => setChatCollapsed(false) : undefined}
        role={chatCollapsed ? "button" : undefined}
        tabIndex={chatCollapsed ? 0 : undefined}
        onKeyDown={chatCollapsed ? (e) => e.key === "Enter" && setChatCollapsed(false) : undefined}
        title={chatCollapsed ? "Expand Chat (click to expand)" : undefined}
      >
        <div className="sidebar-header">
          <span className="sidebar-title">Chat</span>
          <button
            className="sidebar-toggle"
            onClick={(e) => {
              e.stopPropagation();
              setChatCollapsed(!chatCollapsed);
            }}
            title={chatCollapsed ? "Expand" : "Collapse"}
          >
            {chatCollapsed ? "‹" : "›"}
          </button>
        </div>
        <div className="sidebar-content">
          <ChatPanel vaultPath={vaultPath} openFilePath={openFilePath} />
        </div>
      </aside>
      </div>

      <div className="bottom-panels-stack">
      {/* Bottom panel - Scan results */}
      {scanResults && !resultsPanelCollapsed && (
        <div className="scan-results-container">
          <ScanResultsPanel
            results={scanResults}
            onClose={handleCloseResults}
            onFileOpen={setOpenFilePath}
          />
        </div>
      )}

      {/* Bottom panel - Markdown proposals */}
      {markdownProposals.length > 0 && !markdownPanelCollapsed && (
        <div className="markdown-proposals-container">
          <MarkdownProposalsPanel
            proposals={markdownProposals}
            onRemove={handleRemoveProposal}
            onAppliedOpenFile={setOpenFilePath}
            onClosePanel={() => setMarkdownPanelCollapsed(true)}
          />
        </div>
      )}
      </div>
    </div>
  );
}
