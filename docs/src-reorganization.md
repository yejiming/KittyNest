# src-tauri/src 目录重组方案

## 目标

- `src-tauri/src/` 最外层只保留 `main.rs`
- 其余文件按职责归入子文件夹
- 超过 500 行的大文件拆分为多个小文件

## 目标结构

```
src-tauri/src/
├── main.rs
├── lib.rs                     # 模块声明 + Tauri Builder
├── data/                      # 数据层
│   ├── mod.rs
│   ├── models.rs              # 所有数据结构（314行，不动）
│   ├── db/                    # 原 db.rs（3216行 → 拆 6 文件）
│   │   ├── mod.rs             # open(), migrate(), re-export
│   │   ├── schema.rs          # migrate() DDL + add_column_if_missing
│   │   ├── projects.rs        # project CRUD
│   │   ├── sessions.rs        # session CRUD
│   │   ├── jobs.rs            # job 队列
│   │   ├── tasks.rs           # task CRUD
│   │   └── memories.rs        # memory search + session memories
│   └── graph.rs               # 知识图谱（772行，不动）
├── llm/                       # LLM 调用层
│   ├── mod.rs
│   ├── client.rs              # request_json/markdown + 并发控制
│   ├── prompts.rs             # prompt 构建辅助函数
│   └── presets.rs             # 原 presets.rs
├── analysis/                  # 后台分析任务
│   ├── mod.rs
│   ├── jobs.rs                # run_next_analysis_job 主循环
│   ├── session.rs             # session 分析 + review + rebuild
│   ├── memory_search.rs       # 记忆搜索
│   ├── entity.rs              # 实体消歧
│   └── code_context.rs        # 代码上下文扫描
├── commands/                  # Tauri IPC 命令
│   ├── mod.rs                 # re-export + TauriAgentEmitter
│   ├── app_state.rs           # app state + scan_sources
│   ├── agent.rs               # agent run 相关命令
│   ├── jobs.rs                # enqueue_* / get_active_jobs / stop_job
│   └── tasks.rs               # task CRUD + reset_*
├── config/                    # 配置管理
│   ├── mod.rs
│   ├── settings.rs            # LLM 设置读写
│   └── workspace.rs           # 工作区初始化
├── scanner.rs                 # 会话文件扫描（342行，不动）
├── memory.rs                  # 会话记忆结构（130行，不动）
├── markdown.rs                # frontmatter 渲染（34行，不动）
├── utils.rs                   # 工具函数 + 原 errors.rs
└── services.rs                # AppServices 容器（14行，不动）
```

## 大文件拆分细节

### db.rs（3216行 → 6 文件）

| 新文件 | 来源函数 |
|--------|----------|
| schema.rs | `migrate()` DDL、`add_column_if_missing()` |
| projects.rs | `ensure_project_for_workdir`、`list_projects`、`get_project_by_slug`、`update_project_review/progress/agents`、`unique_project_slug` |
| sessions.rs | `upsert_raw_sessions`、`list_sessions`、`unprocessed_sessions*`、`mark_session_processed*`、`stored_session_from_row` |
| jobs.rs | `enqueue_*` 全系列、`claim_next_job`、`update_job_progress`、`complete/fail/cancel_job`、`list_active_jobs` |
| tasks.rs | `upsert_task`、`list_tasks`、`task_status_by_slug`、`update_task_status`、`delete_task_if_empty` |
| memories.rs | `replace_session_memories*`、`session_memories_*`、`create_memory_search`、`memory_search_*` |

### analysis.rs（3652行 → 5 文件）

| 新文件 | 来源函数 |
|--------|----------|
| jobs.rs | `run_next_analysis_job`、`import_historical_sessions`、job 分发逻辑 |
| session.rs | `analyze_session`、`store_session_analysis`、`review_project`、`create_manual_task`、`rebuild_memories`、`rebuild_session_memory` |
| memory_search.rs | `run_memory_search_job`、`memory_search_entities`、`extract_memory_search_entities` |
| entity.rs | `disambiguate_memory_entities`、`remote_entity_alias_groups`、`entity_alias_groups_from_json` |
| code_context.rs | `code_context()`、`is_source_excerpt_candidate()` |

### commands.rs（1524行 → 4 文件）

| 新文件 | 来源函数 |
|--------|----------|
| app_state.rs | `get_app_state`、`get_cached_app_state`、`scan_sources`、`app_state_from_db`、`scan_sources_into_db` |
| agent.rs | `start/stop/clear/resolve/save/load_agent_run`、`run_save_agent_session_job_with_metadata` |
| jobs.rs | `enqueue_*` 全系列、`get_active_jobs`、`stop_job` |
| tasks.rs | `create_task`、`delete_task`、`update_task_status`、`reset_*`、`rebuild_memories` |

### llm.rs（657行 → 2 文件）

| 新文件 | 来源函数 |
|--------|----------|
| client.rs | `request_json`、`request_markdown`、openai/anthropic 实现、`acquire_llm_permit` 并发控制 |
| prompts.rs | 从 analysis.rs 抽出的 prompt 辅助：`session_transcript`、`strip_llm_think_blocks`、`format_*` |

## 合并

- `errors.rs`（5行）→ 合入 `utils.rs`
- `presets.rs` → 移入 `llm/presets.rs`

## 不动的文件

| 文件 | 行数 | 理由 |
|------|------|------|
| main.rs | 3 | 入口 |
| scanner.rs | 342 | 职责单一 |
| memory.rs | 130 | 职责单一 |
| markdown.rs | 34 | 职责单一 |
| services.rs | 14 | 职责单一 |
| graph.rs | 772 | 逻辑紧密，拆开增加认知负担 |
