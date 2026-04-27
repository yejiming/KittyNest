import { invoke } from "@tauri-apps/api/core";
import type {
  AppState,
  CreateTaskResult,
  EnqueueJobResult,
  ImportResult,
  JobRecord,
  LlmSettings,
  ScanResult,
} from "./types";

export type { CreateTaskResult } from "./types";

const fallbackState: AppState = {
  dataDir: "~/.kittynest",
  llmSettings: {
    provider: "OpenRouter",
    baseUrl: "https://openrouter.ai/api/v1",
    interface: "openai",
    model: "",
    apiKey: "",
  },
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
    return { jobId: 1, total: 2 };
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
