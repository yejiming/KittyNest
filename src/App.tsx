import {
  Activity,
  Boxes,
  BrainCircuit,
  CheckCircle2,
  CircleStop,
  CircleDot,
  Database,
  FolderKanban,
  Gauge,
  History,
  Loader2,
  RefreshCw,
  ScanLine,
  Settings,
  ShieldCheck,
  Sparkles,
  TerminalSquare,
  Trash2,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  createTask,
  enqueueAnalyzeSession,
  enqueueAnalyzeProject,
  enqueueAnalyzeSessions,
  enqueueScanSources,
  getActiveJobs,
  getAppState,
  getCachedAppState,
  readMarkdownFile,
  deleteTask,
  resetProjects,
  resetSessions,
  resetTasks,
  saveLlmSettings,
  updateTaskStatus,
  isTauriRuntime,
  stopJob,
} from "./api";
import type { AppState, LlmSettings, ProjectRecord, SessionRecord, TaskRecord } from "./types";

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

  const queueAllSessions = (label = "Queue session analysis", updatedAfter?: string) =>
    runQueueAction(label, async () => {
      const result = await enqueueAnalyzeSessions(updatedAfter);
      return `Analysis queued: ${result.total} session${result.total === 1 ? "" : "s"}`;
    });

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

  const createManualTask = (projectSlug: string, userPrompt: string) =>
    runQueueAction("Create task", async () => {
      const result = await createTask(projectSlug, userPrompt);
      return `Task prompt queued: ${result.taskSlug}`;
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
        <button className="brand" onClick={() => setView("dashboard")}>
          <Boxes size={32} />
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
        <div className="ledger">
          <ShieldCheck size={24} />
          <strong>Local Ledger</strong>
          <span>SQLite synced</span>
          <small>{state.dataDir}</small>
        </div>
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
            projects={state.projects}
            busy={busy}
            onCreate={createManualTask}
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
            onOpenSession={(sessionId) => {
              setSelectedSession(sessionId);
              setView("sessionDetail");
            }}
          />
        )}

        {view === "sessions" && (
          <SessionsList
            sessions={state.sessions}
            busy={busy}
            onAnalyze={(updatedAfter) => queueAllSessions("Queue session analysis", updatedAfter)}
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
          />
        )}
        {view === "memories" && <MemoryView state={state} />}
        {view === "settings" && <SettingsView state={state} onSave={(settings) => runAction("Save settings", async () => {
          await saveLlmSettings(settings);
          return "LLM settings saved";
        })}
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
        />}
      </main>

      <footer className="statusbar">
        <span><TerminalSquare size={16} /> Tauri v2</span>
        <span><Database size={16} /> SQLite synced</span>
        <span><Activity size={16} /> {notice}</span>
      </footer>
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
  onStopJob: (jobId: number) => void;
}) {
  const recentSessions = [...state.sessions].sort(
    (a, b) => Date.parse(b.updatedAt || b.createdAt) - Date.parse(a.updatedAt || a.createdAt),
  );
  return (
    <section className="dashboard-grid">
      <Metric icon={<FolderKanban />} label="Active Projects" value={state.stats.activeProjects} detail={`${state.projects.filter((p) => p.reviewStatus !== "reviewed").length} need review`} />
      <Metric icon={<History />} label="Sessions" value={state.stats.sessions} detail={`${state.stats.unprocessedSessions} new`} />
      <Metric icon={<CheckCircle2 />} label="Open Tasks" value={state.stats.openTasks} detail={`${state.tasks.filter((task) => task.status === "developing").length} in development`} />
      <Metric icon={<BrainCircuit />} label="Memory Updates" value={state.stats.memories} detail="local memory" />

      <section className="panel projects-panel">
        <PanelTitle title="Projects" action={<IconButton label="Scan" icon={<ScanLine size={16} />} onClick={onScan} busy={busy === "Scan new sessions"} />} />
        <div className="project-list panel-scroll five-rows">
          {state.projects.length === 0 && <EmptyLine text="No projects yet. Scan sources to discover local sessions." />}
          {state.projects.map((project) => (
            <button key={project.slug} className="project-row" onClick={() => onOpenProject(project.slug)}>
              <span className="hex">{project.slug.slice(0, 1)}</span>
              <span>
                <strong>{project.displayTitle}</strong>
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
              <span>{session.title ?? session.sessionId}</span>
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
        <PanelTitle title="Memory Pulse" action={<IconButton label="Refresh" icon={<RefreshCw size={16} />} onClick={() => undefined} />} />
        <div className="pulse">
          <BrainCircuit size={54} />
          <strong>Project memory draft</strong>
          <span>{state.stats.memories} memory files indexed</span>
        </div>
      </section>

      <section className="panel jobs-panel">
        <PanelTitle title="Analyze Jobs" action={<StatusPill tone="cyan" icon={<Activity size={16} />} label={busy ? "Running" : "Idle"} />} />
        <JobsTable jobs={state.jobs} onStop={onStopJob} />
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
  projects,
  busy,
  onCreate,
  onOpen,
}: {
  tasks: TaskRecord[];
  projects: ProjectRecord[];
  busy: string | null;
  onCreate: (projectSlug: string, userPrompt: string) => void;
  onOpen: (projectSlug: string, taskSlug: string) => void;
}) {
  const reviewedProjects = projects.filter((project) => project.reviewStatus === "reviewed");
  const [projectSlug, setProjectSlug] = useState(reviewedProjects[0]?.slug ?? "");
  const [userPrompt, setUserPrompt] = useState("");
  useEffect(() => {
    if (!projectSlug && reviewedProjects[0]) {
      setProjectSlug(reviewedProjects[0].slug);
    }
  }, [projectSlug, reviewedProjects]);
  const canCreate = Boolean(projectSlug && userPrompt.trim());
  return (
    <section className="detail-layout">
      <div className="panel wide">
        <PanelTitle title="Create Task" />
        <div className="task-create-form">
          <label>
            Project
            <select
              value={projectSlug}
              onChange={(event) => setProjectSlug(event.target.value)}
              disabled={!reviewedProjects.length}
            >
              {reviewedProjects.map((project) => (
                <option key={project.slug} value={project.slug}>{project.displayTitle}</option>
              ))}
            </select>
          </label>
          <label>
            Task Prompt
            <textarea
              value={userPrompt}
              onChange={(event) => setUserPrompt(event.target.value)}
              placeholder="Describe the task you want KittyNest to refine."
            />
          </label>
          <IconButton
            label="Create Task"
            icon={<CircleDot size={16} />}
            onClick={() => {
              onCreate(projectSlug, userPrompt.trim());
              setUserPrompt("");
            }}
            busy={busy === "Create task"}
            disabled={!canCreate}
          />
          {!reviewedProjects.length && <EmptyLine text="Review a project before creating tasks." />}
        </div>
      </div>
      <div className="panel wide">
        <PanelTitle title="Tasks" />
        <div className="list-page data-table tasks-table" role="table">
          <div className="table-header" role="row">
            <span role="columnheader">Name</span>
            <span role="columnheader">Project</span>
            <span role="columnheader">Status</span>
            <span role="columnheader">Sessions</span>
          </div>
          {tasks.map((task) => (
            <button key={`${task.projectSlug}-${task.slug}`} className="list-row" onClick={() => onOpen(task.projectSlug, task.slug)}>
              <strong>{task.title}</strong>
              <span>{task.projectSlug}</span>
              <small>{task.status}</small>
              <small>{task.sessionCount}</small>
            </button>
          ))}
          {tasks.length === 0 && <EmptyLine text="No tasks yet." />}
        </div>
      </div>
    </section>
  );
}

function SessionsList({
  sessions,
  busy,
  onAnalyze,
  onOpen,
}: {
  sessions: SessionRecord[];
  busy: string | null;
  onAnalyze: (updatedAfter?: string) => void;
  onOpen: (sessionId: string) => void;
}) {
  const [updatedWindow, setUpdatedWindow] = useState("7");
  const updatedAfter = updatedWindow === "all"
    ? undefined
    : new Date(Date.now() - Number(updatedWindow) * 24 * 60 * 60 * 1000).toISOString();
  return (
    <section className="detail-layout">
      <div className="panel wide">
        <PanelTitle
          title="Sessions"
          action={
            <div className="panel-actions">
              <label className="inline-select">
                Updated
                <select value={updatedWindow} onChange={(event) => setUpdatedWindow(event.target.value)}>
                  <option value="3">3 days</option>
                  <option value="7">7 days</option>
                  <option value="30">30 days</option>
                  <option value="90">90 days</option>
                  <option value="all">All</option>
                </select>
              </label>
              <IconButton label="Analyze" icon={<Sparkles size={16} />} onClick={() => onAnalyze(updatedAfter)} busy={busy === "Queue session analysis"} />
            </div>
          }
        />
        <div className="list-page data-table sessions-table" role="table">
          <div className="table-header" role="row">
            <span role="columnheader">Name</span>
            <span role="columnheader">Path</span>
            <span role="columnheader">Project</span>
            <span role="columnheader">Task</span>
            <span role="columnheader">Source</span>
            <span role="columnheader">Status</span>
            <span role="columnheader">Updated</span>
          </div>
          {sessions.map((session) => (
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
          {sessions.length === 0 && <EmptyLine text="No sessions yet." />}
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
        </div>
        <div className="button-row">
          <IconButton label="Analyze" icon={<Sparkles size={16} />} onClick={onAnalyze} busy={busy === "Queue project analyze"} />
        </div>
      </div>
      <div className="markdown-stack">
        <MarkdownPanel title="Project Summary" path={project.infoPath} empty="Review has not been generated yet." />
        <MarkdownPanel title="Progress" path={project.progressPath} empty="Import historical sessions to generate progress." />
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

function MarkdownPanel({ title, path, empty }: { title: string; path: string | null; empty: string }) {
  const [content, setContent] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    setContent("");
    setError("");
    if (!path) return;
    void readMarkdownFile(path)
      .then((result) => setContent(result.content))
      .catch((error) => setError(error instanceof Error ? error.message : String(error)));
  }, [path]);

  return (
    <div className="panel markdown-panel">
      <h3>{title}</h3>
      <div className="markdown-scroll">
        {content ? <MarkdownBlock content={content} /> : <p>{error || empty}</p>}
      </div>
    </div>
  );
}

function MarkdownBlock({ content }: { content: string }) {
  const lines = content.replace(/^---[\s\S]*?---\s*/, "").split(/\r?\n/);
  const nodes: React.ReactNode[] = [];
  let index = 0;
  while (index < lines.length) {
    const line = lines[index];
    const next = lines[index + 1];
    if (isMarkdownTableStart(line, next)) {
      const headers = parseMarkdownTableRow(line);
      const rows: string[][] = [];
      index += 2;
      while (index < lines.length && parseMarkdownTableRow(lines[index]).length) {
        rows.push(parseMarkdownTableRow(lines[index]));
        index += 1;
      }
      nodes.push(
        <table key={`table-${index}`}>
          <thead>
            <tr>
              {headers.map((header, cellIndex) => (
                <th key={cellIndex}>{renderInline(header)}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row, rowIndex) => (
              <tr key={rowIndex}>
                {headers.map((_, cellIndex) => (
                  <td key={cellIndex}>{renderInline(row[cellIndex] ?? "")}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>,
      );
      continue;
    }

    if (line.startsWith("### ")) nodes.push(<h3 key={index}>{renderInline(line.slice(4))}</h3>);
    else if (line.startsWith("## ")) nodes.push(<h2 key={index}>{renderInline(line.slice(3))}</h2>);
    else if (line.startsWith("# ")) nodes.push(<h1 key={index}>{renderInline(line.slice(2))}</h1>);
    else if (line.startsWith("- ")) nodes.push(<p key={index} className="markdown-list-item">{renderInline(line.slice(2))}</p>);
    else if (!line.trim()) nodes.push(<br key={index} />);
    else nodes.push(<p key={index}>{renderInline(line)}</p>);
    index += 1;
  }
  return (
    <div className="markdown-body">
      {nodes}
    </div>
  );
}

function isMarkdownTableStart(line: string, next?: string) {
  const headers = parseMarkdownTableRow(line);
  return headers.length > 0 && Boolean(next && isMarkdownTableSeparator(next, headers.length));
}

function isMarkdownTableSeparator(line: string, columnCount: number) {
  const cells = parseMarkdownTableRow(line);
  return cells.length === columnCount && cells.every((cell) => /^:?-{3,}:?$/.test(cell));
}

function parseMarkdownTableRow(line: string) {
  const trimmed = line.trim();
  if (!trimmed.includes("|")) return [];
  const withoutEdges = trimmed.replace(/^\|/, "").replace(/\|$/, "");
  const cells = withoutEdges.split("|").map((cell) => cell.trim());
  return cells.length > 1 && cells.every((cell) => cell.length > 0) ? cells : [];
}

function renderInline(text: string) {
  const nodes: React.ReactNode[] = [];
  const pattern = /(\*\*[^*]+\*\*|`[^`]+`)/g;
  let lastIndex = 0;
  for (const match of text.matchAll(pattern)) {
    const index = match.index ?? 0;
    if (index > lastIndex) {
      nodes.push(text.slice(lastIndex, index));
    }
    const token = match[0];
    if (token.startsWith("**")) {
      nodes.push(<strong key={index}>{token.slice(2, -2)}</strong>);
    } else {
      nodes.push(<code key={index}>{token.slice(1, -1)}</code>);
    }
    lastIndex = index + token.length;
  }
  if (lastIndex < text.length) {
    nodes.push(text.slice(lastIndex));
  }
  return nodes.length ? nodes : text;
}

function TaskView({
  task,
  sessions,
  onStatus,
  onDelete,
  onOpenSession,
}: {
  task: TaskRecord;
  sessions: SessionRecord[];
  onStatus: (status: string) => void;
  onDelete: () => void;
  onOpenSession: (sessionId: string) => void;
}) {
  const orderedSessions = [...sessions].sort((left, right) => left.updatedAt.localeCompare(right.updatedAt));
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
            <MarkdownBlock content={session.summary ?? "No session summary yet."} />
          </button>
        ))}
        {orderedSessions.length === 0 && <div className="panel wide"><EmptyLine text="No session summaries yet." /></div>}
      </div>
    </section>
  );
}

function taskDirectoryPath(task: TaskRecord) {
  return task.summaryPath.endsWith("/summary.md")
    ? task.summaryPath.slice(0, -"/summary.md".length)
    : task.summaryPath;
}

function SessionView({
  session,
  busy,
  onAnalyze,
}: {
  session: SessionRecord;
  busy: string | null;
  onAnalyze: () => void;
}) {
  return (
    <section className="detail-stack">
      <div className="panel wide">
        <PanelTitle title={session.title ?? session.sessionId} action={<IconButton label="Analyze" icon={<Sparkles size={16} />} onClick={onAnalyze} busy={busy === "Queue session analysis"} />} />
        <div className="session-paths">
          <ProjectPath label="Original Path" value={session.rawPath} />
          <ProjectPath label="System Path" value={session.summaryPath ?? "Not generated yet."} />
        </div>
      </div>
      <MarkdownPanel title="Markdown" path={session.summaryPath} empty="Session markdown has not been written yet." />
    </section>
  );
}

function MemoryView({ state }: { state: AppState }) {
  return (
    <section className="detail-layout">
      <div className="panel wide">
        <h2>Memories</h2>
        <p>Project and system memories are reserved for the next milestone group.</p>
      </div>
      {state.projects.map((project) => (
        <div className="panel" key={project.slug}>
          <h3>{project.displayTitle}</h3>
          <p>{project.progressPath ?? "No project progress yet."}</p>
        </div>
      ))}
    </section>
  );
}

function SettingsView({
  state,
  busy,
  onSave,
  onResetSessions,
  onResetProjects,
  onResetTasks,
}: {
  state: AppState;
  busy: string | null;
  onSave: (settings: LlmSettings) => void;
  onResetSessions: () => void;
  onResetProjects: () => void;
  onResetTasks: () => void;
}) {
  const [settings, setSettings] = useState(state.llmSettings);
  return (
    <section className="settings-grid">
      <div className="panel wide">
        <h2>LLM Provider</h2>
        <label>
          Provider preset
          <select
            value={settings.provider}
            onChange={(event) => {
              const preset = state.providerPresets.find((item) => item.provider === event.target.value);
              if (preset) {
                setSettings({ ...settings, provider: preset.provider, baseUrl: preset.baseUrl, interface: preset.interface });
              }
            }}
          >
            {state.providerPresets.map((preset) => (
              <option key={preset.provider} value={preset.provider}>{preset.provider}</option>
            ))}
          </select>
        </label>
        <label>
          Interface
          <input value={settings.interface} onChange={(event) => setSettings({ ...settings, interface: event.target.value })} />
        </label>
        <label>
          Base URL
          <input value={settings.baseUrl} onChange={(event) => setSettings({ ...settings, baseUrl: event.target.value })} />
        </label>
        <label>
          Model
          <input value={settings.model} onChange={(event) => setSettings({ ...settings, model: event.target.value })} />
        </label>
        <label>
          API Key
          <input type="password" value={settings.apiKey} onChange={(event) => setSettings({ ...settings, apiKey: event.target.value })} />
        </label>
        <IconButton label="Save Settings" icon={<CheckCircle2 size={16} />} onClick={() => onSave(settings)} />
      </div>
      <div className="panel">
        <h3>Session Sources</h3>
        {state.sourceStatuses.map((source) => (
          <p key={source.source} className={source.exists ? "ok" : "warn"}>
            {source.source}: {source.path}
          </p>
        ))}
      </div>
      <div className="panel">
        <h3>Reset State</h3>
        <div className="button-column">
          <IconButton label="Reset Sessions" icon={<RefreshCw size={16} />} onClick={onResetSessions} busy={busy === "Reset sessions"} />
          <IconButton label="Reset Projects" icon={<RefreshCw size={16} />} onClick={onResetProjects} busy={busy === "Reset projects"} />
          <IconButton label="Reset Tasks" icon={<RefreshCw size={16} />} onClick={onResetTasks} busy={busy === "Reset tasks"} />
        </div>
      </div>
    </section>
  );
}

function Metric({ icon, label, value, detail }: { icon: React.ReactNode; label: string; value: number; detail: string }) {
  return (
    <section className="metric">
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
