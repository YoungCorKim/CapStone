import { useState, useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const CHAT_MODEL_STORAGE_KEY = "protoagent-chat-model";
const RAG_CONTEXT_K_STORAGE_KEY = "protoagent-rag-context-top-k";
const RAG_K_MIN = 1;
const RAG_K_MAX = 50;
const RAG_K_DEFAULT = 20;

function clampRagK(value) {
  const n = Math.round(Number(value));
  if (!Number.isFinite(n)) return RAG_K_DEFAULT;
  return Math.min(RAG_K_MAX, Math.max(RAG_K_MIN, n));
}

function readStoredRagK() {
  try {
    const raw = localStorage.getItem(RAG_CONTEXT_K_STORAGE_KEY);
    if (raw == null || raw === "") return RAG_K_DEFAULT;
    return clampRagK(raw);
  } catch {
    return RAG_K_DEFAULT;
  }
}

export default function ChatPanel({ vaultPath, openFilePath }) {
  const [messages, setMessages] = useState([]);
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [modelOptions, setModelOptions] = useState([]);
  const [selectedModel, setSelectedModel] = useState("");
  const [ragContextTopK, setRagContextTopK] = useState(() => readStoredRagK());
  const [progressMessage, setProgressMessage] = useState("");
  const messagesEndRef = useRef(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const opts = await invoke("list_chat_models");
        if (cancelled || !Array.isArray(opts) || opts.length === 0) return;
        setModelOptions(opts);
        const stored = localStorage.getItem(CHAT_MODEL_STORAGE_KEY);
        const ids = new Set(opts.map((o) => o.id));
        if (stored && ids.has(stored)) {
          setSelectedModel(stored);
        } else {
          const fallback =
            opts.find((o) => o.id === "gpt-4o-mini")?.id ?? opts[0].id;
          setSelectedModel(fallback);
          localStorage.setItem(CHAT_MODEL_STORAGE_KEY, fallback);
        }
      } catch {
        /* dropdown stays empty; ask_agent still defaults on backend */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const unlisten = listen("chat-progress", (event) => {
      const p = event.payload;
      if (p && typeof p.message === "string") {
        setProgressMessage(p.message);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages, loading, progressMessage]);

  const handleSend = async () => {
    const text = input.trim();
    if (!text || loading) return;

    setInput("");
    setError(null);
    setProgressMessage("");
    setMessages((prev) => [...prev, { role: "user", content: text }]);
    setLoading(true);

    try {
      const response = await invoke("ask_agent", {
        prompt: text,
        vaultPath: vaultPath ?? null,
        model: selectedModel.trim() ? selectedModel : null,
        ragContextTopK: ragContextTopK,
      });
      setMessages((prev) => [...prev, { role: "assistant", content: response }]);
    } catch (err) {
      setError(String(err));
      setMessages((prev) => [
        ...prev,
        { role: "assistant", content: null, error: String(err) },
      ]);
    } finally {
      setLoading(false);
      setProgressMessage("");
    }
  };

  const handleStop = () => {
    if (!loading) return;
    invoke("cancel_chat").catch(() => {});
  };

  const handleKeyDown = (e) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const handleModelChange = (e) => {
    const id = e.target.value;
    setSelectedModel(id);
    localStorage.setItem(CHAT_MODEL_STORAGE_KEY, id);
  };

  const handleRagKChange = (e) => {
    const next = clampRagK(e.target.value);
    setRagContextTopK(next);
    try {
      localStorage.setItem(RAG_CONTEXT_K_STORAGE_KEY, String(next));
    } catch {
      /* ignore */
    }
  };

  return (
    <div className="chat-panel">
      <div className="chat-settings-rows">
        {modelOptions.length > 0 && (
          <div className="chat-model-row">
            <label className="chat-model-label" htmlFor="chat-model-select">
              Model
            </label>
            <select
              id="chat-model-select"
              className="chat-model-select"
              value={selectedModel}
              onChange={handleModelChange}
              disabled={loading}
            >
              {modelOptions.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.label}
                </option>
              ))}
            </select>
          </div>
        )}
        <div className="chat-model-row">
          <label className="chat-model-label" htmlFor="rag-context-k">
            Vault context
          </label>
          <input
            id="rag-context-k"
            type="number"
            className="chat-rag-input"
            min={RAG_K_MIN}
            max={RAG_K_MAX}
            value={ragContextTopK}
            onChange={handleRagKChange}
            disabled={loading}
            title="Number of vault chunks to retrieve for RAG (1–50). Default 20."
          />
        </div>
      </div>
      <div className="chat-messages">
        {messages.length === 0 && (
          <div className="chat-welcome">
            <p>Ask the AI anything. Try:</p>
            <ul>
              <li>Summarize the current note</li>
              <li>Find duplicate files in my vault</li>
              <li>Find related files</li>
              <li>Create or edit a vault note (approve in the bottom panel)</li>
              <li>Get unstuck / plan next steps (development coach)</li>
              <li>Help me brainstorm ideas</li>
            </ul>
          </div>
        )}
        {messages.map((msg, i) => (
          <div key={i} className={`chat-message chat-message-${msg.role}`}>
            <span className="chat-message-role">
              {msg.role === "user" ? "You" : "AI"}
            </span>
            <div className="chat-message-content">
              {msg.error ? (
                <span className="chat-error">{msg.error}</span>
              ) : (
                <pre>{msg.content}</pre>
              )}
            </div>
          </div>
        ))}
        {loading && (
          <div className="chat-message chat-message-assistant">
            <span className="chat-message-role">AI</span>
            <div className="chat-message-content chat-loading" aria-live="polite">
              <span className="chat-progress-text">
                {progressMessage || "Thinking…"}
              </span>
            </div>
          </div>
        )}
        <div ref={messagesEndRef} />
      </div>
      {error && (
        <div className="chat-input-error">
          <p>{error}</p>
        </div>
      )}
      <div className="chat-input-area">
        <textarea
          className="chat-input"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Type a message... (Enter to send)"
          rows={2}
          disabled={loading}
        />
        <div className="chat-input-buttons">
          <button
            type="button"
            className="chat-stop-btn"
            onClick={handleStop}
            disabled={!loading}
            title="Stop generation"
          >
            Stop
          </button>
          <button
            className="chat-send-btn"
            onClick={handleSend}
            disabled={!input.trim() || loading}
            title="Send (Enter)"
          >
            Send
          </button>
        </div>
      </div>
    </div>
  );
}
