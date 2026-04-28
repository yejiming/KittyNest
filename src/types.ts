export interface ProviderPreset {
  provider: string;
  baseUrl: string;
  interface: string;
}

export interface ProviderCallCount {
  provider: string;
  calls: number;
}

export interface LlmModelSettings {
  id: string;
  remark: string;
  provider: string;
  baseUrl: string;
  interface: string;
  model: string;
  apiKey: string;
  maxContext: number;
  maxTokens: number;
  temperature: number;
}

export interface LlmScenarioModels {
  defaultModel: string;
  projectModel: string;
  sessionModel: string;
  memoryModel: string;
  assistantModel: string;
}

export interface LlmSettings extends LlmModelSettings {
  models: LlmModelSettings[];
  scenarioModels: LlmScenarioModels;
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
  userPreferencePath: string | null;
  agentsPath: string | null;
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
  descriptionPath?: string | null;
  sessionPath?: string | null;
  sessionCount: number;
  createdAt: string;
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
  llmProviderCalls: ProviderCallCount[];
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

export interface MemorySearchResultRecord {
  sourceSession: string;
  sessionTitle: string;
  projectSlug: string;
  memory: string;
  ordinal: number;
}

export interface MemorySearchRecord {
  id: number;
  jobId: number;
  query: string;
  status: string;
  message: string;
  createdAt: string;
  updatedAt: string;
  results: MemorySearchResultRecord[];
}

export interface MemoryEntityRecord {
  entity: string;
  canonicalName?: string;
  entityType: string;
  sessionCount: number;
  createdAt: string;
}

export interface MemoryRelatedSession {
  sessionId: string;
  title: string;
  projectSlug: string;
  sharedEntities: string[];
}

export interface SessionMemoryDetail {
  sessionId: string;
  memoryPath: string;
  memories: string[];
  relatedSessions: MemoryRelatedSession[];
}
