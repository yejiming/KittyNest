import { invoke } from "@tauri-apps/api/core";
import type {
  AppState,
  CreateTaskResult,
  EnqueueJobResult,
  ImportResult,
  JobRecord,
  LlmSettings,
  MemoryEntityRecord,
  MemoryRelatedSession,
  MemorySearchRecord,
  ScanResult,
  SessionMemoryDetail,
} from "./types";

export type { CreateTaskResult } from "./types";

const fallbackState: AppState = {
  dataDir: "~/.kittynest",
  llmSettings: {
    id: "openrouter-default",
    remark: "Default",
    provider: "OpenRouter",
    baseUrl: "https://openrouter.ai/api/v1",
    interface: "openai",
    model: "",
    apiKey: "",
    maxContext: 128000,
    maxTokens: 4096,
    temperature: 0.2,
    models: [],
    scenarioModels: {
      defaultModel: "",
      projectModel: "",
      sessionModel: "",
      memoryModel: "",
      assistantModel: "",
    },
  },
  llmProviderCalls: [],
  providerPresets: [
    {
      provider: "OpenRouter",
      baseUrl: "https://openrouter.ai/api/v1",
      interface: "openai",
    },
  ],
  sourceStatuses: [
    { source: "Claude Code", path: "~/.claude", exists: false },
    { source: "Codex", path: "~/.codex", exists: false },
  ],
  stats: {
    activeProjects: 0,
    openTasks: 0,
    sessions: 0,
    unprocessedSessions: 0,
    memories: 0,
  },
  projects: [],
  tasks: [],
  sessions: [],
  jobs: [],
};

export function isTauriRuntime() {
  return "__TAURI_INTERNALS__" in window;
}

export async function getAppState(): Promise<AppState> {
  if (!isTauriRuntime()) {
    return fallbackState;
  }
  return invoke<AppState>("get_app_state");
}

export async function getCachedAppState(): Promise<AppState> {
  if (!isTauriRuntime()) {
    return fallbackState;
  }
  return invoke<AppState>("get_cached_app_state");
}

export async function scanSources(): Promise<ScanResult> {
  if (!isTauriRuntime()) {
    return { found: 0, inserted: 0, codexFound: 0, claudeFound: 0 };
  }
  return invoke<ScanResult>("scan_sources");
}

export async function importHistoricalSessions(): Promise<ImportResult> {
  const result = await enqueueAnalyzeSessions();
  return { projectsUpdated: 0, tasksCreated: 0, sessionsWritten: result.total };
}

export async function enqueueAnalyzeSessions(updatedAfter?: string): Promise<EnqueueJobResult> {
  if (!isTauriRuntime()) {
    return { jobId: 1, total: 0 };
  }
  return invoke<EnqueueJobResult>("enqueue_analyze_sessions", { updatedAfter });
}

export async function enqueueScanSources(): Promise<EnqueueJobResult> {
  if (!isTauriRuntime()) {
    return { jobId: 1, total: 1 };
  }
  return invoke<EnqueueJobResult>("enqueue_scan_sources");
}

export async function enqueueAnalyzeProjectSessions(projectSlug: string): Promise<EnqueueJobResult> {
  if (!isTauriRuntime()) {
    return { jobId: 1, total: 0 };
  }
  return invoke<EnqueueJobResult>("enqueue_analyze_project_sessions", { projectSlug });
}

export async function enqueueAnalyzeProject(projectSlug: string): Promise<EnqueueJobResult> {
  if (!isTauriRuntime()) {
    return { jobId: 1, total: 3 };
  }
  return invoke<EnqueueJobResult>("enqueue_analyze_project", { projectSlug });
}

export async function enqueueAnalyzeSession(sessionId: string): Promise<EnqueueJobResult> {
  if (!isTauriRuntime()) {
    return { jobId: 1, total: 1 };
  }
  return invoke<EnqueueJobResult>("enqueue_analyze_session", { sessionId });
}

export async function enqueueReviewProject(projectSlug: string): Promise<EnqueueJobResult> {
  if (!isTauriRuntime()) {
    return { jobId: 1, total: 1 };
  }
  return invoke<EnqueueJobResult>("enqueue_review_project", { projectSlug });
}

export async function getActiveJobs(): Promise<JobRecord[]> {
  if (!isTauriRuntime()) {
    return [];
  }
  const result = await invoke<{ jobs: JobRecord[] }>("get_active_jobs");
  return result.jobs;
}

export async function stopJob(jobId: number): Promise<{ stopped: boolean }> {
  if (!isTauriRuntime()) {
    return { stopped: true };
  }
  return invoke<{ stopped: boolean }>("stop_job", { jobId });
}

export async function readMarkdownFile(path: string): Promise<{ content: string }> {
  if (!isTauriRuntime()) {
    return { content: "" };
  }
  return invoke<{ content: string }>("read_markdown_file", { path });
}

export async function reviewProject(projectSlug: string): Promise<{ infoPath: string }> {
  if (!isTauriRuntime()) {
    return { infoPath: "" };
  }
  return invoke<{ infoPath: string }>("review_project", { projectSlug });
}

export async function saveLlmSettings(settings: LlmSettings): Promise<{ saved: boolean }> {
  if (!isTauriRuntime()) {
    return { saved: true };
  }
  return invoke<{ saved: boolean }>("save_llm_settings", { settings });
}

export async function updateTaskStatus(
  projectSlug: string,
  taskSlug: string,
  status: string,
): Promise<{ updated: boolean }> {
  if (!isTauriRuntime()) {
    return { updated: true };
  }
  return invoke<{ updated: boolean }>("update_task_status", {
    projectSlug,
    taskSlug,
    status,
  });
}

export async function createTask(projectSlug: string, userPrompt: string): Promise<CreateTaskResult> {
  if (!isTauriRuntime()) {
    return {
      projectSlug,
      taskSlug: "task",
      jobId: 1,
      total: 1,
      userPromptPath: "",
      llmPromptPath: "",
    };
  }
  return invoke<CreateTaskResult>("create_task", {
    projectSlug,
    userPrompt,
  });
}

export async function deleteTask(projectSlug: string, taskSlug: string): Promise<{ deleted: boolean }> {
  if (!isTauriRuntime()) {
    return { deleted: true };
  }
  return invoke<{ deleted: boolean }>("delete_task", {
    projectSlug,
    taskSlug,
  });
}

export async function startAgentRun(
  sessionId: string,
  projectSlug: string,
  message: string,
): Promise<{ started: boolean }> {
  if (!isTauriRuntime()) {
    return { started: true };
  }
  return invoke<{ started: boolean }>("start_agent_run", { sessionId, projectSlug, message });
}

export async function stopAgentRun(sessionId: string): Promise<{ stopped: boolean }> {
  if (!isTauriRuntime()) {
    return { stopped: true };
  }
  return invoke<{ stopped: boolean }>("stop_agent_run", { sessionId });
}

export async function clearAgentSession(sessionId: string): Promise<{ cleared: boolean }> {
  if (!isTauriRuntime()) {
    return { cleared: true };
  }
  return invoke<{ cleared: boolean }>("clear_agent_session", { sessionId });
}

export async function resolveAgentPermission(
  sessionId: string,
  requestId: string,
  value: string,
  supplementalInfo = "",
): Promise<{ resolved: boolean }> {
  if (!isTauriRuntime()) {
    return { resolved: true };
  }
  return invoke<{ resolved: boolean }>("resolve_agent_permission", {
    sessionId,
    requestId,
    value,
    supplementalInfo,
  });
}

export async function resolveAgentAskUser(
  sessionId: string,
  requestId: string,
  answers: Record<string, unknown>,
): Promise<{ resolved: boolean }> {
  if (!isTauriRuntime()) {
    return { resolved: true };
  }
  return invoke<{ resolved: boolean }>("resolve_agent_ask_user", {
    sessionId,
    requestId,
    answers,
  });
}

export async function resetSessions(): Promise<{ reset: number }> {
  if (!isTauriRuntime()) {
    return { reset: 0 };
  }
  return invoke<{ reset: number }>("reset_sessions");
}

export async function resetProjects(): Promise<{ reset: number }> {
  if (!isTauriRuntime()) {
    return { reset: 0 };
  }
  return invoke<{ reset: number }>("reset_projects");
}

export async function resetTasks(): Promise<{ reset: number }> {
  if (!isTauriRuntime()) {
    return { reset: 0 };
  }
  return invoke<{ reset: number }>("reset_tasks");
}

export async function resetMemories(): Promise<{ reset: number }> {
  if (!isTauriRuntime()) {
    return { reset: 0 };
  }
  return invoke<{ reset: number }>("reset_memories");
}

export async function enqueueRebuildMemories(): Promise<EnqueueJobResult> {
  if (!isTauriRuntime()) {
    return { jobId: 1, total: 1 };
  }
  return invoke<EnqueueJobResult>("enqueue_rebuild_memories");
}

export async function rebuildMemories(): Promise<{ rebuilt: number }> {
  const result = await enqueueRebuildMemories();
  return { rebuilt: result.total };
}

export async function enqueueSearchMemories(query: string): Promise<EnqueueJobResult> {
  if (!isTauriRuntime()) {
    return { jobId: 1, total: 1 };
  }
  return invoke<EnqueueJobResult>("enqueue_search_memories", { query });
}

export async function getMemorySearch(): Promise<MemorySearchRecord | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  const result = await invoke<{ search: MemorySearchRecord | null }>("get_memory_search");
  return result.search;
}

export async function getSessionMemory(sessionId: string): Promise<SessionMemoryDetail> {
  if (!isTauriRuntime()) {
    return {
      sessionId,
      memoryPath: "",
      memories: [],
      relatedSessions: [],
    };
  }
  return invoke<SessionMemoryDetail>("get_session_memory", { sessionId });
}

export async function listMemoryEntities(): Promise<MemoryEntityRecord[]> {
  if (!isTauriRuntime()) {
    return [];
  }
  const result = await invoke<{ entities: MemoryEntityRecord[] }>("list_memory_entities");
  return result.entities;
}

export async function listEntitySessions(entity: string): Promise<MemoryRelatedSession[]> {
  if (!isTauriRuntime()) {
    return [];
  }
  const result = await invoke<{ sessions: MemoryRelatedSession[] }>("list_entity_sessions", { entity });
  return result.sessions;
}
