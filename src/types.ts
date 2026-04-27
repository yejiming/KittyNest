export interface ProviderPreset {
  provider: string;
  baseUrl: string;
  interface: string;
}

export interface LlmSettings {
  provider: string;
  baseUrl: string;
  interface: string;
  model: string;
  apiKey: string;
}

export interface SourceStatus {
  source: string;
  path: string;
  exists: boolean;
}

export interface DashboardStats {
  activeProjects: number;
  openTasks: number;
  sessions: number;
  unprocessedSessions: number;
  memories: number;
}

export interface JobRecord {
  id: number;
  kind: string;
  scope: string;
  sessionId: string | null;
  projectSlug: string | null;
  taskSlug: string | null;
  updatedAfter: string | null;
  status: string;
  total: number;
  completed: number;
  failed: number;
  pending: number;
  message: string;
  startedAt: string;
  updatedAt: string;
  completedAt: string | null;
}

export interface ProjectRecord {
  slug: string;
  displayTitle: string;
  workdir: string;
  sources: string[];
  infoPath: string | null;
  progressPath: string | null;
  reviewStatus: string;
  lastReviewedAt: string | null;
  lastSessionAt: string | null;
}

export interface TaskRecord {
  projectSlug: string;
  slug: string;
  title: string;
  brief: string;
  status: string;
  summaryPath: string;
  sessionCount: number;
  updatedAt: string;
}

export interface SessionRecord {
  source: string;
  sessionId: string;
  rawPath: string;
  projectSlug: string;
  taskSlug: string | null;
  title: string | null;
  summary: string | null;
  summaryPath: string | null;
  createdAt: string;
  updatedAt: string;
  status: string;
}

export interface AppState {
  dataDir: string;
  llmSettings: LlmSettings;
  providerPresets: ProviderPreset[];
  sourceStatuses: SourceStatus[];
  stats: DashboardStats;
  projects: ProjectRecord[];
  tasks: TaskRecord[];
  sessions: SessionRecord[];
  jobs: JobRecord[];
}

export interface ScanResult {
  found: number;
  inserted: number;
  codexFound: number;
  claudeFound: number;
}

export interface ImportResult {
  projectsUpdated: number;
  tasksCreated: number;
  sessionsWritten: number;
}

export interface EnqueueJobResult {
  jobId: number;
  total: number;
}

export interface CreateTaskResult {
  projectSlug: string;
  taskSlug: string;
  jobId: number;
  total: number;
  userPromptPath: string;
  llmPromptPath: string;
}
