# Obsidian Sync 设计文档

## 概述

为 KittyNest 新增增量同步到 Obsidian vault 的功能。将项目中产生的 Projects、Sessions、Tasks、Memories 四类数据同步为 Obsidian 笔记，并通过 `[[wikilink]]` 引用构建 Obsidian 关联图谱。实体（Entity）作为 MOC（Map of Content）索引页，汇总引用该实体的所有 session。

## 架构

### 新增模块

```
src-tauri/src/sync/
  mod.rs           -- 模块入口，SyncManager
  obsidian.rs      -- Obsidian vault 自动检测
  renderer.rs      -- 将 SQLite 数据渲染为 Obsidian Markdown
  state.rs         -- sync_state 表管理（增量追踪）
```

### 数据流

```
[分析完成 / 手动触发]
        │
        ▼
  SyncManager::run(vault_path, mode)
        │
        ├── 1. 读取 SQLite（projects, sessions, tasks, memories, entities）
        │
        ├── 2. 对比 sync_state 表，过滤出需要同步的条目
        │
        ├── 3. renderer 渲染每条记录为 Obsidian Markdown
        │      - 注入 [[wikilink]] 引用
        │      - 生成实体 MOC 索引页
        │
        ├── 4. 写入 vault 的 KittyNest/ 目录
        │
        └── 5. 更新 sync_state 表
```

### 增量策略

新增 SQLite 表 `sync_state`（存储在 `kittynest.sqlite` 中）：

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | INTEGER | 自增主键 |
| `kind` | TEXT | `project` / `session` / `task` / `memory` / `entity` |
| `source_id` | TEXT | 对应记录的 slug 或 id |
| `content_hash` | TEXT | 内容的 SHA-256 hash |
| `last_synced_at` | TEXT | 上次同步时间 |
| `obsidian_path` | TEXT | 写入 vault 的相对路径 |

同步逻辑：
- 新记录（sync_state 中无）→ 创建文件
- 有记录但 hash 不同 → 更新文件
- 有记录但源已删除 → 删除 vault 中的文件（可配置）

### Obsidian Vault 自动检测

扫描策略（按优先级）：
1. 扫描 `~/Library/Mobile Documents/`（iCloud Drive）
2. 扫描 `~/Documents/`、`~/Desktop/`、`~/` 下包含 `.obsidian/` 文件夹的目录
3. 递归深度限制为 3 层，避免扫描过深
4. 如果发现多个 vault，在设置中列出让用户选择

检测结果缓存在 `config.toml` 中，下次启动直接使用，除非用户手动触发重新检测。

## 目录结构

```
<vault>/
  KittyNest/
    projects/
      <project-slug>/
        <project-slug>.md              ← 项目笔记
        sessions/
          <session-slug>.md            ← 会话笔记
        tasks/
          <task-slug>.md               ← 任务笔记
    memories/
      memory-<session-slug>.md         ← 按 session 聚合的 memory（加 memory- 前缀避免与 session 同名）
    entities/
      <entity-name>.md                 ← 实体 MOC 索引页
```

文件命名使用 slug（小写、连字符分隔），避免 Obsidian 中的路径问题。

## 笔记格式与引用关系

### 项目笔记

```markdown
---
tags: [kittynest/project]
workdir: /Users/yejiming/Desktop/kittlabs/KittyNest
sources: [claude, codex]
created_at: 2026-04-15
---

# KittyNest

项目概要内容...

## Sessions
![[2026-05-02-build-fix]]
![[2026-05-01-feature-x]]

## Tasks
![[add-obsidian-sync]]
```

用 `![[...]]` 嵌入（transclusion）sessions 和 tasks，在项目笔记中直接查看子内容。

### 会话笔记

```markdown
---
tags: [kittynest/session]
source: codex
session_id: 019de753-...
project: "[[kitty-nest]]"
created_at: 2026-05-02T06:40:45Z
---

# Build Fix Session

会话摘要内容...

## Memory
![[memory-2026-05-02-build-fix]]

## Related Entities
- [[rusqlite]]
- [[tauri]]
```

引用关系：
- frontmatter 的 `project` 字段用 `[[wikilink]]` → Obsidian 图谱识别
- 底部 Related Entities 列出从该 session 提取的实体 → 连接到 MOC 索引页

### 任务笔记

```markdown
---
tags: [kittynest/task]
status: developing
project: "[[kitty-nest]]"
created_at: 2026-05-01
---

# Add Obsidian Sync

任务描述内容...

## Related Sessions
- [[2026-05-02-build-fix]]
```

### Memory 笔记

```markdown
---
tags: [kittynest/memory]
session: "[[2026-05-02-build-fix]]"
project: "[[kitty-nest]]"
---

# Memory: 2026-05-02 Build Fix

- 项目构建命令是 `npm run build`，最终产物在 `dist/` 目录。
- 构建时遇到 Rollup native addon 签名冲突，通过调整 Node PATH 解决。

## Related Entities
- [[rusqlite]]
- [[tauri]]
```

### 实体 MOC 索引页

```markdown
---
tags: [kittynest/entity]
type: library
---

# rusqlite

## Sessions
- [[2026-05-02-build-fix]]
- [[2026-04-28-db-setup]]

## Memories
- [[memory-2026-05-02-build-fix]]
- [[memory-2026-04-28-db-setup]]
```

实体 MOC 页是纯索引——不产生自己的内容，汇总所有引用该实体的 session 和 memory。

### 图谱结构

```
Project ──contains──> Session ──extracts──> Memory
   │                      │
   │                      ├── references ──> Entity (MOC)
   │                      │
   └──contains──> Task ──references──> Session
```

在 Obsidian Graph View 中，Project、Session、Task、Memory、Entity 都是节点，边由 `[[wikilink]]` 自动建立。

## Job 集成

### 自动触发时机

- `analyze_session` 完成后 → 入队 sync job（同步该 session + 其 memory + 关联实体 MOC + 关联的 project 更新）
- `analyze_project` 完成后 → 入队 sync job（同步该 project + 其下的 sessions/tasks）
- `rebuild_memories` 完成后 → 入队 sync job（全量同步）

### 手动触发

- 设置页面新增 "Sync Now" 按钮 → 入队一个全量 sync job
- "Full Resync" 按钮 → 清空 sync_state 表，重新全量同步

## Tauri 命令

```rust
#[tauri::command]
fn detect_obsidian_vaults() -> Result<Vec<VaultInfo>, String>

#[tauri::command]
fn sync_to_obsidian(state: State<AppState>, mode: String) -> Result<SyncResult, String>
// mode: "incremental" | "full"

#[tauri::command]
fn get_sync_status(state: State<AppState>) -> Result<SyncStatus, String>
```

## 设置页面

在现有 Settings 页面新增 "Obsidian Sync" 区块：

```
┌─ Obsidian Sync ──────────────────────────────┐
│                                              │
│  Vault: [~/Documents/MyVault      ] [检测]   │
│         (自动检测到 2 个 vault，下拉选择)       │
│                                              │
│  状态: 上次同步 2 分钟前 (12 notes, 3 updated) │
│                                              │
│  [Sync Now]    [Full Resync]                 │
│                                              │
│  ☑ 自动同步（分析完成后）                       │
│  ☑ 同步时删除 vault 中已移除的笔记              │
└──────────────────────────────────────────────┘
```

## 配置持久化

在 `config.toml` 中新增 `[obsidian]` 段：

```toml
[obsidian]
vault_path = "/Users/yejiming/Documents/MyVault"
auto_sync = true
delete_removed = true
last_sync_at = "2026-05-02T07:00:00Z"
```

## 错误处理

- vault 路径不存在或 `.obsidian/` 目录丢失 → 跳过同步，设置页面显示警告
- 写入文件失败（权限问题）→ 记录到 error log，不阻塞后续同步
- hash 计算失败 → 视为需要同步，走全量写入
- 同步过程中有新 job 入队 → 排队等待，不并发写同一个 vault
