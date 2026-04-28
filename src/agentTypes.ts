export type AgentEventType =
  | "token"
  | "thinking_status"
  | "thinking_delta"
  | "tool_start"
  | "tool_output"
  | "tool_end"
  | "todo_update"
  | "permission_request"
  | "ask_user_request"
  | "create_task_request"
  | "done"
  | "cancelled"
  | "error";

export interface AgentContextBreakdown {
  system: number;
  user: number;
  assistant: number;
  tool: number;
}

export interface AgentContextSnapshot {
  usedTokens: number;
  maxTokens: number;
  remainingTokens: number;
  thinkingTokens: number;
  breakdown: AgentContextBreakdown;
}

export interface AgentTodoItem {
  content: string;
  activeForm: string;
  status: "pending" | "in_progress" | "completed";
}

export interface AgentOption {
  label: string;
  value?: string;
  description?: string;
  recommended?: boolean;
  selected?: boolean;
  supplementalInfo?: string;
  inputText?: string;
}

export interface AgentAskQuestion {
  header: string;
  question: string;
  multiSelect: boolean;
  allowFreeformInput: boolean;
  options: AgentOption[];
  freeformInput: string;
}

export interface AgentEvent {
  sessionId: string;
  type: AgentEventType;
  delta?: string;
  status?: string;
  reply?: string;
  error?: string;
  context?: unknown;
  todos?: unknown[];
  toolCallId?: string;
  name?: string;
  summary?: string;
  arguments?: Record<string, unknown>;
  resultPreview?: string;
  requestId?: string;
  title?: string;
  description?: string;
  options?: unknown[];
  questions?: unknown[];
}

export interface AgentMessage {
  id: string;
  role: "user" | "assistant" | "thinking" | "tool" | "permission" | "ask_user" | "error";
  content?: string;
  status?: string;
  expanded?: boolean;
  expandedByUser?: boolean;
  toolCallId?: string;
  name?: string;
  summary?: string;
  arguments?: Record<string, unknown>;
  argumentsText?: string;
  output?: string;
  resultPreview?: string;
  requestId?: string;
  title?: string;
  description?: string;
  options?: AgentOption[];
  questions?: AgentAskQuestion[];
}

export interface AgentTimelinePayload {
  version: 1;
  sessionId: string;
  projectSlug: string;
  messages: AgentMessage[];
  todos: AgentTodoItem[];
  context: AgentContextSnapshot;
}

export interface SavedAgentSession {
  version: number;
  sessionId: string;
  projectSlug: string;
  projectRoot: string;
  createdAt: string;
  messages: AgentMessage[];
  todos: AgentTodoItem[];
  context: AgentContextSnapshot;
  llmMessages: unknown[];
}

const emptyBreakdown: AgentContextBreakdown = {
  system: 0,
  user: 0,
  assistant: 0,
  tool: 0,
};

function numberField(value: unknown): number {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed >= 0 ? Math.round(parsed) : 0;
}

export function normalizeAgentContext(value: unknown): AgentContextSnapshot {
  const source = value && typeof value === "object" ? (value as Record<string, unknown>) : {};
  const rawBreakdown = source.breakdown && typeof source.breakdown === "object"
    ? (source.breakdown as Record<string, unknown>)
    : {};
  const breakdown = {
    system: numberField(rawBreakdown.system),
    user: numberField(rawBreakdown.user),
    assistant: numberField(rawBreakdown.assistant),
    tool: numberField(rawBreakdown.tool),
  };
  const breakdownTotal = breakdown.system + breakdown.user + breakdown.assistant + breakdown.tool;
  const usedTokens = Math.max(numberField(source.usedTokens ?? source.used_tokens), breakdownTotal);
  const maxTokens = numberField(source.maxTokens ?? source.max_tokens);
  const remainingTokens = maxTokens > 0
    ? Math.max(0, maxTokens - usedTokens)
    : numberField(source.remainingTokens ?? source.remaining_tokens);
  return {
    usedTokens,
    maxTokens,
    remainingTokens,
    thinkingTokens: numberField(source.thinkingTokens ?? source.thinking_tokens),
    breakdown: { ...emptyBreakdown, ...breakdown },
  };
}

export function contextShareLabel(
  context: AgentContextSnapshot,
  key: keyof AgentContextBreakdown | "thinking",
): string {
  const value = key === "thinking" ? context.thinkingTokens : context.breakdown[key];
  const percent = context.usedTokens > 0 ? (value / context.usedTokens) * 100 : 0;
  return `${percent.toFixed(1)}%`;
}

export function messageFromAgentEvent(event: AgentEvent): AgentMessage | null {
  if (event.type === "tool_start") {
    const toolCallId = String(event.toolCallId ?? "");
    const args = event.arguments && typeof event.arguments === "object" ? event.arguments : {};
    return {
      id: `tool-${toolCallId || Date.now()}`,
      role: "tool",
      toolCallId,
      name: String(event.name ?? "tool"),
      status: "running",
      summary: String(event.summary ?? event.name ?? "tool"),
      arguments: args,
      argumentsText: JSON.stringify(args, null, 2),
      output: "",
      resultPreview: "",
      expanded: false,
      expandedByUser: false,
    };
  }
  if (event.type === "permission_request") {
    return {
      id: `permission-${String(event.requestId ?? Date.now())}`,
      role: "permission",
      requestId: String(event.requestId ?? ""),
      title: String(event.title ?? "Permission Required"),
      description: String(event.description ?? ""),
      options: normalizeOptions(event.options),
    };
  }
  if (event.type === "ask_user_request") {
    return {
      id: `ask-user-${String(event.requestId ?? Date.now())}`,
      role: "ask_user",
      requestId: String(event.requestId ?? ""),
      title: String(event.title ?? "Need your input"),
      questions: normalizeQuestions(event.questions),
    };
  }
  return null;
}

function normalizeOptions(value: unknown): AgentOption[] {
  if (!Array.isArray(value)) return [];
  return value.map((item) => {
    const option = item && typeof item === "object" ? (item as Record<string, unknown>) : {};
    return {
      label: String(option.label ?? option.value ?? "Option"),
      value: String(option.value ?? option.label ?? ""),
      description: String(option.description ?? ""),
      recommended: Boolean(option.recommended),
      supplementalInfo: String(option.supplementalInfo ?? option.supplemental_info ?? ""),
      selected: false,
      inputText: "",
    };
  });
}

function normalizeQuestions(value: unknown): AgentAskQuestion[] {
  if (!Array.isArray(value)) return [];
  return value.map((item) => {
    const question = item && typeof item === "object" ? (item as Record<string, unknown>) : {};
    return {
      header: String(question.header ?? "Question"),
      question: String(question.question ?? ""),
      multiSelect: Boolean(question.multiSelect),
      allowFreeformInput: question.allowFreeformInput !== false,
      options: normalizeOptions(question.options),
      freeformInput: "",
    };
  });
}
