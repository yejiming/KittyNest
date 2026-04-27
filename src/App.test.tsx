import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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
  enqueueRebuildMemories: () => Promise<{ jobId: number; total: number }>;
  enqueueScanSources: () => Promise<{ jobId: number; total: number }>;
  enqueueSearchMemories: (query: string) => Promise<{ jobId: number; total: number }>;
  getCachedAppState: () => Promise<AppState>;
  getMemorySearch: () => Promise<import("./types").MemorySearchRecord | null>;
  getSessionMemory: (sessionId: string) => Promise<import("./types").SessionMemoryDetail>;
  listMemoryEntities: () => Promise<import("./types").MemoryEntityRecord[]>;
  listEntitySessions: (entity: string) => Promise<import("./types").MemoryRelatedSession[]>;
  resetMemories: () => Promise<{ reset: number }>;
  resetProjects: () => Promise<{ reset: number }>;
  resetSessions: () => Promise<{ reset: number }>;
  resetTasks: () => Promise<{ reset: number }>;
  stopJob: (jobId: number) => Promise<{ stopped: boolean }>;
};

const state: AppState = {
  dataDir: "/Users/kc/.kittynest",
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
      taskModel: "",
    },
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
      userPreferencePath: null,
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
    expect(screen.getByText("Import Ses...")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /scan new sessions/i }));

    await waitFor(() => expect(enqueueScan).toHaveBeenCalledTimes(1));
    expect(scan).not.toHaveBeenCalled();
    expect(await screen.findByText(/scan queued/i)).toBeInTheDocument();
  });

  it("truncates dashboard project and session labels to ten characters", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue({
      ...state,
      projects: [{ ...state.projects[0], displayTitle: "ABCDEFGHIJKL" }],
      sessions: [{ ...state.sessions[0], title: "MNOPQRSTUVWX" }],
    });

    render(<App />);

    await screen.findByRole("heading", { name: "Dashboard" });

    expect(document.querySelector(".projects-panel .project-row strong")).toHaveTextContent("ABCDEFGHIJ...");
    expect(document.querySelector(".sessions-panel .session-row:not(.session-row-head) span")).toHaveTextContent(
      "MNOPQRSTUV...",
    );
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

  it("shows memory projects and expands sessions with memory", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue({
      ...state,
      sessions: [
        {
          ...state.sessions[0],
          source: "Claude Code",
          title: "Implement memory module",
          status: "analyzed",
        },
      ],
    });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^sessions$/i }));
    expect(screen.queryByRole("heading", { name: "Memories" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /KittyNest/i }));

    expect(screen.getByText("Implement memory module")).toBeInTheDocument();
    expect(screen.getByText("Claude Code")).toBeInTheDocument();
  });

  it("queues session list analysis for pending and failed sessions with a selectable updated range", async () => {
    vi.spyOn(Date, "now").mockReturnValue(Date.parse("2026-04-27T12:00:00Z"));
    vi.spyOn(api, "getAppState").mockResolvedValue({
      ...state,
      sessions: [
        { ...state.sessions[0], sessionId: "pending", status: "pending", title: "Pending Session" },
        { ...state.sessions[0], sessionId: "failed", status: "failed", title: "Failed Session" },
        { ...state.sessions[0], sessionId: "analyzed", status: "analyzed", title: "Analyzed Session" },
      ],
    });
    const enqueue = vi.spyOn(api, "enqueueAnalyzeSessions").mockResolvedValue({ jobId: 88, total: 2 });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^sessions$/i }));
    expect(screen.getByRole("combobox", { name: /analyze range/i })).toHaveValue("7days");

    await userEvent.click(screen.getByRole("button", { name: /^analyze$/i }));
    await waitFor(() => expect(enqueue).toHaveBeenCalledWith("2026-04-20T12:00:00.000Z"));

    await userEvent.selectOptions(screen.getByRole("combobox", { name: /analyze range/i }), "All");
    await userEvent.click(screen.getByRole("button", { name: /^analyze$/i }));

    await waitFor(() => expect(enqueue).toHaveBeenLastCalledWith(undefined));
    expect(await screen.findByText(/session analysis queued: 2 sessions/i)).toBeInTheDocument();
  });

  it("enqueues one session from session detail", async () => {
    const getState = vi.spyOn(api, "getAppState").mockResolvedValue(state);
    const enqueue = vi
      .spyOn(api, "enqueueAnalyzeSession")
      .mockResolvedValue({ jobId: 9, total: 1 });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^sessions$/i }));
    await userEvent.click(screen.getByRole("button", { name: /KittyNest/i }));
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
    expect(screen.queryByRole("heading", { name: "Memories" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /KittyNest/i }));
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
          userPreferencePath: "/Users/kc/.kittynest/projects/KittyNest/user_preference.md",
        },
      ],
    });
    vi.spyOn(api, "readMarkdownFile")
      .mockResolvedValueOnce({ content: "# Summary\n\n- Rust + Tauri\n\n[Docs](https://example.com)" })
      .mockResolvedValueOnce({ content: "# Progress\n\n- Done item" })
      .mockResolvedValueOnce({ content: "# User Preference\n\n- Prefers concise answers" });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^projects$/i }));
    await userEvent.click(screen.getByRole("button", { name: /not reviewed/i }));

    expect(await screen.findByRole("heading", { name: "Summary" })).toBeInTheDocument();
    expect(await screen.findByText("Rust + Tauri")).toBeInTheDocument();
    expect(await screen.findByRole("link", { name: "Docs" })).toHaveAttribute("href", "https://example.com");
    expect(await screen.findByRole("heading", { name: "Progress", level: 1 })).toBeInTheDocument();
    expect(await screen.findByText("Done item")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "User Preference", level: 1 })).toBeInTheDocument();
    expect(await screen.findByText("Prefers concise answers")).toBeInTheDocument();
    expect(screen.getByText("Project Path")).toBeInTheDocument();
    expect(screen.getByText("/Users/kc/KittyNest")).toBeInTheDocument();
    expect(screen.getByText("Project Summary Path")).toBeInTheDocument();
    expect(screen.getByText("/Users/kc/.kittynest/projects/KittyNest/summary.md")).toBeInTheDocument();
    expect(screen.getByText("Project Progress Path")).toBeInTheDocument();
    expect(screen.getByText("/Users/kc/.kittynest/projects/KittyNest/progress.md")).toBeInTheDocument();
    expect(screen.getByText("User Preference Path")).toBeInTheDocument();
    expect(screen.getByText("/Users/kc/.kittynest/projects/KittyNest/user_preference.md")).toBeInTheDocument();
    expect(screen.queryByText(/Review file:/i)).not.toBeInTheDocument();
  });

  it("uses the shared markdown panel layout", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue({
      ...state,
      projects: [
        {
          ...state.projects[0],
          infoPath: "/Users/kc/.kittynest/projects/KittyNest/summary.md",
          progressPath: "/Users/kc/.kittynest/projects/KittyNest/progress.md",
          userPreferencePath: "/Users/kc/.kittynest/projects/KittyNest/user_preference.md",
        },
      ],
    });
    vi.spyOn(api, "readMarkdownFile")
      .mockResolvedValueOnce({ content: "# Summary\n\n- First item" })
      .mockResolvedValueOnce({ content: "# Progress\n\n- Second item" })
      .mockResolvedValueOnce({ content: "# User Preference\n\n- Third item" });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^projects$/i }));
    await userEvent.click(screen.getByRole("button", { name: /not reviewed/i }));

    const markdownScrolls = document.querySelectorAll(".markdown-scroll");
    expect(markdownScrolls).toHaveLength(3);
    markdownScrolls.forEach((node) => {
      expect(node.classList.contains("markdown-scroll-hidden")).toBe(false);
    });
    expect(document.querySelectorAll(".markdown-body")).toHaveLength(3);
  });

  it("copies raw markdown from markdown panels", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue({
      ...state,
      projects: [
        {
          ...state.projects[0],
          infoPath: "/Users/kc/.kittynest/projects/KittyNest/summary.md",
        },
      ],
    });
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    vi.spyOn(api, "readMarkdownFile").mockImplementation(async (path) => ({
      content: path.endsWith("/summary.md")
        ? "# Summary\n\n- **Raw** markdown"
        : "# Progress\n\n- Other markdown",
    }));

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^projects$/i }));
    await userEvent.click(screen.getByRole("button", { name: /not reviewed/i }));
    await screen.findByText("Raw", { selector: "strong" });
    await userEvent.click(screen.getByRole("button", { name: /copy project summary markdown/i }));

    expect(writeText).toHaveBeenCalledWith("# Summary\n\n- **Raw** markdown");
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
      sessions: [{ ...state.sessions[0], status: "analyzed", title: "Fresh" }],
      jobs: [],
    };
    const getState = vi.spyOn(api, "getAppState").mockResolvedValue(runningState);
    const getCachedState = vi
      .spyOn(api as ApiWithReviewQueue, "getCachedAppState")
      .mockResolvedValueOnce({
        ...runningState,
        sessions: [{ ...state.sessions[0], status: "analyzed", title: "Partial" }],
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
    expect(await screen.findByText("Partial")).toBeInTheDocument();

    await act(async () => {
      intervals[0]();
      await Promise.resolve();
      await Promise.resolve();
    });

    await waitFor(() => expect(getCachedState).toHaveBeenCalledTimes(2));
    expect(getState).toHaveBeenCalledTimes(1);
    expect(await screen.findByText("Fresh")).toBeInTheDocument();
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
          title: "Older",
          createdAt: "2026-04-26T02:59:00Z",
          updatedAt: "2026-04-24T03:00:00Z",
        },
        {
          ...state.sessions[0],
          sessionId: "newer",
          title: "Newer",
          createdAt: "2026-04-24T03:00:00Z",
          updatedAt: "2026-04-26T02:30:00Z",
        },
        {
          ...state.sessions[0],
          sessionId: "middle",
          title: "Middle",
          createdAt: "2026-04-25T03:00:00Z",
          updatedAt: "2026-04-26T01:00:00Z",
        },
      ],
    });

    render(<App />);

    const newer = await screen.findByText("Newer");
    const middle = screen.getByText("Middle");
    const older = screen.getByText("Older");
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
    await userEvent.click(screen.getByRole("button", { name: /KittyNest/i }));
    expect(screen.getAllByRole("columnheader", { name: "Name" }).length).toBeGreaterThan(0);
    expect(screen.getAllByRole("columnheader", { name: "Path" }).length).toBeGreaterThan(0);
    expect(screen.queryByRole("columnheader", { name: "Project" })).not.toBeInTheDocument();
    expect(screen.queryByRole("columnheader", { name: "Task" })).not.toBeInTheDocument();
    expect(screen.getAllByRole("columnheader", { name: "Source" }).length).toBeGreaterThan(0);
    expect(screen.getAllByRole("columnheader", { name: "Status" }).length).toBeGreaterThan(0);
    expect(screen.queryByRole("columnheader", { name: "Updated" })).not.toBeInTheDocument();
  });

  it("renders session detail as summary path and markdown cards", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue(state);
    vi.spyOn(api, "readMarkdownFile").mockResolvedValue({ content: "# Session Notes\n\n- Imported" });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^sessions$/i }));
    await userEvent.click(screen.getByRole("button", { name: /KittyNest/i }));
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

    const markdownScroll = document.querySelector(".markdown-scroll");
    expect(markdownScroll).not.toBeNull();
    if (markdownScroll) {
      expect(markdownScroll.classList.contains("markdown-scroll-hidden")).toBe(false);
    }

    const markdownBody = document.querySelector(".markdown-body");
    expect(markdownBody).not.toBeNull();
    expect(document.querySelectorAll(".markdown-body")).toHaveLength(1);
  });

  it("shows session memory path, graph, and markdown memory cards", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue({
      ...state,
      sessions: [
        {
          ...state.sessions[0],
          sessionId: "session-1",
          title: "Implement memory module",
          status: "analyzed",
        },
        {
          ...state.sessions[0],
          sessionId: "session-2",
          title: "Related Session",
          status: "analyzed",
        },
      ],
    });
    const getSessionMemory = vi.spyOn(api as ApiWithReviewQueue, "getSessionMemory").mockImplementation(async (sessionId) => ({
      sessionId,
      memoryPath: `/Users/kc/.kittynest/memories/sessions/${sessionId}/memory.md`,
      memories: sessionId === "session-2" ? ["Related memory."] : ["**SQLite** stores local memory.", "User prefers short facts."],
      relatedSessions: sessionId === "session-2" ? [] : [
        {
          sessionId: "session-2",
          title: "Related Session",
          projectSlug: "KittyNest",
          sharedEntities: ["sqlite"],
        },
      ],
    }));

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^sessions$/i }));
    await userEvent.click(screen.getByRole("button", { name: /KittyNest/i }));
    await userEvent.click(screen.getByRole("button", { name: /Implement memory module/i }));

    expect(await screen.findByText("Memory Path")).toBeInTheDocument();
    expect(screen.getByText("/Users/kc/.kittynest/memories/sessions/session-1/memory.md")).toBeInTheDocument();
    expect(screen.getByText("Related Session")).toBeInTheDocument();
    expect(screen.getByText("SQLite")).toBeInTheDocument();
    expect(screen.queryByText("Current")).not.toBeInTheDocument();
    expect(document.querySelector(".react-flow")).not.toBeNull();
    expect(document.querySelector(".react-flow__minimap")).toBeNull();
    expect(screen.getByLabelText("Related memory graph")).toHaveAttribute("data-edge-count", "2");

    fireEvent.click(screen.getByText("Related Session"));

    await waitFor(() => expect(getSessionMemory).toHaveBeenLastCalledWith("session-2"));
    expect(await screen.findByRole("heading", { name: "Related Session" })).toBeInTheDocument();
    expect(screen.getByText("/Users/kc/.kittynest/memories/sessions/session-2/memory.md")).toBeInTheDocument();
  });

  it("queues memory search and renders latest results", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue(state);
    vi.spyOn(api as ApiWithReviewQueue, "enqueueSearchMemories").mockResolvedValue({ jobId: 77, total: 1 });
    vi.spyOn(api as ApiWithReviewQueue, "getMemorySearch").mockResolvedValue({
      id: 1,
      jobId: 77,
      query: "sqlite",
      status: "completed",
      message: "1 memory found",
      createdAt: "2026-04-27T00:00:00Z",
      updatedAt: "2026-04-27T00:00:01Z",
      results: [
        {
          sourceSession: "session-1",
          sessionTitle: "Implement memory module",
          projectSlug: "KittyNest",
          memory: "SQLite stores local graph memory.",
          ordinal: 0,
        },
      ],
    });

    render(<App />);
    await screen.findByRole("heading", { name: "Dashboard" });
    await userEvent.click(screen.getByRole("button", { name: "Memory" }));
    await userEvent.type(screen.getByPlaceholderText("Search memory by topic or entity"), "sqlite");
    await userEvent.click(screen.getByRole("button", { name: "Send memory search" }));

    expect(api.enqueueSearchMemories).toHaveBeenCalledWith("sqlite");
    expect(await screen.findByText("Implement memory module")).toBeInTheDocument();
    expect(screen.getByText("SQLite stores local graph memory.")).toBeInTheDocument();
  });

  it("expands entity sessions and opens session detail", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue(state);
    vi.spyOn(api as ApiWithReviewQueue, "listMemoryEntities").mockResolvedValue([
      { entity: "sqlite", canonicalName: "SQLite", entityType: "technology", sessionCount: 2, createdAt: "2026-04-27T00:00:00Z" } as any,
    ]);
    const listEntitySessions = vi.spyOn(api as ApiWithReviewQueue, "listEntitySessions").mockResolvedValue([
      { sessionId: "abc", title: "Import Sessions", projectSlug: "KittyNest", sharedEntities: ["sqlite"] },
    ]);

    render(<App />);
    await screen.findByRole("heading", { name: "Dashboard" });
    await userEvent.click(screen.getByRole("button", { name: "Memory" }));
    await userEvent.click(await screen.findByRole("button", { name: /SQLite/i }));
    expect(screen.queryByRole("columnheader", { name: "Name" })).not.toBeInTheDocument();
    expect(listEntitySessions).toHaveBeenCalledWith("SQLite");
    await userEvent.click(await screen.findByText("Import Sessions"));

    expect(await screen.findByText("Memory Path")).toBeInTheDocument();
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

  it("renders task prompt markdown from sibling files without task summary card", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue({
      ...state,
      tasks: [
        {
          ...state.tasks[0],
          summaryPath: "/Users/kc/.kittynest/projects/KittyNest/tasks/session-ingest/llm_prompt.md",
        },
      ],
    });
    const readMarkdownFile = vi.spyOn(api, "readMarkdownFile").mockImplementation(async (path) => {
      if (path.endsWith("/user_prompt.md")) {
        return { content: "# User Prompt\n\nBuild **memory** refresh." };
      }
      if (path.endsWith("/llm_prompt.md")) {
        return { content: "# LLM Prompt\n\nUse `canonical_name`." };
      }
      return { content: "" };
    });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^tasks$/i }));
    await userEvent.click(screen.getByRole("button", { name: /session ingest/i }));

    expect(await screen.findByRole("heading", { name: "User Prompt", level: 1 })).toBeInTheDocument();
    expect(await screen.findByText("memory", { selector: "strong" })).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "LLM Prompt", level: 1 })).toBeInTheDocument();
    expect((await screen.findAllByText("canonical_name", { selector: "code" })).length).toBeGreaterThan(0);
    expect(readMarkdownFile).toHaveBeenCalledWith("/Users/kc/.kittynest/projects/KittyNest/tasks/session-ingest/user_prompt.md");
    expect(readMarkdownFile).toHaveBeenCalledWith("/Users/kc/.kittynest/projects/KittyNest/tasks/session-ingest/llm_prompt.md");
    expect(screen.queryByRole("heading", { name: "Task Summary" })).not.toBeInTheDocument();
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

  it("queues memory refresh through analyze jobs", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue(state);
    const enqueueRebuildMemories = vi
      .spyOn(api as ApiWithReviewQueue, "enqueueRebuildMemories")
      .mockResolvedValue({ jobId: 42, total: 2 });
    vi.spyOn(api as ApiWithReviewQueue, "getActiveJobs").mockResolvedValue([
      {
        id: 42,
        kind: "rebuild_memories",
        scope: "memory_rebuild",
        sessionId: null,
        projectSlug: null,
        taskSlug: null,
        updatedAfter: null,
        status: "queued",
        total: 2,
        completed: 0,
        failed: 0,
        pending: 2,
        message: "Queued for analysis",
        startedAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        completedAt: null,
      },
    ]);

    render(<App />);

    expect(await screen.findByRole("heading", { name: "Dashboard" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^scan$/i })).toBeInTheDocument();
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
    await userEvent.click(screen.getByRole("button", { name: /^refresh$/i }));
    await waitFor(() => expect(enqueueRebuildMemories).toHaveBeenCalledTimes(1));
    expect(await screen.findByText(/memory refresh queued: 2 steps/i)).toBeInTheDocument();
    expect(await screen.findByText("rebuild_memories")).toBeInTheDocument();
  });

  it("queues entity disambiguation when no sessions need memory rebuild", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue(state);
    const enqueueRebuildMemories = vi
      .spyOn(api as ApiWithReviewQueue, "enqueueRebuildMemories")
      .mockResolvedValue({ jobId: 43, total: 1 });
    vi.spyOn(api as ApiWithReviewQueue, "getActiveJobs").mockResolvedValue([
      {
        id: 43,
        kind: "rebuild_memories",
        scope: "memory_rebuild",
        sessionId: null,
        projectSlug: null,
        taskSlug: null,
        updatedAfter: null,
        status: "queued",
        total: 1,
        completed: 0,
        failed: 0,
        pending: 1,
        message: "Queued for analysis",
        startedAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        completedAt: null,
      },
    ]);

    render(<App />);

    expect(await screen.findByRole("heading", { name: "Dashboard" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /^refresh$/i }));

    await waitFor(() => expect(enqueueRebuildMemories).toHaveBeenCalledTimes(1));
    expect(await screen.findByText(/memory refresh queued: 1 step/i)).toBeInTheDocument();
    expect(await screen.findByText("rebuild_memories")).toBeInTheDocument();
    expect(screen.getByText("0 / 1")).toBeInTheDocument();
    expect(screen.queryByText("0 / 0")).not.toBeInTheDocument();
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
    const resetMemories = vi
      .spyOn(api as ApiWithReviewQueue, "resetMemories")
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

    await userEvent.click(screen.getByRole("button", { name: /^reset memories$/i }));
    await waitFor(() => expect(resetMemories).toHaveBeenCalledTimes(1));
    expect(await screen.findByText(/memories reset: 1/i)).toBeInTheDocument();
    expect(getState).toHaveBeenCalledTimes(1);
    expect(getCachedState).toHaveBeenCalledTimes(4);
  });

  it("saves LLM settings without rescanning session sources", async () => {
    const getState = vi.spyOn(api, "getAppState").mockResolvedValue(state);
    const getCachedState = vi
      .spyOn(api as ApiWithReviewQueue, "getCachedAppState")
      .mockResolvedValue(state);
    const saveLlmSettings = vi
      .spyOn(api, "saveLlmSettings")
      .mockResolvedValue({ saved: true });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^settings$/i }));
    await userEvent.click(screen.getByRole("button", { name: /^save model$/i }));

    await waitFor(() => expect(saveLlmSettings).toHaveBeenCalledTimes(1));
    expect(await screen.findByText(/llm settings saved/i)).toBeInTheDocument();
    expect(getState).toHaveBeenCalledTimes(1);
    expect(getCachedState).toHaveBeenCalledTimes(1);
  });

  it("manages saved LLM models, global limits, and scenario model choices", async () => {
    const settingsState = {
      ...state,
      llmSettings: {
        ...state.llmSettings,
        id: "openrouter-fast",
        remark: "Fast",
        model: "openai/gpt-4o-mini",
        apiKey: "sk-openrouter",
        maxContext: 64000,
        maxTokens: 2048,
        temperature: 0.3,
        models: [
          {
            id: "openrouter-fast",
            provider: "OpenRouter",
            remark: "Fast",
            baseUrl: "https://openrouter.ai/api/v1",
            interface: "openai",
            model: "openai/gpt-4o-mini",
            apiKey: "sk-openrouter",
          },
          {
            id: "anthropic-deep",
            provider: "Anthropic",
            remark: "Deep",
            baseUrl: "https://api.anthropic.com",
            interface: "anthropic",
            model: "claude-3-5-sonnet-latest",
            apiKey: "sk-anthropic",
          },
        ],
        scenarioModels: {
          defaultModel: "openrouter-fast",
          projectModel: "anthropic-deep",
          sessionModel: "",
          memoryModel: "",
          taskModel: "",
        },
      },
      providerPresets: [
        ...state.providerPresets,
        {
          provider: "Anthropic",
          baseUrl: "https://api.anthropic.com",
          interface: "anthropic",
        },
      ],
    } as AppState;
    vi.spyOn(api, "getAppState").mockResolvedValue(settingsState);
    vi.spyOn(api as ApiWithReviewQueue, "getCachedAppState").mockResolvedValue(settingsState);
    const saveLlmSettings = vi.spyOn(api, "saveLlmSettings").mockResolvedValue({ saved: true });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: /^settings$/i }));
    expect(screen.queryByRole("heading", { name: /session sources/i })).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "LLM Global Settings" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Anthropic Deep" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Anthropic Deep" }));
    expect(screen.getByLabelText(/^Provider$/i)).toHaveValue("Anthropic");
    expect(screen.getByLabelText(/^Remark$/i)).toHaveValue("Deep");
    expect(screen.getByLabelText(/^Remark$/i)).toBeRequired();

    await userEvent.clear(screen.getByLabelText(/max tokens/i));
    await userEvent.type(screen.getByLabelText(/max tokens/i), "4096");
    await userEvent.clear(screen.getByLabelText(/temperature/i));
    await userEvent.type(screen.getByLabelText(/temperature/i), "0.45");
    await userEvent.selectOptions(screen.getByLabelText(/session model/i), "openrouter-fast");
    await userEvent.click(screen.getByRole("button", { name: /^save model$/i }));

    await waitFor(() => expect(saveLlmSettings).toHaveBeenCalledTimes(1));
    expect(saveLlmSettings).toHaveBeenCalledWith(expect.objectContaining({
      maxContext: 64000,
      maxTokens: 4096,
      temperature: 0.45,
      scenarioModels: {
        defaultModel: "openrouter-fast",
        projectModel: "anthropic-deep",
        sessionModel: "openrouter-fast",
        memoryModel: "",
        taskModel: "",
      },
      models: expect.arrayContaining([
        expect.objectContaining({
          id: "anthropic-deep",
          provider: "Anthropic",
          remark: "Deep",
          interface: "anthropic",
          model: "claude-3-5-sonnet-latest",
        }),
      ]),
    }));
  });
});
