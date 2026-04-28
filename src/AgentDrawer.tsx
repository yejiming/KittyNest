import {
  Bot,
  ChevronDown,
  ChevronUp,
  CircleStop,
  ListTodo,
  Send,
  Settings,
  Wrench,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState, type CSSProperties, type PointerEvent as ReactPointerEvent } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  isTauriRuntime,
  resolveAgentAskUser,
  resolveAgentPermission,
  startAgentRun,
  stopAgentRun,
} from "./api";
import {
  contextShareLabel,
  messageFromAgentEvent,
  normalizeAgentContext,
  type AgentAskQuestion,
  type AgentContextSnapshot,
  type AgentEvent,
  type AgentMessage,
  type AgentOption,
  type AgentTodoItem,
} from "./agentTypes";
import type { ProjectRecord } from "./types";

const minDrawerWidth = 360;
const defaultDrawerWidth = 460;
const sessionStorageKey = "kittynest:agent-session";

const emptyContext = normalizeAgentContext({});

interface AgentDrawerProps {
  open: boolean;
  projects: ProjectRecord[];
  onClose: () => void;
}

export function AgentDrawer({ open, projects, onClose }: AgentDrawerProps) {
  const reviewedProjects = useMemo(
    () => projects.filter((project) => project.reviewStatus === "reviewed"),
    [projects],
  );
  const [sessionId] = useState(getOrCreateSessionId);
  const [selectedProject, setSelectedProject] = useState(reviewedProjects[0]?.slug ?? "");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [width, setWidth] = useState(defaultDrawerWidth);
  const [input, setInput] = useState("");
  const [running, setRunning] = useState(false);
  const [messages, setMessages] = useState<AgentMessage[]>([]);
  const [context, setContext] = useState<AgentContextSnapshot>(emptyContext);
  const [todos, setTodos] = useState<AgentTodoItem[]>([]);
  const [todosOpen, setTodosOpen] = useState(true);

  useEffect(() => {
    if (reviewedProjects.length === 0) {
      setSelectedProject("");
      return;
    }
    setSelectedProject((current) =>
      reviewedProjects.some((project) => project.slug === current) ? current : reviewedProjects[0].slug,
    );
  }, [reviewedProjects]);

  const handleAgentEvent = useCallback((event: AgentEvent) => {
    if (event.sessionId !== sessionId) return;
    if (event.context) setContext(normalizeAgentContext(event.context));

    if (event.type === "token") {
      appendAssistantMessage(event.delta ?? "");
      return;
    }
    if (event.type === "thinking_status" || event.type === "thinking_delta") {
      upsertThinkingMessage(event);
      return;
    }
    if (event.type === "tool_start") {
      const message = messageFromAgentEvent(event);
      if (message) upsertMessage(message);
      return;
    }
    if (event.type === "tool_output" || event.type === "tool_end") {
      updateToolMessage(event);
      return;
    }
    if (event.type === "permission_request" || event.type === "ask_user_request") {
      const message = messageFromAgentEvent(event);
      if (message) upsertMessage(message);
      return;
    }
    if (event.type === "todo_update") {
      setTodos(normalizeTodos(event.todos));
      setTodosOpen(true);
      return;
    }
    if (event.type === "done") {
      setRunning(false);
      finishAssistantMessage("done", event.reply);
      return;
    }
    if (event.type === "cancelled") {
      setRunning(false);
      finishAssistantMessage("cancelled");
      return;
    }
    if (event.type === "error") {
      setRunning(false);
      setMessages((current) => [
        ...current,
        { id: `error-${Date.now()}`, role: "error", content: event.error ?? "Agent run failed" },
      ]);
    }
  }, [sessionId]);

  useEffect(() => {
    let disposed = false;
    const browserHandler = (event: Event) => {
      handleAgentEvent((event as CustomEvent<AgentEvent>).detail);
    };
    window.addEventListener("kittynest-agent-event", browserHandler);

    let unlisten: (() => void) | undefined;
    if (isTauriRuntime()) {
      void import("@tauri-apps/api/event").then(({ listen }) =>
        listen<AgentEvent>("agent://event", (event) => handleAgentEvent(event.payload)),
      ).then((cleanup) => {
        if (disposed) {
          cleanup();
          return;
        }
        unlisten = cleanup;
      });
    }

    return () => {
      disposed = true;
      window.removeEventListener("kittynest-agent-event", browserHandler);
      unlisten?.();
    };
  }, [handleAgentEvent]);

  function appendAssistantMessage(delta: string) {
    if (!delta) return;
    setMessages((current) => {
      const last = current[current.length - 1];
      if (last?.role === "assistant" && last.status === "running") {
        return [
          ...current.slice(0, -1),
          { ...last, content: `${last.content ?? ""}${delta}` },
        ];
      }
      return [
        ...current,
        { id: `assistant-${Date.now()}`, role: "assistant", content: delta, status: "running" },
      ];
    });
  }

  function finishAssistantMessage(status: string, finalContent?: string) {
    setMessages((current) => {
      let finished = false;
      const next = current.map((message) => {
        if (message.role !== "assistant" || message.status !== "running") return message;
        finished = true;
        return {
          ...message,
          content: finalContent ?? message.content,
          status,
        };
      });
      if (finished || !finalContent) return next;
      const last = next[next.length - 1];
      if (last?.role === "assistant" && last.status === status && last.content === finalContent) {
        return next;
      }
      return [
        ...next,
        { id: `assistant-${Date.now()}`, role: "assistant", content: finalContent, status },
      ];
    });
  }

  function upsertThinkingMessage(event: AgentEvent) {
    setMessages((current) => {
      const runningIndex = findLastMessageIndex(
        current,
        (message) => message.role === "thinking" && message.status === "running",
      );
      const lastThinkingIndex = findLastMessageIndex(current, (message) => message.role === "thinking");
      const index = runningIndex >= 0
        ? runningIndex
        : event.type === "thinking_status" && event.status !== "running"
          ? lastThinkingIndex
          : -1;
      const nextContent = event.type === "thinking_delta" ? event.delta ?? "" : "";
      if (index >= 0) {
        const next = [...current];
        next[index] = {
          ...next[index],
          content: event.type === "thinking_delta" ? `${next[index].content ?? ""}${nextContent}` : next[index].content,
          status: event.status ?? next[index].status,
        };
        return next;
      }
      if (event.type === "thinking_status" && event.status !== "running") {
        return current;
      }
      return [
        ...current,
        {
          id: `thinking-${Date.now()}`,
          role: "thinking",
          content: nextContent,
          status: event.status ?? "running",
          expanded: false,
        },
      ];
    });
  }

  function upsertMessage(message: AgentMessage) {
    setMessages((current) => {
      const index = current.findIndex((item) => item.id === message.id);
      if (index < 0) return [...current, message];
      const next = [...current];
      next[index] = { ...next[index], ...message };
      return next;
    });
  }

  function updateToolMessage(event: AgentEvent) {
    setMessages((current) => {
      const index = current.findIndex((message) => message.role === "tool" && message.toolCallId === event.toolCallId);
      const output = event.type === "tool_end" ? event.resultPreview ?? "" : event.delta ?? "";
      if (index < 0) {
        return [
          ...current,
          {
            id: `tool-${event.toolCallId || Date.now()}`,
            role: "tool",
            toolCallId: event.toolCallId,
            name: event.name ?? "tool",
            status: event.type === "tool_end" ? event.status ?? "done" : "running",
            summary: event.summary ?? event.name ?? "Tool",
            output,
            resultPreview: event.resultPreview ?? "",
            expanded: false,
          },
        ];
      }
      const next = [...current];
      const message = next[index];
      next[index] = {
        ...message,
        status: event.type === "tool_end" ? event.status ?? "done" : message.status,
        output: event.type === "tool_end" && !message.output
          ? event.resultPreview ?? ""
          : `${message.output ?? ""}${event.delta ?? ""}`,
        resultPreview: event.resultPreview ?? message.resultPreview,
      };
      return next;
    });
  }

  async function submitMessage() {
    const trimmed = input.trim();
    if (!trimmed || running || !selectedProject) return;

    setMessages((current) => [
      ...current,
      { id: `user-${Date.now()}`, role: "user", content: trimmed },
    ]);
    setInput("");
    setRunning(true);
    try {
      await startAgentRun(sessionId, selectedProject, trimmed);
    } catch (error) {
      setRunning(false);
      setMessages((current) => [
        ...current,
        { id: `error-${Date.now()}`, role: "error", content: error instanceof Error ? error.message : String(error) },
      ]);
    }
  }

  async function stopRun() {
    await stopAgentRun(sessionId);
    setRunning(false);
  }

  function startResize(event: ReactPointerEvent<HTMLDivElement>) {
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = width;
    const move = (moveEvent: PointerEvent) => {
      setWidth(Math.max(minDrawerWidth, startWidth + startX - moveEvent.clientX));
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
  }

  async function answerPermission(message: AgentMessage, option: AgentOption) {
    await resolveAgentPermission(
      sessionId,
      message.requestId ?? "",
      option.value || option.label,
      option.supplementalInfo ?? "",
    );
    setMessages((current) => current.filter((item) => item.id !== message.id));
  }

  async function answerAskUser(message: AgentMessage) {
    const answers = Object.fromEntries((message.questions ?? []).map((question) => {
      const selected = question.options
        .filter((option) => option.selected)
        .map((option) => option.value || option.label);
      const value = question.multiSelect ? selected : selected[0] ?? question.freeformInput;
      return [question.header || question.question, question.freeformInput || value || ""];
    }));
    await resolveAgentAskUser(sessionId, message.requestId ?? "", answers);
    setMessages((current) => current.filter((item) => item.id !== message.id));
  }

  function updateAskQuestion(messageId: string, questionIndex: number, update: (question: AgentAskQuestion) => AgentAskQuestion) {
    setMessages((current) => current.map((message) => {
      if (message.id !== messageId || !message.questions) return message;
      return {
        ...message,
        questions: message.questions.map((question, index) => index === questionIndex ? update(question) : question),
      };
    }));
  }

  const contextPercent = context.maxTokens > 0
    ? Math.min(100, (context.usedTokens / context.maxTokens) * 100)
    : 0;

  return (
    <>
      <aside
        aria-label="Agent Assistant"
        className={`agent-drawer ${open ? "open" : ""}`}
        role="complementary"
        style={{ width: open ? width : 0 }}
      >
        <div className="agent-drawer-resizer" onPointerDown={startResize} />
        <header className="agent-drawer-header">
          <div>
            <span className="agent-kicker"><Bot size={14} /> Task Assistant</span>
            <strong>Agent Assistant</strong>
          </div>
          <div className="agent-header-actions">
            <button aria-label="Close assistant" className="agent-icon-button" onClick={onClose}>
              <X size={17} />
            </button>
          </div>
        </header>

        <div className="agent-message-list">
          {messages.length === 0 && (
            <div className="agent-empty">
              <Bot size={24} />
              <strong>Ready for reviewed project work.</strong>
              <span>{selectedProject || "Review a project before starting."}</span>
            </div>
          )}
          {messages.map((message) => (
            <AgentMessageView
              key={message.id}
              message={message}
              onToggle={() => setMessages((current) => current.map((item) =>
                item.id === message.id ? { ...item, expanded: !item.expanded } : item,
              ))}
              onPermission={(option) => void answerPermission(message, option)}
              onAskSubmit={() => void answerAskUser(message)}
              onAskChange={(questionIndex, value) => updateAskQuestion(message.id, questionIndex, (question) => ({
                ...question,
                freeformInput: value,
              }))}
              onAskOption={(questionIndex, optionIndex) => updateAskQuestion(message.id, questionIndex, (question) => ({
                ...question,
                options: question.options.map((option, index) => ({
                  ...option,
                  selected: question.multiSelect
                    ? index === optionIndex ? !option.selected : option.selected
                    : index === optionIndex,
                })),
              }))}
            />
          ))}
        </div>

        <footer className="agent-composer">
          {todos.length > 0 && (
            <section className={`agent-todo-card ${todosOpen ? "open" : "collapsed"}`}>
              <button className="agent-todo-toggle" onClick={() => setTodosOpen((current) => !current)}>
                <ListTodo size={15} />
                <span>TODO</span>
                {todosOpen ? <ChevronDown size={14} /> : <ChevronUp size={14} />}
              </button>
              {todosOpen && (
                <ul>
                  {todos.map((todo) => (
                    <li key={todo.content} className={todo.status}>
                      <span />
                      <strong>{todo.content}</strong>
                    </li>
                  ))}
                </ul>
              )}
            </section>
          )}

          <label className="sr-only" htmlFor="agent-message-input">Message Task Assistant</label>
          <textarea
            id="agent-message-input"
            aria-label="Message Task Assistant"
            disabled={!selectedProject}
            onChange={(event) => setInput(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
                event.preventDefault();
                void submitMessage();
              }
            }}
            placeholder={selectedProject ? "Ask the assistant..." : "Select a reviewed project first"}
            value={input}
          />
          <div className="agent-composer-actions">
            <button aria-label="Assistant settings" className="agent-round-button" onClick={() => setSettingsOpen(true)}>
              <Settings size={18} />
            </button>
            <div className="agent-send-cluster">
              <ContextRing context={context} percent={contextPercent} />
              <button
                aria-label={running ? "Stop" : "Send"}
                className="agent-round-button agent-send-button"
                disabled={!running && (!input.trim() || !selectedProject)}
                onClick={() => running ? void stopRun() : void submitMessage()}
              >
                {running ? <CircleStop size={19} /> : <Send size={18} />}
              </button>
            </div>
          </div>
        </footer>
      </aside>

      {settingsOpen && (
        <div className="agent-modal-backdrop" role="presentation">
          <section aria-label="Assistant settings" className="agent-settings-modal" role="dialog">
            <header>
              <strong>Assistant</strong>
              <button aria-label="Close assistant settings" className="agent-icon-button" onClick={() => setSettingsOpen(false)}>
                <X size={16} />
              </button>
            </header>
            <div className="agent-tabs" role="tablist">
              <button aria-selected="true" role="tab">Task Assistant</button>
            </div>
            <label className="agent-field">
              <span>Project</span>
              <select
                aria-label="Project"
                disabled={reviewedProjects.length === 0}
                onChange={(event) => setSelectedProject(event.target.value)}
                value={selectedProject}
              >
                {reviewedProjects.length === 0 && <option value="">No reviewed projects</option>}
                {reviewedProjects.map((project) => (
                  <option key={project.slug} value={project.slug}>{project.displayTitle || project.slug}</option>
                ))}
              </select>
            </label>
          </section>
        </div>
      )}
    </>
  );
}

function AgentMessageView({
  message,
  onToggle,
  onPermission,
  onAskSubmit,
  onAskChange,
  onAskOption,
}: {
  message: AgentMessage;
  onToggle: () => void;
  onPermission: (option: AgentOption) => void;
  onAskSubmit: () => void;
  onAskChange: (questionIndex: number, value: string) => void;
  onAskOption: (questionIndex: number, optionIndex: number) => void;
}) {
  if (message.role === "user") {
    return <article className="agent-message user">{message.content}</article>;
  }
  if (message.role === "assistant") {
    return (
      <article className="agent-message assistant">
        <ReactMarkdown remarkPlugins={[remarkGfm]}>{message.content ?? ""}</ReactMarkdown>
      </article>
    );
  }
  if (message.role === "thinking" || message.role === "tool") {
    const title = message.role === "thinking" ? "Thinking" : message.summary || message.name || "Tool";
    return (
      <article className={`agent-fold-card ${message.role}`}>
        <button onClick={onToggle}>
          {message.role === "tool" && <Wrench size={14} />}
          <strong>{title}</strong>
          <span>{message.role === "tool" ? message.status : message.content || message.status}</span>
          {message.expanded ? <ChevronDown size={14} /> : <ChevronUp size={14} />}
        </button>
        {message.expanded && (
          <pre>{message.role === "tool"
            ? [message.argumentsText, message.output || message.resultPreview].filter(Boolean).join("\n\n")
            : message.content}</pre>
        )}
      </article>
    );
  }
  if (message.role === "permission") {
    return (
      <article className="agent-action-card permission">
        <strong>{message.title}</strong>
        <p>{message.description}</p>
        <div>
          {(message.options ?? []).map((option) => (
            <button key={option.value || option.label} onClick={() => onPermission(option)}>
              {option.label}
            </button>
          ))}
        </div>
      </article>
    );
  }
  if (message.role === "ask_user") {
    return (
      <article className="agent-action-card ask-user">
        <strong>{message.title}</strong>
        {(message.questions ?? []).map((question, questionIndex) => (
          <label key={`${question.header}-${questionIndex}`} className="agent-question">
            <span>{question.question || question.header}</span>
            <div>
              {question.options.map((option, optionIndex) => (
                <button
                  className={option.selected ? "selected" : ""}
                  key={option.value || option.label}
                  onClick={() => onAskOption(questionIndex, optionIndex)}
                  type="button"
                >
                  {option.label}
                </button>
              ))}
            </div>
            {question.allowFreeformInput && (
              <textarea
                aria-label={question.header}
                onChange={(event) => onAskChange(questionIndex, event.target.value)}
                value={question.freeformInput}
              />
            )}
          </label>
        ))}
        <button className="agent-submit-answer" onClick={onAskSubmit}>Submit</button>
      </article>
    );
  }
  return <article className="agent-message error">{message.content}</article>;
}

function ContextRing({ context, percent }: { context: AgentContextSnapshot; percent: number }) {
  return (
    <div
      aria-label="Assistant context usage"
      className="agent-context-ring"
      role="status"
      style={{ "--context-fill": `${percent}%` } as CSSProperties}
    >
      <span>{Math.round(percent)}%</span>
      <div className="agent-context-tooltip" role="tooltip">
        <strong>Context</strong>
        <span>{context.usedTokens} / {context.maxTokens || 0} tokens</span>
        <span>System: {contextShareLabel(context, "system")}</span>
        <span>User: {contextShareLabel(context, "user")}</span>
        <span>Assistant: {contextShareLabel(context, "assistant")}</span>
        <span>Thinking: {contextShareLabel(context, "thinking")}</span>
        <span>Tool: {contextShareLabel(context, "tool")}</span>
      </div>
    </div>
  );
}

function findLastMessageIndex(messages: AgentMessage[], predicate: (message: AgentMessage) => boolean) {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    if (predicate(messages[index])) return index;
  }
  return -1;
}

function normalizeTodos(value: unknown): AgentTodoItem[] {
  if (!Array.isArray(value)) return [];
  return value.map((item) => {
    const todo = item && typeof item === "object" ? (item as Record<string, unknown>) : {};
    const status = String(todo.status ?? "pending");
    const normalizedStatus: AgentTodoItem["status"] =
      status === "in_progress" || status === "completed" ? status : "pending";
    return {
      content: String(todo.content ?? ""),
      activeForm: String(todo.activeForm ?? todo.active_form ?? todo.content ?? ""),
      status: normalizedStatus,
    };
  }).filter((todo) => todo.content);
}

function getOrCreateSessionId() {
  const existing = window.sessionStorage.getItem(sessionStorageKey);
  if (existing) return existing;
  const generated = window.crypto?.randomUUID?.() ?? `agent-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  window.sessionStorage.setItem(sessionStorageKey, generated);
  return generated;
}
