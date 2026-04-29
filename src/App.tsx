import {
  Activity,
  Bot,
  BrainCircuit,
  CheckCircle2,
  CircleStop,
  CircleDot,
  Database,
  FolderKanban,
  Gauge,
  History,
  Loader2,
  Plus,
  RefreshCw,
  ScanLine,
  Send,
  Settings,
  ShieldCheck,
  Sparkles,
  TerminalSquare,
  Trash2,
} from "lucide-react";
import {
  Background,
  Controls,
  MarkerType,
  ReactFlow,
  type Edge,
  type Node,
  useNodesState,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useEffect, useMemo, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import kittyAvatar from "../src-tauri/assets/kittynest-cat-avatar.png";
import memoryPulseBrain from "../src-tauri/assets/memory-pulse-brain.png";
import tauriJobsCube from "../src-tauri/assets/tauri-jobs-cube.png";
import {
  enqueueAnalyzeSessions,
  enqueueAnalyzeSession,
  enqueueAnalyzeProject,
  enqueueRebuildMemories,
  enqueueScanSources,
  enqueueSearchMemories,
  getActiveJobs,
  getAppState,
  getCachedAppState,
  getMemorySearch,
  getSessionMemory,
  listEntitySessions,
  listMemoryEntities,
  readMarkdownFile,
  deleteTask,
  loadAgentSession,
  resetMemories,
  resetProjects,
  resetSessions,
  resetTasks,
  saveLlmSettings,
  updateTaskStatus,
  isTauriRuntime,
  stopJob,
} from "./api";
import { AgentDrawer } from "./AgentDrawer";
import type { AgentMessage, SavedAgentSession } from "./agentTypes";
import type {
  AppState,
  LlmModelSettings,
  LlmSettings,
  MemoryEntityRecord,
  MemoryRelatedSession,
  MemorySearchRecord,
  ProjectRecord,
  SessionMemoryDetail,
  SessionRecord,
  TaskRecord,
} from "./types";

type View =
  | "dashboard"
  | "projects"
  | "projectDetail"
  | "tasks"
  | "taskDetail"
  | "sessions"
  | "sessionDetail"
  | "memories"
  | "settings";

export default function App() {
  const [state, setState] = useState<AppState | null>(null);
  const [view, setView] = useState<View>("dashboard");
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [selectedTask, setSelectedTask] = useState<string | null>(null);
  const [selectedSession, setSelectedSession] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState("Tauri commands idle");
  const [loadError, setLoadError] = useState<string | null>(null);
  const [agentDrawerOpen, setAgentDrawerOpen] = useState(false);
  const [loadedAgentSession, setLoadedAgentSession] = useState<SavedAgentSession | null>(null);
  const [loadedAgentSessionSignal, setLoadedAgentSessionSignal] = useState(0);
  const [agentRefreshSignal, setAgentRefreshSignal] = useState(0);
  const tauriRuntime = isTauriRuntime();

  const refresh = async () => {
    setLoadError(null);
    const next = await getAppState();
    setState(next);
    setSelectedProject((current) => current ?? next.projects[0]?.slug ?? null);
  };

  const refreshCached = async () => {
    setLoadError(null);
    const next = await getCachedAppState();
    setState(next);
    setSelectedProject((current) => current ?? next.projects[0]?.slug ?? null);
  };

  useEffect(() => {
    void refresh().catch((error) => {
      const message = error instanceof Error ? error.message : String(error);
      setNotice(message);
      setLoadError(message);
    });
  }, []);

  useEffect(() => {
    if (!state?.jobs.length) return;
    const id = window.setInterval(() => {
      void refreshCached().catch((error) => {
        const message = error instanceof Error ? error.message : String(error);
        setNotice(message);
      });
    }, 2000);
    return () => window.clearInterval(id);
  }, [state?.jobs.length]);

  const currentProject = useMemo(
    () => state?.projects.find((project) => project.slug === selectedProject) ?? state?.projects[0],
    [selectedProject, state],
  );
  const projectTasks = useMemo(
    () => state?.tasks.filter((task) => task.projectSlug === currentProject?.slug) ?? [],
    [currentProject?.slug, state],
  );
  const currentTask = useMemo(
    () => projectTasks.find((task) => task.slug === selectedTask) ?? projectTasks[0],
    [projectTasks, selectedTask],
  );
  const currentSession = useMemo(
    () => state?.sessions.find((session) => session.sessionId === selectedSession) ?? state?.sessions[0],
    [selectedSession, state],
  );

  async function runAction(label: string, action: () => Promise<string>, refreshMode: "scan" | "cached" = "scan") {
    setBusy(label);
    setNotice(`${label} running`);
    try {
      const message = await action();
      setNotice(message);
      await (refreshMode === "cached" ? refreshCached() : refresh());
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(null);
    }
  }

  async function refreshActiveJobs() {
    const jobs = await getActiveJobs();
    setState((current) => (current ? { ...current, jobs } : current));
    return jobs;
  }

  async function runQueueAction(label: string, action: () => Promise<string>) {
    setBusy(label);
    setNotice(`${label} running`);
    try {
      const message = await action();
      setNotice(message);
      await refreshActiveJobs();
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(null);
    }
  }

  const queueSession = (sessionId: string) =>
    runQueueAction("Queue session analysis", async () => {
      const result = await enqueueAnalyzeSession(sessionId);
      return `Analysis queued: ${result.total} session${result.total === 1 ? "" : "s"}`;
    });

  const queueProjectAnalyze = (projectSlug: string) =>
    runQueueAction("Queue project analyze", async () => {
      const result = await enqueueAnalyzeProject(projectSlug);
      return `Project analysis queued: ${result.total} step${result.total === 1 ? "" : "s"}`;
    });

  const queueSessionsAnalyze = (updatedAfter?: string) =>
    runQueueAction("Queue sessions analysis", async () => {
      const result = await enqueueAnalyzeSessions(updatedAfter);
      return `Session analysis queued: ${result.total} session${result.total === 1 ? "" : "s"}`;
    });

  const queueSourceScan = () =>
    runQueueAction("Scan new sessions", async () => {
      const result = await enqueueScanSources();
      return `Scan queued: ${result.total} job${result.total === 1 ? "" : "s"}`;
    });

  const stopAnalyzeJob = (jobId: number) =>
    runQueueAction("Stop analysis job", async () => {
      const result = await stopJob(jobId);
      return result.stopped ? "Analysis job stopped" : "Analysis job was already finished";
    });

  if (!state) {
    return (
      <main className="boot">
        {loadError ? (
          <section className="boot-error">
            <strong>Local ledger failed to load</strong>
            <span>{loadError}</span>
            <button onClick={() => void refresh().catch((error) => {
              const message = error instanceof Error ? error.message : String(error);
              setNotice(message);
              setLoadError(message);
            })}>
              Retry
            </button>
          </section>
        ) : (
          <>
            <Loader2 className="spin" size={28} />
            <span>Loading local ledger</span>
          </>
        )}
      </main>
    );
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="window-dots" aria-hidden="true">
          <span />
          <span />
          <span />
        </div>
        <button className="brand" aria-label="Dashboard home" onClick={() => setView("dashboard")}>
          <img className="brand-avatar" src={kittyAvatar} alt="KittyNest cat avatar" />
          <span>
            <strong>KittyNest</strong>
            <small>Local Mode</small>
          </span>
        </button>
        <NavButton active={view === "dashboard"} icon={<Gauge size={19} />} label="Dashboard" onClick={() => setView("dashboard")} />
        <NavButton active={view === "projects" || view === "projectDetail"} icon={<FolderKanban size={19} />} label="Projects" onClick={() => setView("projects")} />
        <NavButton active={view === "sessions" || view === "sessionDetail"} icon={<History size={19} />} label="Sessions" onClick={() => setView("sessions")} />
        <NavButton active={view === "tasks" || view === "taskDetail"} icon={<CircleDot size={19} />} label="Tasks" onClick={() => setView("tasks")} />
        <NavButton active={view === "memories"} icon={<BrainCircuit size={19} />} label="Memory" onClick={() => setView("memories")} />
        <NavButton active={view === "settings"} icon={<Settings size={19} />} label="Settings" onClick={() => setView("settings")} />
        <button className="assistant-launch" aria-label="Assistant" onClick={() => setAgentDrawerOpen(true)}>
          <Bot size={19} />
          <span>Assistant</span>
        </button>
      </aside>

      <main className="workspace">
        <header className="titlebar">
          <div>
            <small>KittyNest</small>
            <h1>{titleFor(view, currentProject, currentTask)}</h1>
          </div>
          <div className="top-actions">
            <StatusPill tone="green" icon={<ShieldCheck size={17} />} label="Local Mode" />
            <StatusPill
              tone={state.llmSettings.model ? "green" : "amber"}
              icon={<Sparkles size={17} />}
              label={state.llmSettings.model ? "LLM Ready" : "LLM Draft"}
            />
          </div>
        </header>

        {!tauriRuntime && (
          <section className="runtime-banner">
            Browser preview only. Local Claude Code and Codex session scanning requires the Tauri
            desktop runtime.
          </section>
        )}

        {view === "dashboard" && (
          <Dashboard
            state={state}
            busy={busy}
            onScan={queueSourceScan}
            onOpenProjects={() => setView("projects")}
            onOpenTasks={() => setView("tasks")}
            onOpenSessions={() => setView("sessions")}
            onCreateProject={() => setNotice("Create Project is reserved for a later milestone")}
            onCreateTask={() => setView("tasks")}
            onOpenProject={(slug) => {
              setSelectedProject(slug);
              setView("projectDetail");
            }}
            onOpenSession={(sessionId) => {
              setSelectedSession(sessionId);
              setView("sessionDetail");
            }}
            onRefreshMemories={() => runQueueAction("Refresh memories", async () => {
              const result = await enqueueRebuildMemories();
              return `Memory refresh queued: ${result.total} step${result.total === 1 ? "" : "s"}`;
            })}
            onStopJob={stopAnalyzeJob}
          />
        )}

        {view === "projects" && (
          <ProjectsList
            projects={state.projects}
            onOpen={(slug) => {
              setSelectedProject(slug);
              setView("projectDetail");
            }}
          />
        )}

        {view === "projectDetail" && currentProject && (
          <ProjectView
            project={currentProject}
            tasks={projectTasks}
            busy={busy}
            onAnalyze={() => queueProjectAnalyze(currentProject.slug)}
            onOpenTask={(taskSlug) => {
              setSelectedTask(taskSlug);
              setView("taskDetail");
            }}
          />
        )}

        {view === "tasks" && (
          <TasksList
            tasks={state.tasks}
            onOpen={(projectSlug, taskSlug) => {
              setSelectedProject(projectSlug);
              setSelectedTask(taskSlug);
              setView("taskDetail");
            }}
          />
        )}

        {view === "taskDetail" && currentTask && (
          <TaskView
            task={currentTask}
            sessions={state.sessions.filter((session) => session.projectSlug === currentTask.projectSlug && session.taskSlug === currentTask.slug)}
            onStatus={(status) =>
              runAction("Update task status", async () => {
                await updateTaskStatus(currentTask.projectSlug, currentTask.slug, status);
                return `Task status updated to ${status}`;
              })
            }
            onDelete={() =>
              runAction("Delete task", async () => {
                await deleteTask(currentTask.projectSlug, currentTask.slug);
                setView("tasks");
                return "Task deleted";
              })
            }
            onLoad={async (loadedSession) => {
              try {
                const next = loadedSession ?? await loadAgentSession(currentTask.projectSlug, currentTask.slug);
                setLoadedAgentSession(next);
                setLoadedAgentSessionSignal((current) => current + 1);
                setAgentDrawerOpen(true);
                setNotice("Task session loaded");
              } catch (error) {
                setNotice(error instanceof Error ? error.message : String(error));
              }
            }}
            onOpenSession={(sessionId) => {
              setSelectedSession(sessionId);
              setView("sessionDetail");
            }}
          />
        )}

        {view === "sessions" && (
          <SessionsList
            projects={state.projects}
            sessions={state.sessions}
            busy={busy}
            onAnalyze={queueSessionsAnalyze}
            onOpen={(sessionId) => {
              setSelectedSession(sessionId);
              setView("sessionDetail");
            }}
          />
        )}

        {view === "sessionDetail" && currentSession && (
          <SessionView
            session={currentSession}
            busy={busy}
            onAnalyze={() => queueSession(currentSession.sessionId)}
            onOpenSession={(sessionId) => {
              setSelectedSession(sessionId);
              setView("sessionDetail");
            }}
          />
        )}
        {view === "memories" && (
          <MemoryView
            jobs={state.jobs}
            sessions={state.sessions}
            onOpenSession={(sessionId) => {
              setSelectedSession(sessionId);
              setView("sessionDetail");
            }}
            onSearch={(query) => runQueueAction("Search memories", async () => {
              const result = await enqueueSearchMemories(query);
              return `Memory search queued: ${result.total} job${result.total === 1 ? "" : "s"}`;
            })}
          />
        )}
        {view === "settings" && <SettingsView state={state} onSave={(settings) => runAction("Save settings", async () => {
          const assistantModelChanged =
            (state.llmSettings.scenarioModels?.assistantModel ?? "") !== (settings.scenarioModels?.assistantModel ?? "");
          await saveLlmSettings(settings);
          if (assistantModelChanged) setAgentRefreshSignal((current) => current + 1);
          return "LLM settings saved";
        }, "cached")}
          busy={busy}
          onResetSessions={() => runAction("Reset sessions", async () => {
            const result = await resetSessions();
            return `Sessions reset: ${result.reset}`;
          }, "cached")}
          onResetProjects={() => runAction("Reset projects", async () => {
            const result = await resetProjects();
            return `Projects reset: ${result.reset}`;
          }, "cached")}
          onResetTasks={() => runAction("Reset tasks", async () => {
            const result = await resetTasks();
            return `Tasks reset: ${result.reset}`;
          }, "cached")}
          onResetMemories={() => runAction("Reset memories", async () => {
            const result = await resetMemories();
            return `Memories reset: ${result.reset}`;
          }, "cached")}
        />}
      </main>

      <footer className="statusbar">
        <span><TerminalSquare size={16} /> Tauri v2</span>
        <span><Database size={16} /> SQLite synced</span>
        <span><Activity size={16} /> {notice}</span>
      </footer>
      <AgentDrawer
        open={agentDrawerOpen}
        projects={state.projects}
        loadedSession={loadedAgentSession}
        loadSignal={loadedAgentSessionSignal}
        refreshSignal={agentRefreshSignal}
        onClose={() => setAgentDrawerOpen(false)}
        onRunComplete={() => void refreshCached()}
        onSaved={() => void refreshCached()}
      />
    </div>
  );
}

function Dashboard({
  state,
  busy,
  onScan,
  onOpenProjects,
  onOpenTasks,
  onOpenSessions,
  onCreateProject,
  onCreateTask,
  onOpenProject,
  onOpenSession,
  onRefreshMemories,
  onStopJob,
}: {
  state: AppState;
  busy: string | null;
  onScan: () => void;
  onOpenProjects: () => void;
  onOpenTasks: () => void;
  onOpenSessions: () => void;
  onCreateProject: () => void;
  onCreateTask: () => void;
  onOpenProject: (slug: string) => void;
  onOpenSession: (sessionId: string) => void;
  onRefreshMemories: () => void;
  onStopJob: (jobId: number) => void;
}) {
  const recentSessions = [...state.sessions].sort(
    (a, b) => Date.parse(b.updatedAt || b.createdAt) - Date.parse(a.updatedAt || a.createdAt),
  );
  return (
    <section className="dashboard-grid">
      <div className="metrics-strip">
        <Metric tone="projects" icon={<FolderKanban />} label="Active Projects" value={state.stats.activeProjects} detail={`${state.projects.filter((p) => p.reviewStatus !== "reviewed").length} need review`} />
        <Metric tone="sessions" icon={<History />} label="Sessions" value={state.stats.sessions} detail={`${state.stats.unprocessedSessions} new`} />
        <Metric tone="tasks" icon={<CheckCircle2 />} label="Open Tasks" value={state.stats.openTasks} detail={`${state.tasks.filter((task) => task.status === "developing").length} in development`} />
        <Metric tone="memory" icon={<BrainCircuit />} label="Memory Updates" value={state.stats.memories} detail="local memory" />
      </div>

      <section className="panel projects-panel">
        <PanelTitle title="Projects" action={<IconButton label="Scan" icon={<ScanLine size={16} />} onClick={onScan} busy={busy === "Scan new sessions"} />} />
        <div className="project-list panel-scroll five-rows">
          {state.projects.length === 0 && <EmptyLine text="No projects yet. Scan sources to discover local sessions." />}
          {state.projects.map((project) => (
            <button key={project.slug} className="project-row" onClick={() => onOpenProject(project.slug)}>
              <span className="hex">{project.slug.slice(0, 1)}</span>
              <span>
                <strong>{truncateDashboardLabel(project.displayTitle)}</strong>
              </span>
              <em>{project.reviewStatus.replace("_", " ")}</em>
            </button>
          ))}
        </div>
        <div className="panel-footer">
          <IconButton label="+ Create Project" icon={<FolderKanban size={16} />} onClick={onCreateProject} />
        </div>
      </section>

      <section className="panel sessions-panel">
        <PanelTitle title="Recent Sessions" action={<IconButton label="View All" icon={<History size={16} />} onClick={onOpenSessions} />} />
        <div className="session-table panel-scroll five-rows">
          <div className="session-row session-row-head">
            <span>Session</span>
            <small>Project</small>
            <small>Source</small>
            <small>Updated</small>
          </div>
          {recentSessions.map((session) => (
            <button key={session.sessionId} className="session-row" onClick={() => onOpenSession(session.sessionId)}>
              <span>{truncateDashboardLabel(session.title ?? session.sessionId)}</span>
              <small>{session.projectSlug}</small>
              <small>{session.source}</small>
              <small>{compactAgeLabel(session.updatedAt || session.createdAt)}</small>
            </button>
          ))}
          {state.sessions.length === 0 && <EmptyLine text="No analyzed sessions yet." />}
        </div>
        <div className="panel-footer">
          <IconButton label="Scan New Sessions" icon={<ScanLine size={16} />} onClick={onScan} busy={busy === "Scan new sessions"} />
        </div>
      </section>

      <section className="panel status-panel">
        <PanelTitle title="Task Status" action={<IconButton label="View All" icon={<CircleDot size={16} />} onClick={onOpenTasks} />} />
        <div className="task-status-body">
          <TaskDonut tasks={state.tasks} />
          <div className="status-counts">
            <StatusCount label="Discussing" value={state.tasks.filter((task) => task.status === "discussing").length} />
            <StatusCount label="Developing" value={state.tasks.filter((task) => task.status === "developing").length} />
            <StatusCount label="Done" value={state.tasks.filter((task) => task.status === "done").length} />
          </div>
        </div>
        <div className="panel-footer">
          <IconButton label="Create Task" icon={<CircleDot size={16} />} onClick={onCreateTask} />
        </div>
      </section>

      <section className="panel pulse-panel">
        <PanelTitle title="Memory Pulse" action={<IconButton label="Refresh" icon={<RefreshCw size={16} />} onClick={onRefreshMemories} busy={busy === "Refresh memories"} />} />
        <div className="pulse">
          <img className="pulse-art" src={memoryPulseBrain} alt="" />
          <div className="pulse-copy">
            <strong>Project memory draft</strong>
            <span>{state.stats.memories} memory files indexed</span>
          </div>
        </div>
      </section>

      <section className="panel jobs-panel">
        <PanelTitle title="Analyze Jobs" action={<StatusPill tone="cyan" icon={<Activity size={16} />} label={busy ? "Running" : "Idle"} />} />
        <div className="jobs-layout">
          <JobsTable jobs={state.jobs} onStop={onStopJob} />
          <img className="jobs-art" src={tauriJobsCube} alt="" />
        </div>
      </section>
    </section>
  );
}

function ProjectsList({ projects, onOpen }: { projects: ProjectRecord[]; onOpen: (slug: string) => void }) {
  return (
    <section className="detail-layout">
      <div className="panel wide">
        <PanelTitle title="Projects" />
        <div className="list-page data-table projects-table" role="table">
          <div className="table-header" role="row">
            <span role="columnheader">Name</span>
            <span role="columnheader">Path</span>
            <span role="columnheader">Status</span>
            <span role="columnheader">Source</span>
          </div>
          {projects.map((project) => (
            <button key={project.slug} className="list-row" onClick={() => onOpen(project.slug)}>
              <strong>{project.displayTitle}</strong>
              <span>{project.workdir}</span>
              <small>{project.reviewStatus.replace("_", " ")}</small>
              <small>{project.sources.join(" / ") || "local"}</small>
            </button>
          ))}
          {projects.length === 0 && <EmptyLine text="No projects yet. Scan sources to discover local sessions." />}
        </div>
      </div>
    </section>
  );
}

function TasksList({
  tasks,
  onOpen,
}: {
  tasks: TaskRecord[];
  onOpen: (projectSlug: string, taskSlug: string) => void;
}) {
  return (
    <section className="detail-layout">
      <div className="panel wide">
        <PanelTitle title="Tasks" />
        <div className="list-page data-table tasks-table" role="table">
          <div className="table-header" role="row">
            <span role="columnheader">Name</span>
            <span role="columnheader">Project</span>
            <span role="columnheader">Status</span>
            <span role="columnheader">Created</span>
          </div>
          {tasks.map((task) => (
            <button key={`${task.projectSlug}-${task.slug}`} className="list-row" onClick={() => onOpen(task.projectSlug, task.slug)}>
              <strong>{task.title}</strong>
              <span>{task.projectSlug}</span>
              <small>{task.status}</small>
              <small>{task.createdAt ? task.createdAt.slice(0, 10) : compactAgeLabel(task.updatedAt)}</small>
            </button>
          ))}
          {tasks.length === 0 && <EmptyLine text="No tasks yet." />}
        </div>
      </div>
    </section>
  );
}

function SessionsList({
  projects,
  sessions,
  busy,
  onAnalyze,
  onOpen,
}: {
  projects: ProjectRecord[];
  sessions: SessionRecord[];
  busy: string | null;
  onAnalyze: (updatedAfter?: string) => void;
  onOpen: (sessionId: string) => void;
}) {
  const [expanded, setExpanded] = useState<string | null>(null);
  const [analyzeRange, setAnalyzeRange] = useState<AnalyzeRange>("7days");
  const sessionsByProject = (projectSlug: string) =>
    sessions.filter((session) => session.projectSlug === projectSlug && session.status === "analyzed");
  return (
    <section className="detail-layout">
      <div className="panel wide">
        <PanelTitle
          title="Sessions"
          action={
            <div className="panel-actions">
              <label className="inline-select">
                <span>Analyze range</span>
                <select
                  aria-label="Analyze range"
                  value={analyzeRange}
                  onChange={(event) => setAnalyzeRange(event.target.value as AnalyzeRange)}
                >
                  <option value="3days">3days</option>
                  <option value="7days">7days</option>
                  <option value="30days">30days</option>
                  <option value="All">All</option>
                </select>
              </label>
              <IconButton
                label="Analyze"
                icon={<Sparkles size={16} />}
                onClick={() => onAnalyze(updatedAfterForAnalyzeRange(analyzeRange))}
                busy={busy === "Queue sessions analysis"}
              />
            </div>
          }
        />
        <div className="list-page data-table projects-table" role="table">
          <div className="table-header" role="row">
            <span role="columnheader">Name</span>
            <span role="columnheader">Path</span>
            <span role="columnheader">Status</span>
            <span role="columnheader">Source</span>
          </div>
          {projects.map((project) => (
            <div key={project.slug} className="memory-project-row">
              <button
                className="list-row"
                onClick={() => setExpanded(expanded === project.slug ? null : project.slug)}
              >
                <strong>{project.displayTitle}</strong>
                <span>{project.workdir}</span>
                <small>{project.reviewStatus.replace("_", " ")}</small>
                <small>{project.sources.join(" / ") || "local"}</small>
              </button>
              {expanded === project.slug && (
                <div className="nested-sessions sessions-table">
                  {sessionsByProject(project.slug).map((session) => (
                    <button key={session.sessionId} className="list-row" onClick={() => onOpen(session.sessionId)}>
                      <strong>{session.title ?? session.sessionId}</strong>
                      <span>{session.rawPath}</span>
                      <span>{session.projectSlug}</span>
                      <span>{session.taskSlug ?? "unassigned"}</span>
                      <small>{session.source}</small>
                      <small>{session.status}</small>
                      <small>{compactAgeLabel(session.updatedAt || session.createdAt)}</small>
                    </button>
                  ))}
                  {sessionsByProject(project.slug).length === 0 && <EmptyLine text="No memories yet." />}
                </div>
              )}
            </div>
          ))}
          {projects.length === 0 && <EmptyLine text="No projects yet. Scan sources to discover local sessions." />}
        </div>
      </div>
    </section>
  );
}

function ProjectView({
  project,
  tasks,
  busy,
  onAnalyze,
  onOpenTask,
}: {
  project: ProjectRecord;
  tasks: TaskRecord[];
  busy: string | null;
  onAnalyze: () => void;
  onOpenTask: (taskSlug: string) => void;
}) {
  return (
    <section className="detail-layout">
      <div className="panel wide">
        <h2>{project.displayTitle}</h2>
        <div className="project-paths">
          <ProjectPath label="Project Path" value={project.workdir} />
          <ProjectPath label="Project Summary Path" value={project.infoPath ?? "Not generated yet."} />
          <ProjectPath label="Project Progress Path" value={project.progressPath ?? "Not generated yet."} />
          <ProjectPath label="User Preference Path" value={project.userPreferencePath ?? "Not generated yet."} />
          <ProjectPath label="AGENTS.md Path" value={project.agentsPath ?? "Not generated yet."} />
        </div>
        <div className="button-row">
          <IconButton label="Analyze" icon={<Sparkles size={16} />} onClick={onAnalyze} busy={busy === "Queue project analyze"} />
        </div>
      </div>
      <div className="markdown-stack">
        <MarkdownPanel
          title="Project Summary"
          path={project.infoPath}
          empty="Review has not been generated yet."
        />
        <MarkdownPanel
          title="Progress"
          path={project.progressPath}
          empty="Import historical sessions to generate progress."
        />
        <MarkdownPanel
          title="User Preference"
          path={project.userPreferencePath}
          empty="Analyze the project to generate user preferences."
        />
        <MarkdownPanel
          title="AGENTS.md"
          path={project.agentsPath}
          empty="Analyze the project to generate AGENTS.md."
        />
      </div>
      <div className="task-grid">
        {tasks.map((task) => (
          <button key={task.slug} className="task-card" onClick={() => onOpenTask(task.slug)}>
            <strong>{task.title}</strong>
            <span>{task.status}</span>
            <small>{task.sessionCount} sessions</small>
          </button>
        ))}
      </div>
    </section>
  );
}

function ProjectPath({ label, value }: { label: string; value: string }) {
  return (
    <div className="project-path">
      <span className="project-path-label">{label}</span>
      <span className="project-path-value">{value}</span>
    </div>
  );
}

function MarkdownPanel({
  title,
  path,
  empty,
}: {
  title: string;
  path: string | null;
  empty: string;
}) {
  const [content, setContent] = useState("");
  const [error, setError] = useState("");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    setContent("");
    setError("");
    setCopied(false);
    if (!path) return;
    void readMarkdownFile(path)
      .then((result) => setContent(result.content))
      .catch((error) => setError(error instanceof Error ? error.message : String(error)));
  }, [path]);

  const copyMarkdown = () => {
    if (!content || !navigator.clipboard) return;
    void navigator.clipboard.writeText(content).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    });
  };

  return (
    <div className="panel markdown-panel">
      <div className="markdown-panel-head">
        <h3>{title}</h3>
        <button
          type="button"
          className="markdown-copy-button"
          aria-label={`Copy ${title} markdown`}
          disabled={!content}
          onClick={copyMarkdown}
        >
          {copied ? "OK" : "COPY"}
        </button>
      </div>
      <div className="markdown-scroll">
        {content ? <MarkdownContent content={content} /> : <p>{error || empty}</p>}
      </div>
    </div>
  );
}

function MarkdownContent({ content }: { content: string }) {
  return (
    <div className="markdown-body">
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{stripFrontmatter(content)}</ReactMarkdown>
    </div>
  );
}

function stripFrontmatter(content: string) {
  return content.replace(/^---[\s\S]*?---\s*/, "").trim();
}

function truncateDashboardLabel(value: string) {
  const characters = Array.from(value);
  return characters.length > 10 ? `${characters.slice(0, 10).join("")}...` : value;
}

function TaskView({
  task,
  sessions,
  onStatus,
  onDelete,
  onLoad,
  onOpenSession,
}: {
  task: TaskRecord;
  sessions: SessionRecord[];
  onStatus: (status: string) => void;
  onDelete: () => void;
  onLoad: (loadedSession?: SavedAgentSession | null) => void;
  onOpenSession: (sessionId: string) => void;
}) {
  const orderedSessions = [...sessions].sort((left, right) => left.updatedAt.localeCompare(right.updatedAt));
  const taskDir = taskDirectoryPath(task);
  const [savedSession, setSavedSession] = useState<SavedAgentSession | null>(null);
  const [savedSessionError, setSavedSessionError] = useState("");

  useEffect(() => {
    setSavedSession(null);
    setSavedSessionError("");
    if (!task.sessionPath) return;
    let disposed = false;
    void loadAgentSession(task.projectSlug, task.slug)
      .then((session) => {
        if (!disposed) setSavedSession(session);
      })
      .catch((error) => {
        if (!disposed) setSavedSessionError(error instanceof Error ? error.message : String(error));
      });
    return () => {
      disposed = true;
    };
  }, [task.projectSlug, task.slug, task.sessionPath]);

  if (task.sessionPath) {
    return (
      <section className="detail-layout">
        <div className="panel wide">
          <PanelTitle
            title="Task Info"
            action={(
              <div className="panel-actions">
                <IconButton label="Delete" icon={<Trash2 size={16} />} onClick={onDelete} />
                <IconButton label="Load" icon={<Bot size={16} />} onClick={() => onLoad(savedSession)} />
              </div>
            )}
          />
          <div className="task-meta">
            <span>Name</span>
            <strong>{task.title}</strong>
            <span>Project</span>
            <strong>{task.projectSlug}</strong>
            <span>Status</span>
            <strong>{task.status}</strong>
            <span>Created</span>
            <strong>{task.createdAt}</strong>
          </div>
          <div className="segmented">
            {["discussing", "developing", "done"].map((status) => (
              <button
                key={status}
                className={task.status === status ? "active" : ""}
                onClick={() => onStatus(status)}
              >
                {status}
              </button>
            ))}
          </div>
        </div>
        <div className="saved-task-description">
          <MarkdownPanel
            title="Task Description"
            path={task.descriptionPath ?? task.summaryPath}
            empty="Task description has not been written yet."
          />
        </div>
        <SavedTaskConversation messages={savedSession?.messages ?? []} error={savedSessionError} />
      </section>
    );
  }

  return (
    <section className="detail-layout">
      <div className="panel wide">
        <PanelTitle
          title="Task Info"
          action={<IconButton label="Delete" icon={<Trash2 size={16} />} onClick={onDelete} disabled={task.sessionCount > 0} />}
        />
        <div className="task-meta">
          <span>Path</span>
          <strong>{taskDirectoryPath(task)}</strong>
          <span>Sessions</span>
          <strong>{task.sessionCount}</strong>
        </div>
        <div className="segmented">
          {["discussing", "developing", "done"].map((status) => (
            <button
              key={status}
              className={task.status === status ? "active" : ""}
              disabled={task.sessionCount === 0 && status !== "discussing"}
              onClick={() => onStatus(status)}
            >
              {status}
            </button>
          ))}
        </div>
      </div>
      <div className="markdown-stack">
        <MarkdownPanel
          title="User Prompt"
          path={`${taskDir}/user_prompt.md`}
          empty="User prompt has not been written yet."
        />
        <MarkdownPanel
          title="LLM Prompt"
          path={`${taskDir}/llm_prompt.md`}
          empty="LLM prompt has not been written yet."
        />
      </div>
      <div className="task-summary-stack">
        {orderedSessions.map((session) => (
          <button
            key={session.sessionId}
            className="panel task-summary-card"
            aria-label={`Open session ${session.title ?? session.sessionId}`}
            onClick={() => onOpenSession(session.sessionId)}
          >
            <div className="task-summary-card-head">
              <strong>{session.title ?? session.sessionId}</strong>
              <small>{session.updatedAt}</small>
            </div>
            <span>{session.sessionId}</span>
            <MarkdownContent content={session.summary ?? "No session summary yet."} />
          </button>
        ))}
        {orderedSessions.length === 0 && <div className="panel wide"><EmptyLine text="No session summaries yet." /></div>}
      </div>
    </section>
  );
}

function SavedTaskConversation({ messages, error }: { messages: AgentMessage[]; error: string }) {
  const conversation = messages.filter((message) => message.role === "user" || message.role === "assistant");
  return (
    <div className="panel wide saved-task-conversation">
      <PanelTitle title="Conversation" />
      <div className="task-conversation-messages">
        {conversation.map((message) => (
          <article key={message.id} className={`agent-message ${message.role}`}>
            {message.role === "assistant"
              ? <MarkdownContent content={message.content ?? ""} />
              : message.content}
          </article>
        ))}
        {conversation.length === 0 && <EmptyLine text={error || "No saved conversation messages yet."} />}
      </div>
    </div>
  );
}

function taskDirectoryPath(task: TaskRecord) {
  for (const filename of ["/summary.md", "/user_prompt.md", "/llm_prompt.md"]) {
    if (task.summaryPath.endsWith(filename)) {
      return task.summaryPath.slice(0, -filename.length);
    }
  }
  return task.summaryPath;
}

function SessionView({
  session,
  busy,
  onAnalyze,
  onOpenSession,
}: {
  session: SessionRecord;
  busy: string | null;
  onAnalyze: () => void;
  onOpenSession: (sessionId: string) => void;
}) {
  const [memory, setMemory] = useState<SessionMemoryDetail | null>(null);
  const [memoryError, setMemoryError] = useState("");

  useEffect(() => {
    setMemory(null);
    setMemoryError("");
    void getSessionMemory(session.sessionId)
      .then(setMemory)
      .catch((error) => setMemoryError(error instanceof Error ? error.message : String(error)));
  }, [session.sessionId]);

  return (
    <section className="detail-stack">
      <div className="panel wide">
        <PanelTitle title={session.title ?? session.sessionId} action={<IconButton label="Analyze" icon={<Sparkles size={16} />} onClick={onAnalyze} busy={busy === "Queue session analysis"} />} />
        <div className="session-paths">
          <ProjectPath label="Original Path" value={session.rawPath} />
          <ProjectPath label="System Path" value={session.summaryPath ?? "Not generated yet."} />
          <ProjectPath label="Memory Path" value={memory?.memoryPath ?? "Not generated yet."} />
        </div>
      </div>
      <MarkdownPanel title="Summary" path={session.summaryPath} empty="Session summary has not been written yet." />
      <SessionMemoryPanel
        detail={memory}
        error={memoryError}
        currentSessionId={session.sessionId}
        currentSessionTitle={session.title ?? session.sessionId}
        onOpenSession={onOpenSession}
      />
    </section>
  );
}

function SessionMemoryPanel({
  detail,
  error,
  currentSessionId,
  currentSessionTitle,
  onOpenSession,
}: {
  detail: SessionMemoryDetail | null;
  error: string;
  currentSessionId: string;
  currentSessionTitle: string;
  onOpenSession: (sessionId: string) => void;
}) {
  return (
    <div className="panel memory-panel">
      <h3>Memory</h3>
      {error && <p className="empty-line">{error}</p>}
      {detail ? (
        <>
          <MemoryGraph
            detail={detail}
            currentSessionId={currentSessionId}
            currentSessionTitle={currentSessionTitle}
            onOpenSession={onOpenSession}
          />
          <div className="memory-line-grid">
            {detail.memories.map((line, index) => (
              <article className="memory-line-card" key={`${index}-${line}`}>
                <MarkdownContent content={line} />
              </article>
            ))}
            {detail.memories.length === 0 && <EmptyLine text="No memory lines yet." />}
          </div>
        </>
      ) : (
        <p className="empty-line">Loading memory.</p>
      )}
    </div>
  );
}

function MemoryGraph({
  detail,
  currentSessionId,
  currentSessionTitle,
  onOpenSession,
}: {
  detail: SessionMemoryDetail;
  currentSessionId: string;
  currentSessionTitle: string;
  onOpenSession: (sessionId: string) => void;
}) {
  const graph = useMemo(() => {
    const related = detail.relatedSessions.slice(0, 8);
    const entities = Array.from(new Set(related.flatMap((session) => session.sharedEntities))).slice(0, 8);
    const spread = (index: number, total: number, min: number, max: number) =>
      total <= 1 ? (min + max) / 2 : min + (index * (max - min)) / (total - 1);
    const entityWidth = Math.max(360, (entities.length - 1) * 190);
    const relatedWidth = Math.max(360, (related.length - 1) * 190);
    const graphWidth = Math.max(entityWidth, relatedWidth);
    const centerX = graphWidth / 2;

    const nodes: Node[] = [
      {
        id: `session:${currentSessionId}`,
        type: "input",
        position: { x: centerX - 92, y: 16 },
        data: {
          label: <MemoryFlowNode kind="Session" label={currentSessionTitle} active />,
          nodeKind: "session",
          sessionId: currentSessionId,
        },
        className: "memory-flow-node active",
      },
      ...entities.map((entity, index) => ({
        id: `entity:${entity}`,
        position: { x: spread(index, entities.length, centerX - entityWidth / 2, centerX + entityWidth / 2) - 92, y: 188 },
        data: { label: <MemoryFlowNode kind="Entity" label={entity} />, nodeKind: "entity" },
        className: "memory-flow-node entity",
      })),
      ...related.map((session, index) => ({
        id: `session:${session.sessionId}`,
        type: "output",
        position: { x: spread(index, related.length, centerX - relatedWidth / 2, centerX + relatedWidth / 2) - 92, y: 358 },
        data: {
          label: <MemoryFlowNode kind="Session" label={session.title} />,
          nodeKind: "session",
          sessionId: session.sessionId,
        },
        className: "memory-flow-node",
      })),
    ];
    const baseEdge = {
      markerEnd: { type: MarkerType.ArrowClosed },
      className: "memory-flow-edge",
    };
    const edges: Edge[] = [
      ...entities.map((entity) => ({
        ...baseEdge,
        id: `edge:${currentSessionId}:${entity}`,
        source: `session:${currentSessionId}`,
        target: `entity:${entity}`,
      })),
      ...related.flatMap((session) =>
        session.sharedEntities.map((entity) => ({
          ...baseEdge,
          id: `edge:${entity}:${session.sessionId}`,
          source: `entity:${entity}`,
          target: `session:${session.sessionId}`,
        })),
      ),
    ];
    return { nodes, edges };
  }, [currentSessionId, currentSessionTitle, detail.relatedSessions]);
  const [nodes, setNodes, onNodesChange] = useNodesState(graph.nodes);

  useEffect(() => {
    setNodes(graph.nodes);
  }, [graph.nodes, setNodes]);

  return (
    <div className="memory-graph" aria-label="Related memory graph" data-edge-count={graph.edges.length}>
      <ReactFlow
        nodes={nodes}
        edges={graph.edges}
        onNodesChange={onNodesChange}
        onNodeClick={(_, node) => {
          if (node.data.nodeKind !== "session" || typeof node.data.sessionId !== "string") return;
          onOpenSession(node.data.sessionId);
        }}
        fitView
        fitViewOptions={{ padding: 0.2 }}
        minZoom={0.35}
        maxZoom={1.8}
        nodesDraggable
        nodesConnectable={false}
        proOptions={{ hideAttribution: true }}
      >
        <Background gap={32} color="rgba(116, 255, 241, 0.18)" />
        <Controls position="bottom-right" showInteractive={false} />
      </ReactFlow>
    </div>
  );
}

function MemoryFlowNode({ kind, label, active = false }: { kind: string; label: string; active?: boolean }) {
  return (
    <div className={`memory-flow-node-inner ${active ? "active" : ""}`}>
      <span>{kind}</span>
      <strong>{label}</strong>
    </div>
  );
}

function MemoryView({
  jobs,
  sessions,
  onOpenSession,
  onSearch,
}: {
  jobs: AppState["jobs"];
  sessions: SessionRecord[];
  onOpenSession: (sessionId: string) => void;
  onSearch: (query: string) => Promise<void>;
}) {
  const [query, setQuery] = useState("");
  const [latestSearch, setLatestSearch] = useState<MemorySearchRecord | null>(null);
  const [entities, setEntities] = useState<MemoryEntityRecord[]>([]);
  const [expandedEntity, setExpandedEntity] = useState<string | null>(null);
  const [entitySessions, setEntitySessions] = useState<Record<string, MemoryRelatedSession[]>>({});
  const [loadingEntity, setLoadingEntity] = useState<string | null>(null);
  const [error, setError] = useState("");
  const hasSubmittedSearch = useRef(false);

  const refreshEntities = () => {
    setError("");
    void listMemoryEntities()
      .then(setEntities)
      .catch((error) => setError(error instanceof Error ? error.message : String(error)));
  };

  const refreshLatestSearch = () => {
    setError("");
    void getMemorySearch()
      .then(setLatestSearch)
      .catch((error) => setError(error instanceof Error ? error.message : String(error)));
  };

  useEffect(() => {
    setLatestSearch(null);
    refreshEntities();
  }, []);

  useEffect(() => {
    if (!hasSubmittedSearch.current) return;
    refreshEntities();
    refreshLatestSearch();
  }, [jobs]);

  const submitSearch = () => {
    const trimmed = query.trim();
    if (!trimmed) return;
    setQuery("");
    hasSubmittedSearch.current = true;
    void onSearch(trimmed).then(() => {
      refreshEntities();
      refreshLatestSearch();
    });
  };

  const toggleEntity = (entity: string) => {
    setExpandedEntity((current) => (current === entity ? null : entity));
    if (entitySessions[entity]) return;
    setLoadingEntity(entity);
    void listEntitySessions(entity)
      .then((sessions) => setEntitySessions((current) => ({ ...current, [entity]: sessions })))
      .catch((error) => setError(error instanceof Error ? error.message : String(error)))
      .finally(() => setLoadingEntity((current) => (current === entity ? null : current)));
  };

  return (
    <section className="detail-stack">
      <div className="panel memory-search-panel">
        <PanelTitle title="Memory Search" />
        <div className="memory-search-row">
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") submitSearch();
            }}
            placeholder="Search memory by topic or entity"
          />
          <button
            aria-label="Send memory search"
            className="icon-button icon-only"
            disabled={!query.trim()}
            onClick={submitSearch}
          >
            <Send size={16} />
          </button>
        </div>
        {error && <p className="empty-line">{error}</p>}
        {latestSearch && (
          <div className="memory-search-meta">
            <span>{latestSearch.query}</span>
            <small>{latestSearch.status}</small>
            <small>{latestSearch.message}</small>
          </div>
        )}
        <div className="memory-results">
          {latestSearch?.results.map((result) => (
            <button
              key={`${result.sourceSession}-${result.ordinal}`}
              className="memory-result-card"
              onClick={() => onOpenSession(result.sourceSession)}
            >
              <strong>{result.sessionTitle}</strong>
              <small>{result.projectSlug}</small>
              <MarkdownContent content={result.memory} />
            </button>
          ))}
          {latestSearch && latestSearch.results.length === 0 && <EmptyLine text="No matching memories yet." />}
          {!latestSearch && <EmptyLine text="Search results will appear here." />}
        </div>
      </div>

      <div className="panel memory-entity-panel">
        <PanelTitle title="Entities" />
        <div className="list-page data-table entity-table" role="table">
          <div className="table-header" role="row">
            <span role="columnheader">Entity</span>
            <span role="columnheader">Related Sessions</span>
            <span role="columnheader">Created</span>
          </div>
          {entities.map((entity) => (
            <div key={`${entity.entity}-${entityDisplayName(entity)}`} className="entity-row-group">
              <button className="list-row" onClick={() => toggleEntity(entityDisplayName(entity))}>
                <strong>{entityDisplayName(entity)}</strong>
                <span>{entity.sessionCount}</span>
                <small>{entity.createdAt}</small>
              </button>
              {expandedEntity === entityDisplayName(entity) && (
                <div className="entity-sessions sessions-table">
                  {(entitySessions[entityDisplayName(entity)] ?? []).map((session) => (
                    <EntitySessionRow
                      key={session.sessionId}
                      relatedSession={session}
                      session={sessions.find((item) => item.sessionId === session.sessionId) ?? null}
                      onOpen={onOpenSession}
                    />
                  ))}
                  {loadingEntity === entityDisplayName(entity) && <EmptyLine text="Loading related sessions." />}
                  {loadingEntity !== entityDisplayName(entity) && (entitySessions[entityDisplayName(entity)] ?? []).length === 0 && (
                    <EmptyLine text="No related sessions yet." />
                  )}
                </div>
              )}
            </div>
          ))}
          {entities.length === 0 && <EmptyLine text="No entities yet. Refresh memories to build the graph." />}
        </div>
      </div>
    </section>
  );
}

type AnalyzeRange = "3days" | "7days" | "30days" | "All";

function updatedAfterForAnalyzeRange(range: AnalyzeRange) {
  if (range === "All") return undefined;
  const days = Number(range.replace("days", ""));
  return new Date(Date.now() - days * 24 * 60 * 60 * 1000).toISOString();
}

function entityDisplayName(entity: MemoryEntityRecord) {
  return entity.canonicalName || entity.entity;
}

function EntitySessionRow({
  relatedSession,
  session,
  onOpen,
}: {
  relatedSession: MemoryRelatedSession;
  session: SessionRecord | null;
  onOpen: (sessionId: string) => void;
}) {
  return (
    <button className="list-row" onClick={() => onOpen(relatedSession.sessionId)}>
      <strong>{session?.title ?? relatedSession.title}</strong>
      <span>{session?.rawPath ?? relatedSession.sharedEntities.join(", ")}</span>
      <span>{session?.projectSlug ?? relatedSession.projectSlug}</span>
      <span>{session?.taskSlug ?? "unassigned"}</span>
      <small>{session?.source ?? "memory"}</small>
      <small>{session?.status ?? "analyzed"}</small>
      <small>{session ? compactAgeLabel(session.updatedAt || session.createdAt) : ""}</small>
    </button>
  );
}

function SettingsView({
  state,
  busy,
  onSave,
  onResetSessions,
  onResetProjects,
  onResetTasks,
  onResetMemories,
}: {
  state: AppState;
  busy: string | null;
  onSave: (settings: LlmSettings) => void;
  onResetSessions: () => void;
  onResetProjects: () => void;
  onResetTasks: () => void;
  onResetMemories: () => void;
}) {
  const [settings, setSettings] = useState(() => normalizeLlmSettings(state.llmSettings));
  const [draftModel, setDraftModel] = useState<LlmModelSettings>(() =>
    initialDraftModel(normalizeLlmSettings(state.llmSettings)),
  );
  const [editingModelId, setEditingModelId] = useState(draftModel.id);
  const providerCallCounts = providerCallCountsForSettings(settings.models, state.llmProviderCalls ?? []);

  useEffect(() => {
    const normalized = normalizeLlmSettings(state.llmSettings);
    const model = initialDraftModel(normalized);
    setSettings(normalized);
    setDraftModel(model);
    setEditingModelId(model.id);
  }, [state.llmSettings]);

  const providerOptions = state.providerPresets.some((preset) => preset.provider === draftModel.provider)
    ? state.providerPresets
    : [
        {
          provider: draftModel.provider,
          baseUrl: draftModel.baseUrl,
          interface: draftModel.interface,
        },
        ...state.providerPresets,
      ];
  const canSaveModel = Boolean(draftModel.provider.trim() && draftModel.remark.trim());
  const updateScenarioModel = (key: keyof LlmSettings["scenarioModels"], value: string) => {
    setSettings((current) => ({
      ...current,
      scenarioModels: {
        ...current.scenarioModels,
        [key]: value,
      },
    }));
  };
  const saveModelList = () => {
    onSave(settingsWithDefaultModel(settings));
  };
  const saveModel = () => {
    if (!canSaveModel) return;
    const nextModel = {
      ...draftModel,
      id: llmModelId(draftModel.provider, draftModel.remark),
      provider: draftModel.provider.trim(),
      remark: draftModel.remark.trim(),
      baseUrl: draftModel.baseUrl.trim(),
      interface: draftModel.interface.trim() || "openai",
      model: draftModel.model.trim(),
      apiKey: draftModel.apiKey.trim(),
      maxContext: Math.max(0, Math.round(draftModel.maxContext || 0)),
      maxTokens: Math.max(0, Math.round(draftModel.maxTokens || 0)),
      temperature: Number.isFinite(draftModel.temperature) ? draftModel.temperature : 0.2,
    };
    const nextModels = upsertLlmModel(settings.models, editingModelId, nextModel);
    const nextScenarioModels = rewriteScenarioModelIds(settings.scenarioModels, editingModelId, nextModel.id);
    if (!nextScenarioModels.defaultModel) {
      nextScenarioModels.defaultModel = nextModel.id;
    }
    const defaultModel = nextModels.find((model) => model.id === nextScenarioModels.defaultModel) ?? nextModel;
    const nextSettings = {
      ...settings,
      ...llmModelFields(defaultModel),
      models: nextModels,
      scenarioModels: nextScenarioModels,
    };
    setSettings(nextSettings);
    setDraftModel(nextModel);
    setEditingModelId(nextModel.id);
    onSave(nextSettings);
  };

  return (
    <section className="settings-grid">
      <div className="panel model-list-panel">
        <h2>Model List</h2>
        <div className="model-list-card-body">
          <div className="llm-model-list">
            {settings.models.map((model) => (
              <button
                key={model.id}
                className={`llm-model-row ${editingModelId === model.id ? "active" : ""}`}
                onClick={() => {
                  setDraftModel(model);
                  setEditingModelId(model.id);
                }}
              >
                <strong>{model.provider}</strong>
                <span>{model.remark}</span>
              </button>
            ))}
            {settings.models.length === 0 && <p className="empty-line">No saved models yet.</p>}
          </div>
          <button
            aria-label="Add Model"
            className="llm-add-model"
            onClick={() => {
              setDraftModel(blankLlmModel(state.providerPresets));
              setEditingModelId("");
            }}
          >
            <Plus size={18} />
          </button>
          <div className="llm-scenarios">
            <ScenarioSelect
              label="Default model"
              value={settings.scenarioModels.defaultModel}
              models={settings.models}
              required
              onChange={(value) => updateScenarioModel("defaultModel", value)}
            />
            <ScenarioSelect
              label="Project model"
              value={settings.scenarioModels.projectModel}
              models={settings.models}
              onChange={(value) => updateScenarioModel("projectModel", value)}
            />
            <ScenarioSelect
              label="Session model"
              value={settings.scenarioModels.sessionModel}
              models={settings.models}
              onChange={(value) => updateScenarioModel("sessionModel", value)}
            />
            <ScenarioSelect
              label="Memory model"
              value={settings.scenarioModels.memoryModel}
              models={settings.models}
              onChange={(value) => updateScenarioModel("memoryModel", value)}
            />
            <ScenarioSelect
              label="Assistant model"
              value={settings.scenarioModels.assistantModel}
              models={settings.models}
              onChange={(value) => updateScenarioModel("assistantModel", value)}
            />
          </div>
          {providerCallCounts.length > 0 && (
            <div className="provider-call-stats">
              <strong>Provider Calls</strong>
              <div>
                {providerCallCounts.map((item) => (
                  <span key={item.provider}>
                    <em>{item.provider}</em>
                    <b>{item.calls}</b>
                  </span>
                ))}
              </div>
            </div>
          )}
        </div>
        <div className="model-list-actions">
          <IconButton label="Save" icon={<CheckCircle2 size={16} />} onClick={saveModelList} disabled={!settings.scenarioModels.defaultModel} />
        </div>
      </div>
      <div className="panel llm-settings-panel">
        <h2>LLM Settings</h2>
        <label className="settings-form-row">
          <span>Provider</span>
          <select
            aria-label="Provider"
            value={draftModel.provider}
            onChange={(event) => {
              const preset = state.providerPresets.find((item) => item.provider === event.target.value);
              setDraftModel((current) => preset
                ? { ...current, provider: preset.provider, baseUrl: preset.baseUrl, interface: preset.interface }
                : { ...current, provider: event.target.value });
            }}
          >
            {providerOptions.map((preset) => (
              <option key={preset.provider} value={preset.provider}>{preset.provider}</option>
            ))}
          </select>
        </label>
        <label className="settings-form-row">
          <span>Remark</span>
          <input
            required
            value={draftModel.remark}
            onChange={(event) => setDraftModel({ ...draftModel, remark: event.target.value })}
          />
        </label>
        <label className="settings-form-row">
          <span>Interface</span>
          <input value={draftModel.interface} onChange={(event) => setDraftModel({ ...draftModel, interface: event.target.value })} />
        </label>
        <label className="settings-form-row">
          <span>Base URL</span>
          <input value={draftModel.baseUrl} onChange={(event) => setDraftModel({ ...draftModel, baseUrl: event.target.value })} />
        </label>
        <label className="settings-form-row">
          <span>Model</span>
          <input value={draftModel.model} onChange={(event) => setDraftModel({ ...draftModel, model: event.target.value })} />
        </label>
        <label className="settings-form-row">
          <span>API Key</span>
          <input type="password" value={draftModel.apiKey} onChange={(event) => setDraftModel({ ...draftModel, apiKey: event.target.value })} />
        </label>
        <label className="settings-form-row">
          <span>Max Context</span>
          <input
            type="number"
            min={0}
            value={draftModel.maxContext}
            onChange={(event) => setDraftModel({ ...draftModel, maxContext: event.currentTarget.valueAsNumber || 0 })}
          />
        </label>
        <label className="settings-form-row">
          <span>Max Tokens</span>
          <input
            type="number"
            min={0}
            value={draftModel.maxTokens}
            onChange={(event) => setDraftModel({ ...draftModel, maxTokens: event.currentTarget.valueAsNumber || 0 })}
          />
        </label>
        <label className="settings-form-row">
          <span>Temperature</span>
          <input
            type="number"
            min={0}
            max={2}
            step={0.01}
            value={draftModel.temperature}
            onChange={(event) => setDraftModel({ ...draftModel, temperature: event.currentTarget.valueAsNumber || 0 })}
          />
        </label>
        <div className="llm-settings-actions">
          <IconButton label="Save Model" icon={<CheckCircle2 size={16} />} onClick={saveModel} disabled={!canSaveModel} />
        </div>
      </div>
      <div className="panel">
        <h3>Reset State</h3>
        <div className="button-column">
          <IconButton label="Reset Sessions" icon={<RefreshCw size={16} />} onClick={onResetSessions} busy={busy === "Reset sessions"} />
          <IconButton label="Reset Projects" icon={<RefreshCw size={16} />} onClick={onResetProjects} busy={busy === "Reset projects"} />
          <IconButton label="Reset Tasks" icon={<RefreshCw size={16} />} onClick={onResetTasks} busy={busy === "Reset tasks"} />
          <IconButton label="Reset Memories" icon={<RefreshCw size={16} />} onClick={onResetMemories} busy={busy === "Reset memories"} />
        </div>
      </div>
    </section>
  );
}

function ScenarioSelect({
  label,
  value,
  models,
  required = false,
  onChange,
}: {
  label: string;
  value: string;
  models: LlmModelSettings[];
  required?: boolean;
  onChange: (value: string) => void;
}) {
  return (
    <label className="scenario-select-row">
      <span>{label}</span>
      <select
        aria-label={label}
        value={value}
        required={required}
        disabled={models.length === 0}
        onChange={(event) => onChange(event.target.value)}
      >
        {!required && <option value="">Default fallback</option>}
        {required && <option value="">Select model</option>}
        {models.map((model) => (
          <option key={model.id} value={model.id}>{model.provider} {model.remark}</option>
        ))}
      </select>
    </label>
  );
}

function normalizeLlmSettings(settings: LlmSettings): LlmSettings {
  const models = (settings.models?.length ? settings.models : legacyLlmModel(settings)).map((model) =>
    normalizeLlmModel(model, settings),
  );
  const legacyScenarioModels = settings.scenarioModels as LlmSettings["scenarioModels"] & { taskModel?: string };
  const scenarioModels = {
    defaultModel: settings.scenarioModels?.defaultModel || models[0]?.id || "",
    projectModel: settings.scenarioModels?.projectModel || "",
    sessionModel: settings.scenarioModels?.sessionModel || "",
    memoryModel: settings.scenarioModels?.memoryModel || "",
    assistantModel: settings.scenarioModels?.assistantModel || legacyScenarioModels?.taskModel || "",
  };
  const activeModel = models.find((model) => model.id === scenarioModels.defaultModel) ?? models[0];
  return {
    ...settings,
    ...llmModelFields(activeModel ?? modelFromSettings(settings)),
    models,
    scenarioModels,
  };
}

function initialDraftModel(settings: LlmSettings) {
  return settings.models.find((model) => model.id === settings.scenarioModels.defaultModel)
    ?? settings.models[0]
    ?? modelFromSettings(settings);
}

function legacyLlmModel(settings: LlmSettings): LlmModelSettings[] {
  if (!settings.model && !settings.apiKey) return [];
  return [modelFromSettings(settings)];
}

function modelFromSettings(settings: LlmSettings): LlmModelSettings {
  const remark = settings.remark || "Default";
  return {
    id: settings.id || llmModelId(settings.provider, remark),
    provider: settings.provider,
    remark,
    baseUrl: settings.baseUrl,
    interface: settings.interface,
    model: settings.model,
    apiKey: settings.apiKey,
    maxContext: settings.maxContext ?? 128000,
    maxTokens: settings.maxTokens ?? 4096,
    temperature: settings.temperature ?? 0.2,
  };
}

function llmModelFields(model: LlmModelSettings) {
  return {
    id: model.id,
    provider: model.provider,
    remark: model.remark,
    baseUrl: model.baseUrl,
    interface: model.interface,
    model: model.model,
    apiKey: model.apiKey,
    maxContext: model.maxContext,
    maxTokens: model.maxTokens,
    temperature: model.temperature,
  };
}

function blankLlmModel(presets: AppState["providerPresets"]): LlmModelSettings {
  const preset = presets[0] ?? { provider: "", baseUrl: "", interface: "openai" };
  return {
    id: "",
    provider: preset.provider,
    remark: "",
    baseUrl: preset.baseUrl,
    interface: preset.interface,
    model: "",
    apiKey: "",
    maxContext: 128000,
    maxTokens: 4096,
    temperature: 0.2,
  };
}

function settingsWithDefaultModel(settings: LlmSettings) {
  const defaultModel = settings.models.find((model) => model.id === settings.scenarioModels.defaultModel);
  return defaultModel ? { ...settings, ...llmModelFields(defaultModel) } : settings;
}

function upsertLlmModel(models: LlmModelSettings[], editingModelId: string, nextModel: LlmModelSettings) {
  const withoutCurrent = models.filter((model) => model.id !== editingModelId && model.id !== nextModel.id);
  return [...withoutCurrent, nextModel];
}

function rewriteScenarioModelIds(
  scenarioModels: LlmSettings["scenarioModels"],
  oldModelId: string,
  nextModelId: string,
) {
  return {
    defaultModel: scenarioModels.defaultModel === oldModelId ? nextModelId : scenarioModels.defaultModel,
    projectModel: scenarioModels.projectModel === oldModelId ? nextModelId : scenarioModels.projectModel,
    sessionModel: scenarioModels.sessionModel === oldModelId ? nextModelId : scenarioModels.sessionModel,
    memoryModel: scenarioModels.memoryModel === oldModelId ? nextModelId : scenarioModels.memoryModel,
    assistantModel: scenarioModels.assistantModel === oldModelId ? nextModelId : scenarioModels.assistantModel,
  };
}

function providerCallCountsForSettings(
  models: LlmModelSettings[],
  callCounts: AppState["llmProviderCalls"],
) {
  const rows = callCounts.filter((item) => item.calls > 0 && item.provider.trim());
  const seen = new Set(rows.map((item) => item.provider));
  for (const model of models) {
    const provider = model.provider.trim();
    if (!provider || seen.has(provider)) continue;
    rows.push({ provider, calls: 0 });
    seen.add(provider);
  }
  return rows;
}

function normalizeLlmModel(model: LlmModelSettings, settings: LlmSettings): LlmModelSettings {
  return {
    ...model,
    maxContext: model.maxContext ?? settings.maxContext ?? 128000,
    maxTokens: model.maxTokens ?? settings.maxTokens ?? 4096,
    temperature: model.temperature ?? settings.temperature ?? 0.2,
  };
}

function llmModelId(provider: string, remark: string) {
  return `${provider.trim().toLowerCase().replace(/\s+/g, "-")}-${remark.trim().toLowerCase().replace(/\s+/g, "-")}`;
}

function Metric({
  icon,
  label,
  value,
  detail,
  tone,
}: {
  icon: React.ReactNode;
  label: string;
  value: number;
  detail: string;
  tone: "projects" | "tasks" | "sessions" | "memory";
}) {
  return (
    <section className={`metric metric-${tone}`}>
      <span className="metric-icon">{icon}</span>
      <small>{label}</small>
      <strong>{String(value).padStart(2, "0")}</strong>
      <em>{detail}</em>
    </section>
  );
}

function NavButton({ active, icon, label, onClick }: { active: boolean; icon: React.ReactNode; label: string; onClick: () => void }) {
  return (
    <button className={`nav-button ${active ? "active" : ""}`} onClick={onClick}>
      {icon}
      <span>{label}</span>
    </button>
  );
}

function IconButton({ label, icon, onClick, busy = false, disabled = false }: { label: string; icon: React.ReactNode; onClick: () => void; busy?: boolean; disabled?: boolean }) {
  return (
    <button className="icon-button" onClick={onClick} disabled={busy || disabled}>
      {busy ? <Loader2 className="spin" size={16} /> : icon}
      <span>{label}</span>
    </button>
  );
}

function PanelTitle({ title, action }: { title: string; action?: React.ReactNode }) {
  return (
    <div className="panel-title">
      <h2>{title}</h2>
      {action}
    </div>
  );
}

function StatusPill({ tone, icon, label }: { tone: "green" | "amber" | "cyan"; icon: React.ReactNode; label: string }) {
  return (
    <span className={`status-pill ${tone}`}>
      {icon}
      {label}
    </span>
  );
}

function TaskDonut({ tasks }: { tasks: TaskRecord[] }) {
  const discussing = tasks.filter((task) => task.status === "discussing").length;
  const developing = tasks.filter((task) => task.status === "developing").length;
  const done = tasks.filter((task) => task.status === "done").length;
  const total = Math.max(1, discussing + developing + done);
  const discussingStop = Math.round((discussing / total) * 100);
  const developingStop = discussingStop + Math.round((developing / total) * 100);
  return (
    <div
      className="task-donut"
      style={{
        background: `conic-gradient(#00d8ff 0 ${discussingStop}%, #7fff5e ${discussingStop}% ${developingStop}%, #25d982 ${developingStop}% 100%)`,
      }}
      aria-hidden="true"
    >
      <span />
    </div>
  );
}

function JobsTable({ jobs, onStop }: { jobs: AppState["jobs"]; onStop: (jobId: number) => void }) {
  if (!jobs.length) {
    return <p className="empty-line">No active analysis jobs.</p>;
  }
  return (
    <div className="jobs-table">
      {jobs.map((job) => (
        <div className="jobs-row" key={job.id}>
          <strong>{job.kind}</strong>
          <span>{job.completed} / {job.total}</span>
          <span>{job.pending} pending</span>
          <span>{elapsedLabel(job.startedAt)}</span>
          <em>{job.status}</em>
          <IconButton label="Stop" icon={<CircleStop size={16} />} onClick={() => onStop(job.id)} />
        </div>
      ))}
    </div>
  );
}

function elapsedLabel(startedAt: string) {
  const started = Date.parse(startedAt);
  if (Number.isNaN(started)) return "elapsed unknown";
  const seconds = Math.max(0, Math.floor((Date.now() - started) / 1000));
  const minutes = Math.floor(seconds / 60);
  const remainder = seconds % 60;
  return `${minutes}:${String(remainder).padStart(2, "0")}`;
}

function compactAgeLabel(timestamp: string) {
  const parsed = Date.parse(timestamp);
  if (Number.isNaN(parsed)) return "unknown";
  const minutes = Math.max(1, Math.floor((Date.now() - parsed) / 60000));
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

function StatusCount({ label, value }: { label: string; value: number }) {
  return (
    <span className="status-count">
      <i />
      {label}
      <strong>{value}</strong>
    </span>
  );
}

function EmptyLine({ text }: { text: string }) {
  return <p className="empty-line">{text}</p>;
}

function titleFor(view: View, project?: ProjectRecord, task?: TaskRecord) {
  if (view === "projects") return "Projects";
  if (view === "projectDetail") return project?.displayTitle ?? "Projects";
  if (view === "tasks") return "Tasks";
  if (view === "taskDetail") return task?.title ?? "Tasks";
  if (view === "sessions") return "Sessions";
  if (view === "sessionDetail") return "Session Detail";
  if (view === "memories") return "Memory";
  if (view === "settings") return "Settings";
  return "Dashboard";
}
