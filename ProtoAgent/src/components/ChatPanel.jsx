import { useState, useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

export default function ChatPanel({ vaultPath, openFilePath }) {
  const [messages, setMessages] = useState([]);
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const messagesEndRef = useRef(null);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages]);

  const handleSend = async () => {
    const text = input.trim();
    if (!text || loading) return;

    setInput("");
    setError(null);
    setMessages((prev) => [...prev, { role: "user", content: text }]);
    setLoading(true);

    try {
      const response = await invoke("ask_agent", {
        prompt: text,
        vaultPath: vaultPath ?? null,
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
    }
  };

  const handleKeyDown = (e) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  return (
    <div className="chat-panel">
      <div className="chat-messages">
        {messages.length === 0 && (
          <div className="chat-welcome">
            <p>Ask the AI anything. Try:</p>
            <ul>
              <li>Summarize the current note</li>
              <li>Find duplicate files in my vault</li>
              <li>Find related files</li>
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
            <div className="chat-message-content chat-loading">
              <span>Thinking...</span>
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
  );
}
