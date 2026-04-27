import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import * as api from "./api";
import type { AppState } from "./types";

type ApiWithReviewQueue = typeof api & {
  createTask: (projectSlug: string, userPrompt: string) => Promise<api.CreateTaskResult>;
  deleteTask: (projectSlug: string, taskSlug: string) => Promise<{ deleted: boolean }>;
  enqueueAnalyzeProject: (projectSlug: string) => Promise<{ jobId: number; total: number }>;
  enqueueAnalyzeProjectSessions: (projectSlug: string) => Promise<{ jobId: number; total: number }>;
  enqueueReviewProject: (projectSlug: string) => Promise<{ jobId: number; total: number }>;
  enqueueScanSources: () => Promise<{ jobId: number; total: number }>;
  getCachedAppState: () => Promise<AppState>;
  resetProjects: () => Promise<{ reset: number }>;
  resetSessions: () => Promise<{ reset: number }>;
  resetTasks: () => Promise<{ reset: number }>;
  stopJob: (jobId: number) => Promise<{ stopped: boolean }>;
};

const state: AppState = {
  dataDir: "/Users/kc/.kittynest",
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
    { source: "Claude Code", path: "/Users/kc/.claude", exists: true },
    { source: "Codex", path: "/Users/kc/.codex", exists: true },
  ],
  stats: {
    activeProjects: 1,
    openTasks: 1,
    sessions: 2,
    unprocessedSessions: 1,
    memories: 0,
  },
  projects: [
    {
      slug: "KittyNest",
      displayTitle: "KittyNest",
      workdir: "/Users/kc/KittyNest",
      sources: ["codex"],
      infoPath: null,
      progressPath: "/Users/kc/.kittynest/projects/KittyNest/progress.md",
      reviewStatus: "not_reviewed",
      lastReviewedAt: null,
      lastSessionAt: "2026-04-26T01:00:00Z",
    },
  ],
  tasks: [
    {
      projectSlug: "KittyNest",
      slug: "session-ingest",
      title: "Session Ingest",
      brief: "User goal: import sessions",
      status: "developing",
      summaryPath: "/Users/kc/.kittynest/projects/KittyNest/tasks/session-ingest/summary.md",
      sessionCount: 2,
      updatedAt: "2026-04-26T01:00:00Z",
    },
  ],
  sessions: [
    {
      source: "codex",
      sessionId: "abc",
      projectSlug: "KittyNest",
      taskSlug: "session-ingest",
      title: "Import Sessions",
      summary: "Session summary",
      summaryPath: "/Users/kc/.kittynest/projects/KittyNest/sessions/abc/summary.md",
      rawPath: "/Users/kc/.codex/sessions/2026/04/26/abc.jsonl",
      createdAt: "2026-04-26T00:30:00Z",
      updatedAt: "2026-04-26T01:00:00Z",
      status: "analyzed",
    },
  ],
  jobs: [],
};

describe("KittyNest dashboard", () => {
  beforeEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("renders project/session state and queues manual scan", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue(state);
    const scan = vi.spyOn(api, "scanSources").mockResolvedValue({
      found: 2,
      inserted: 1,
      codexFound: 1,
      claudeFound: 1,
    });
    const enqueueScan = vi
      .spyOn(api as ApiWithReviewQueue, "enqueueScanSources")
      .mockResolvedValue({ jobId: 11, total: 1 });

    render(<App />);

    expect(await screen.findByRole("heading", { name: "Dashboard" })).toBeInTheDocument();
    expect(screen.getByText("Active Projects")).toBeInTheDocument();
    expect(screen.getByText("Import Sessions")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /scan new sessions/i }));

    await waitFor(() => expect(enqueueScan).toHaveBeenCalledTimes(1));
    expect(scan).not.toHaveBeenCalled();
    expect(await screen.findByText(/scan queued/i)).toBeInTheDocument();
  });

  it("shows a load error instead of staying on the boot screen", async () => {
    vi.spyOn(api, "getAppState").mockRejectedValue(new Error("database is locked"));

    render(<App />);

    expect(await screen.findByText(/database is locked/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /retry/i })).toBeInTheDocument();
  });

  it("enqueues project analyze from project detail with one action", async () => {
    const getState = vi.spyOn(api, "getAppState").mockResolvedValue(state);
    const enqueue = vi
      .spyOn(api as ApiWithReviewQueue, "enqueueAnalyzeProject")
      .mockResolvedValue({ jobId: 7, total: 3 });
    const enqueueProjectSessions = vi
      .spyOn(api as ApiWithReviewQueue, "enqueueAnalyzeProjectSessions")
      .mockResolvedValue({ jobId: 9, total: 1 });
    const enqueueReview = vi
      .spyOn(api as ApiWithReviewQueue, "enqueueReviewProject")
      .mockResolvedValue({ jobId: 10, total: 1 });
    const enqueueAll = vi.spyOn(api, "enqueueAnalyzeSessions").mockResolvedValue({ jobId: 8, total: 1 });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^projects$/i }));
    await userEvent.click(screen.getByRole("button", { name: /not reviewed/i }));
    await userEvent.click(screen.getByRole("button", { name: /^analyze$/i }));

    await waitFor(() => expect(enqueue).toHaveBeenCalledWith("KittyNest"));
    expect(screen.queryByRole("button", { name: /review project/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /import historical sessions/i })).not.toBeInTheDocument();
    expect(enqueueProjectSessions).not.toHaveBeenCalled();
    expect(enqueueReview).not.toHaveBeenCalled();
    expect(enqueueAll).not.toHaveBeenCalled();
    expect(getState).toHaveBeenCalledTimes(1);
    expect(await screen.findByText(/project analysis queued: 3 steps/i)).toBeInTheDocument();
  });

  it("enqueues unprocessed sessions updated within the selected window from the sessions list", async () => {
    vi.spyOn(Date, "now").mockReturnValue(Date.parse("2026-04-26T03:00:00Z"));
    vi.spyOn(api, "getAppState").mockResolvedValue(state);
    const enqueue = vi
      .spyOn(api, "enqueueAnalyzeSessions")
      .mockResolvedValue({ jobId: 8, total: 1 });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^sessions$/i }));
    expect(screen.getByRole("combobox", { name: /updated/i })).toHaveValue("7");
    expect(screen.getByRole("option", { name: /3 days/i })).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByRole("combobox", { name: /updated/i }), "3");
    await userEvent.click(screen.getByRole("button", { name: /^analyze$/i }));

    await waitFor(() =>
      expect(enqueue).toHaveBeenCalledWith("2026-04-23T03:00:00.000Z"),
    );
  });

  it("enqueues one session from session detail", async () => {
    const getState = vi.spyOn(api, "getAppState").mockResolvedValue(state);
    const enqueue = vi
      .spyOn(api, "enqueueAnalyzeSession")
      .mockResolvedValue({ jobId: 9, total: 1 });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^sessions$/i }));
    await userEvent.click(screen.getByRole("button", { name: /import sessions/i }));
    await userEvent.click(screen.getByRole("button", { name: /^analyze$/i }));

    await waitFor(() => expect(enqueue).toHaveBeenCalledWith("abc"));
    expect(getState).toHaveBeenCalledTimes(1);
  });

  it("opens list pages before item detail pages", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue(state);

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^projects$/i }));
    expect(screen.getAllByRole("heading", { name: "Projects" }).length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: /not reviewed/i })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /^tasks$/i }));
    expect(screen.getAllByRole("heading", { name: "Tasks" }).length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: /session ingest/i })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /^sessions$/i }));
    expect(screen.getAllByRole("heading", { name: "Sessions" }).length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: /import sessions/i })).toBeInTheDocument();
  });

  it("creates a manual task from the tasks list for reviewed projects", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue({
      ...state,
      projects: [{ ...state.projects[0], reviewStatus: "reviewed" }],
    });
    const createTask = vi
      .spyOn(api as ApiWithReviewQueue, "createTask")
      .mockResolvedValue({
        projectSlug: "KittyNest",
        taskSlug: "ship-next-milestone",
        jobId: 12,
        total: 1,
        userPromptPath: "/Users/kc/.kittynest/projects/KittyNest/tasks/ship-next-milestone/user_prompt.md",
        llmPromptPath: "/Users/kc/.kittynest/projects/KittyNest/tasks/ship-next-milestone/llm_prompt.md",
      });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^tasks$/i }));
    await userEvent.selectOptions(screen.getByRole("combobox", { name: /project/i }), "KittyNest");
    await userEvent.type(screen.getByLabelText(/task prompt/i), "Ship next milestone");
    await userEvent.click(screen.getByRole("button", { name: /^create task$/i }));

    await waitFor(() => expect(createTask).toHaveBeenCalledWith("KittyNest", "Ship next milestone"));
    expect(await screen.findByText(/task prompt queued: ship-next-milestone/i)).toBeInTheDocument();
  });

  it("renders project summary and progress markdown content", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue({
      ...state,
      projects: [
        {
          ...state.projects[0],
          infoPath: "/Users/kc/.kittynest/projects/KittyNest/summary.md",
          progressPath: "/Users/kc/.kittynest/projects/KittyNest/progress.md",
        },
      ],
    });
    vi.spyOn(api, "readMarkdownFile")
      .mockResolvedValueOnce({ content: "# Summary\n\n- Rust + Tauri" })
      .mockResolvedValueOnce({ content: "# Progress\n\n- Done item" });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^projects$/i }));
    await userEvent.click(screen.getByRole("button", { name: /not reviewed/i }));

    expect(await screen.findByRole("heading", { name: "Summary" })).toBeInTheDocument();
    expect(await screen.findByText("Rust + Tauri")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Progress", level: 1 })).toBeInTheDocument();
    expect(await screen.findByText("Done item")).toBeInTheDocument();
    expect(screen.getByText("Project Path")).toBeInTheDocument();
    expect(screen.getByText("/Users/kc/KittyNest")).toBeInTheDocument();
    expect(screen.getByText("Project Summary Path")).toBeInTheDocument();
    expect(screen.getByText("/Users/kc/.kittynest/projects/KittyNest/summary.md")).toBeInTheDocument();
    expect(screen.getByText("Project Progress Path")).toBeInTheDocument();
    expect(screen.getByText("/Users/kc/.kittynest/projects/KittyNest/progress.md")).toBeInTheDocument();
    expect(screen.queryByText(/Review file:/i)).not.toBeInTheDocument();
  });

  it("renders active analyze jobs with progress counts and stop action", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue({
      ...state,
      jobs: [
        {
          id: 3,
          kind: "analyze_sessions",
          scope: "all_unprocessed",
          sessionId: null,
          projectSlug: null,
          taskSlug: null,
          updatedAfter: null,
          status: "running",
          total: 5,
          completed: 2,
          failed: 0,
          pending: 3,
          message: "Analyzed 2 of 5",
          startedAt: "2026-04-26T00:00:00Z",
          updatedAt: "2026-04-26T00:00:02Z",
          completedAt: null,
        },
      ],
    });
    const stop = vi.spyOn(api as ApiWithReviewQueue, "stopJob").mockResolvedValue({ stopped: true });

    render(<App />);

    expect(await screen.findByRole("heading", { name: "Analyze Jobs" })).toBeInTheDocument();
    expect(screen.queryByText("Tauri Jobs")).not.toBeInTheDocument();
    expect(await screen.findByText("analyze_sessions")).toBeInTheDocument();
    expect(screen.getByText("2 / 5")).toBeInTheDocument();
    expect(screen.getByText("3 pending")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /^stop$/i }));
    await waitFor(() => expect(stop).toHaveBeenCalledWith(3));
  });

  it("refreshes cached state while jobs run and when they finish without rescanning sources", async () => {
    const intervals: Array<() => void> = [];
    vi.spyOn(window, "setInterval").mockImplementation((handler, timeout) => {
      if (timeout === 2000) {
        intervals.push(handler as () => void);
      }
      return 1;
    });
    vi.spyOn(window, "clearInterval").mockImplementation(() => undefined);
    const runningState = {
      ...state,
      jobs: [
        {
          id: 4,
          kind: "analyze_sessions",
          scope: "all_unprocessed",
          sessionId: null,
          projectSlug: null,
          taskSlug: null,
          updatedAfter: null,
          status: "running",
          total: 1,
          completed: 0,
          failed: 0,
          pending: 1,
          message: "Analyzing sessions",
          startedAt: "2026-04-26T00:00:00Z",
          updatedAt: "2026-04-26T00:00:00Z",
          completedAt: null,
        },
      ],
    };
    const completedState = {
      ...state,
      sessions: [{ ...state.sessions[0], status: "analyzed", title: "Fresh summary" }],
      jobs: [],
    };
    const getState = vi.spyOn(api, "getAppState").mockResolvedValue(runningState);
    const getCachedState = vi
      .spyOn(api as ApiWithReviewQueue, "getCachedAppState")
      .mockResolvedValueOnce({
        ...runningState,
        sessions: [{ ...state.sessions[0], status: "analyzed", title: "Partial summary" }],
      })
      .mockResolvedValueOnce(completedState);

    render(<App />);
    await screen.findByText("analyze_sessions");
    await waitFor(() => expect(intervals).toHaveLength(1));

    await act(async () => {
      intervals[0]();
      await Promise.resolve();
      await Promise.resolve();
    });

    await waitFor(() => expect(getCachedState).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("Partial summary")).toBeInTheDocument();

    await act(async () => {
      intervals[0]();
      await Promise.resolve();
      await Promise.resolve();
    });

    await waitFor(() => expect(getCachedState).toHaveBeenCalledTimes(2));
    expect(getState).toHaveBeenCalledTimes(1);
    expect(await screen.findByText("Fresh summary")).toBeInTheDocument();
  });

  it("keeps all dashboard projects and sessions in scrollable panels", async () => {
    const projects = Array.from({ length: 6 }, (_, index) => ({
      ...state.projects[0],
      slug: `Project${index + 1}`,
      displayTitle: `Project ${index + 1}`,
      workdir: `/Users/kc/Project${index + 1}`,
    }));
    const sessions = Array.from({ length: 6 }, (_, index) => ({
      ...state.sessions[0],
      sessionId: `session-${index + 1}`,
      title: `Session ${index + 1}`,
      createdAt: `2026-04-26T0${index}:00:00Z`,
    }));
    vi.spyOn(api, "getAppState").mockResolvedValue({ ...state, projects, sessions });

    render(<App />);

    expect(await screen.findByText("Project 6")).toBeInTheDocument();
    expect(screen.getByText("Session 6")).toBeInTheDocument();
  });

  it("sorts recent sessions by updated time and shows compact age labels", async () => {
    vi.spyOn(Date, "now").mockReturnValue(Date.parse("2026-04-26T03:00:00Z"));
    vi.spyOn(api, "getAppState").mockResolvedValue({
      ...state,
      sessions: [
        {
          ...state.sessions[0],
          sessionId: "older",
          title: "Older Session",
          createdAt: "2026-04-26T02:59:00Z",
          updatedAt: "2026-04-24T03:00:00Z",
        },
        {
          ...state.sessions[0],
          sessionId: "newer",
          title: "Newer Session",
          createdAt: "2026-04-24T03:00:00Z",
          updatedAt: "2026-04-26T02:30:00Z",
        },
        {
          ...state.sessions[0],
          sessionId: "middle",
          title: "Middle Session",
          createdAt: "2026-04-25T03:00:00Z",
          updatedAt: "2026-04-26T01:00:00Z",
        },
      ],
    });

    render(<App />);

    const newer = await screen.findByText("Newer Session");
    const middle = screen.getByText("Middle Session");
    const older = screen.getByText("Older Session");
    expect(newer.compareDocumentPosition(middle)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    expect(middle.compareDocumentPosition(older)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    expect(screen.getByText("30m ago")).toBeInTheDocument();
    expect(screen.getByText("2h ago")).toBeInTheDocument();
    expect(screen.getByText("2d ago")).toBeInTheDocument();
  });

  it("renders table headers on project task and session list pages", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue(state);

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^projects$/i }));
    expect(screen.getByRole("columnheader", { name: "Name" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Path" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Status" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Source" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /^tasks$/i }));
    expect(screen.getByRole("columnheader", { name: "Name" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Project" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Status" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /^sessions$/i }));
    expect(screen.getByRole("columnheader", { name: "Name" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Path" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Project" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Task" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Source" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Status" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Updated" })).toBeInTheDocument();
  });

  it("renders session detail as summary path and markdown cards", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue(state);
    vi.spyOn(api, "readMarkdownFile").mockResolvedValue({ content: "# Session Notes\n\n- Imported" });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^sessions$/i }));
    await userEvent.click(screen.getByRole("button", { name: /import sessions/i }));

    expect(screen.queryByRole("heading", { name: "Source" })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Path" })).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Import Sessions" })).toBeInTheDocument();
    expect(screen.getByText("Original Path")).toBeInTheDocument();
    expect(screen.getByText("System Path")).toBeInTheDocument();
    expect(screen.getByText("/Users/kc/.codex/sessions/2026/04/26/abc.jsonl")).toBeInTheDocument();
    expect(screen.getByText("/Users/kc/.kittynest/projects/KittyNest/sessions/abc/summary.md")).toBeInTheDocument();
    expect(screen.queryByText("Session summary")).not.toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Session Notes" })).toBeInTheDocument();
    expect(await screen.findByText("Imported")).toBeInTheDocument();
  });

  it("renders task detail metadata and session summary cards", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue({
      ...state,
      sessions: [
        {
          ...state.sessions[0],
          sessionId: "later",
          title: "Later Session",
          summary: "Later summary",
          updatedAt: "2026-04-26T02:00:00Z",
        },
        {
          ...state.sessions[0],
          sessionId: "earlier",
          title: "Earlier Session",
          summary: "Earlier summary",
          updatedAt: "2026-04-26T01:00:00Z",
        },
      ],
    });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^tasks$/i }));
    await userEvent.click(screen.getByRole("button", { name: /session ingest/i }));

    expect(screen.getByRole("heading", { name: "Task Info" })).toBeInTheDocument();
    expect(screen.getByText("Path")).toBeInTheDocument();
    expect(screen.getByText("/Users/kc/.kittynest/projects/KittyNest/tasks/session-ingest")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "developing" })).toHaveClass("active");
    expect(screen.queryByRole("heading", { name: "Related Sessions" })).not.toBeInTheDocument();
    const cards = screen.getAllByRole("button", { name: /Open session/i });
    expect(cards[0]).toHaveTextContent("Earlier summary");
    expect(cards[1]).toHaveTextContent("Later summary");
  });

  it("deletes zero-session tasks from task detail", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue({
      ...state,
      tasks: [
        {
          ...state.tasks[0],
          status: "discussing",
          sessionCount: 0,
        },
      ],
      sessions: [],
    });
    const deleteTask = vi
      .spyOn(api as ApiWithReviewQueue, "deleteTask")
      .mockResolvedValue({ deleted: true });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^tasks$/i }));
    await userEvent.click(screen.getByRole("button", { name: /session ingest/i }));
    await userEvent.click(screen.getByRole("button", { name: /^delete$/i }));

    await waitFor(() => expect(deleteTask).toHaveBeenCalledWith("KittyNest", "session-ingest"));
  });

  it("renders inline markdown emphasis and code in project progress", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue({
      ...state,
      projects: [
        {
          ...state.projects[0],
          progressPath: "/Users/kc/.kittynest/projects/KittyNest/progress.md",
        },
      ],
    });
    vi.spyOn(api, "readMarkdownFile").mockResolvedValue({
      content: "# Progress\n\n- **Session Ingest** (`session-ingest`)",
    });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^projects$/i }));
    await userEvent.click(screen.getByRole("button", { name: /not reviewed/i }));

    await waitFor(() => {
      const inlineStrong = screen
        .getAllByText("Session Ingest", { selector: "strong" })
        .find((node) => node.closest(".markdown-body"));
      expect(inlineStrong).toBeTruthy();
    });
    expect(await screen.findByText("session-ingest", { selector: "code" })).toBeInTheDocument();
  });

  it("renders markdown tables in project progress", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue({
      ...state,
      projects: [
        {
          ...state.projects[0],
          progressPath: "/Users/kc/.kittynest/projects/KittyNest/progress.md",
        },
      ],
    });
    vi.spyOn(api, "readMarkdownFile").mockResolvedValue({
      content: "# Progress\n\n| Area | Status |\n| --- | --- |\n| API | Done |",
    });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^projects$/i }));
    await userEvent.click(screen.getByRole("button", { name: /not reviewed/i }));

    expect(await screen.findByRole("table")).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Area" })).toBeInTheDocument();
    expect(screen.getByRole("cell", { name: "API" })).toBeInTheDocument();
  });

  it("shows dashboard panel actions from the concept", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue(state);

    render(<App />);

    expect(await screen.findByRole("button", { name: /^scan$/i })).toBeInTheDocument();
    const sessionsNav = screen.getByRole("button", { name: /^sessions$/i });
    const tasksNav = screen.getByRole("button", { name: /^tasks$/i });
    expect(sessionsNav.compareDocumentPosition(tasksNav)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    const sessionsMetric = screen.getByText("Sessions", { selector: "small" }).closest(".metric");
    const tasksMetric = screen.getByText("Open Tasks", { selector: "small" }).closest(".metric");
    expect(sessionsMetric?.compareDocumentPosition(tasksMetric!)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    expect(screen.getByRole("button", { name: /\+ create project/i })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /view all/i })).toHaveLength(2);
    expect(screen.getByRole("button", { name: /scan new sessions/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^create task$/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^refresh$/i })).toBeInTheDocument();
  });

  it("shows reset controls in settings and calls backend reset commands", async () => {
    const getState = vi.spyOn(api, "getAppState").mockResolvedValue(state);
    const getCachedState = vi
      .spyOn(api as ApiWithReviewQueue, "getCachedAppState")
      .mockResolvedValue(state);
    const resetSessions = vi
      .spyOn(api as ApiWithReviewQueue, "resetSessions")
      .mockResolvedValue({ reset: 1 });
    const resetProjects = vi
      .spyOn(api as ApiWithReviewQueue, "resetProjects")
      .mockResolvedValue({ reset: 1 });
    const resetTasks = vi
      .spyOn(api as ApiWithReviewQueue, "resetTasks")
      .mockResolvedValue({ reset: 1 });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^settings$/i }));
    await userEvent.click(screen.getByRole("button", { name: /^reset sessions$/i }));
    await waitFor(() => expect(resetSessions).toHaveBeenCalledTimes(1));
    expect(await screen.findByText(/sessions reset: 1/i)).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /^reset projects$/i }));
    await waitFor(() => expect(resetProjects).toHaveBeenCalledTimes(1));
    expect(await screen.findByText(/projects reset: 1/i)).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /^reset tasks$/i }));
    await waitFor(() => expect(resetTasks).toHaveBeenCalledTimes(1));
    expect(await screen.findByText(/tasks reset: 1/i)).toBeInTheDocument();
    expect(getState).toHaveBeenCalledTimes(1);
    expect(getCachedState).toHaveBeenCalledTimes(3);
  });
});
