# KittyNest Memory Module 技术设计报告

> 版本：v1.0  
> 日期：2026-04-26  
> 状态：设计冻结，待实现

---

## 1. 概述

Memory Module 是 KittyNest 的核心知识沉淀层，负责将 LLM 分析产生的 Session 摘要转化为可长期维护的结构化记忆。设计目标是在**本地优先**的前提下，构建一个从 Session → 项目 → 系统的三级级联记忆体系，同时维护一张跨 Session 的实体关系图，以支持未来基于关联的检索和推理。

---

## 2. 核心设计决策

| # | 决策项 | 方案 | 理由 |
|---|--------|------|------|
| 1 | Session 级记忆生成时机 | **Session 分析完成后自动触发** | 记忆是分析的副产物，无需用户干预，保证数据完整性 |
| 2 | 项目级记忆更新策略 | **自动增量更新** | Session 分析完成后，立即将新 Session 记忆合并进现有项目记忆，避免全量重算 |
| 3 | 系统级记忆更新策略 | **UI 手动触发** | 系统级记忆涉及跨项目聚合，Token 成本高，由用户在 Memory 页面主动触发 |
| 4 | 图数据库方案 | **CozoDB + SQLite 后端** | CozoDB 原生支持 Datalog 查询和图遍历，SQLite 后端与现有索引层统一，无额外运维负担 |
| 5 | 实体关系抽取时机 | **与 Session 记忆同步生成** | 复用同一次 LLM 上下文，降低延迟和费用，原子性写入 |

---

## 3. 架构设计

### 3.1 三级级联记忆

```
┌─────────────────────────────────────────────────────────────┐
│  Layer 3: System Memory                                     │
│  ~/.kittynest/memories/system/memory.md                     │
│  跨项目通用约束、用户偏好、全局技术栈规律                      │
│  触发方式: UI 手动 (Memory 页 "Refresh System Memory")       │
└──────────────────────┬──────────────────────────────────────┘
                       │ 读取所有 Project Memory
┌──────────────────────▼──────────────────────────────────────┐
│  Layer 2: Project Memory                                    │
│  ~/.kittynest/memories/projects/<project_name>.md           │
│  项目级长期事实：偏好、约束、决策、已知问题、技术栈事实         │
│  触发方式: 自动增量 (每次 Session Memory 生成后合并)          │
└──────────────────────┬──────────────────────────────────────┘
                       │ 读取单个 Session Memory
┌──────────────────────▼──────────────────────────────────────┐
│  Layer 1: Session Memory                                    │
│  ~/.kittynest/memories/sessions/<session_slug>/memory.md    │
│  原子事实：从单次 Session 提取的结构化知识                     │
│  触发方式: 自动 (Session 分析完成后立即生成)                  │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 实体关系图（Graph Layer）

```
┌─────────────────────────────────────────────────────────────┐
│  Graph Layer: CozoDB (SQLite-backed)                        │
│  ~/.kittynest/kittynest_graph.db                            │
│                                                              │
│  节点: Entity (技术、模块、API、人、概念等)                   │
│  边: Relation (depends_on, replaced_by, used_in, etc.)      │
│  来源: 每次 Session Memory 生成时同步抽取                    │
└─────────────────────────────────────────────────────────────┘
```

---

## 4. 数据流与工作流

### 4.1 Session 分析完成后的自动链路

```
Session 分析完成 (analysis.rs)
    │
    ├─→ 1. 生成 Session Memory (memory.rs)
    │      输入: session.summary, session.title, transcript
    │      输出: ~/.kittynest/memories/sessions/<slug>/memory.md
    │      LLM Prompt: 抽取原子事实，输出 Markdown + 实体关系 JSON
    │
    ├─→ 2. 写入实体关系图 (graph.rs)
    │      输入: LLM 返回的 entities + relations JSON
    │      输出: CozoDB 节点/边写入
    │
    └─→ 3. 增量更新项目记忆 (memory.rs)
           输入: 新 Session Memory + 现有 Project Memory
           输出: ~/.kittynest/memories/projects/<name>.md
           LLM Prompt: 增量合并（去重、消解冲突、追加新事实）
```

### 4.2 系统级记忆手动链路

```
用户点击 Memory 页 "Refresh System Memory"
    │
    └─→ 读取所有 ~/.kittynest/memories/projects/*.md
        输入: 全部项目级记忆
        输出: ~/.kittynest/memories/system/memory.md
        LLM Prompt: 跨项目聚合通用约束与偏好
```

---

## 5. 存储方案详解

### 5.1 文件层（Markdown Store）

| 层级 | 路径 | 内容格式 |
|------|------|---------|
| Session | `~/.kittynest/memories/sessions/<session_slug>/memory.md` | Frontmatter + 原子事实列表 |
| Project | `~/.kittynest/memories/projects/<project_name>.md` | Frontmatter + 主题分类记忆 |
| System | `~/.kittynest/memories/system/memory.md` | Frontmatter + 全局分类记忆 |

**Markdown 格式约定**：
- 所有记忆文件使用 frontmatter 记录元数据（`updated_at`, `source`, `version`）
- 正文使用二级标题分类（`## Preferences`, `## Constraints`, `## Decisions`, `## Known Issues`, `## Tech Stack Facts`）
- Session 记忆正文使用列表形式的原子事实，便于上层 LLM 快速扫描

### 5.2 图数据库层（CozoDB + SQLite）

**选型理由**：
- **CozoDB**：原生支持递归查询和 Datalog，适合知识图谱的复杂关联检索；单机嵌入式运行，无需独立服务进程。
- **SQLite 后端**：与现有 `kittynest.sqlite` 索引层保持一致，备份和迁移简单。

**核心 Schema（CozoScript）**：

```cozo
# 实体节点
:create entity { id: Int => name: String, type: String, source_session: String, source_project: String, first_seen: String }

# 关系边
:create relation { id: Int => subject: Int, predicate: String, object: Int, source_session: String, confidence: Float }

# 实体别名（用于消歧）
:create entity_alias { name: String => canonical_id: Int }
```

**查询示例**：
```cozo
# 查找与 "Tauri" 有直接关系的所有实体
?[related_entity, predicate] :=
    *entity{id: tauri_id, name: "Tauri"},
    *relation{subject: tauri_id, predicate, object: related_id},
    *entity{id: related_id, name: related_entity}
```

---

## 6. LLM 输出契约

### 6.1 Session Memory LLM 输出格式

要求 LLM 在一次调用中返回两部分内容：

**Part A: 记忆文本（Markdown）**
```markdown
---
source: codex
session_id: abc-123
project: KittyNest
generated_at: 2026-04-26T12:00:00Z
---

## Facts
- 用户决定使用 CozoDB 作为图数据库方案。
- 项目级记忆采用自动增量更新策略。
- Session 记忆在分析完成后自动生成。

## Entities Mentioned
- CozoDB (technology)
- SQLite (technology)
- KittyNest (project)
```

**Part B: 结构化实体关系（JSON）**
```json
{
  "entities": [
    { "name": "CozoDB", "type": "technology" },
    { "name": "SQLite", "type": "technology" },
    { "name": "KittyNest", "type": "project" }
  ],
  "relations": [
    { "subject": "KittyNest", "predicate": "uses", "object": "CozoDB", "confidence": 0.95 },
    { "subject": "CozoDB", "predicate": "backed_by", "object": "SQLite", "confidence": 0.98 }
  ]
}
```

### 6.2 项目级记忆增量合并 Prompt 策略

增量合并时，LLM 接收：
1. **现有项目记忆**（Current Project Memory）
2. **新增 Session 记忆**（New Session Memory）

要求输出更新后的项目记忆，遵循以下规则：
- **合并重复**：如果新事实与旧事实语义相同，保留更精确的表述
- **消解冲突**：如果新事实与旧事实矛盾，优先采用更新时间更近的，并在记忆中标注冲突与决议
- **追加新事实**：将不重复的新事实归入正确分类
- **保留来源**：在事实后标注来源 session（如 `<!-- source: session-abc -->`）

### 6.3 系统级记忆聚合 Prompt 策略

系统级记忆聚合时，LLM 接收所有项目级记忆，输出全局视角的约束与偏好：
- 提取跨项目一致的技术偏好
- 提取跨项目通用的编码约束
- 保留项目间的差异（不强行统一）
- 按重要性排序事实

---

## 7. UI 交互设计

### 7.1 Memory 页面（新增/改造）

当前 `MemoryView` 为占位页面，需扩展为完整的 Memory 管理中心：

```
┌─────────────────────────────────────────┐
│  Memory                                 │
├─────────────────────────────────────────┤
│                                         │
│  [System Memory]                        │
│  ├─ 最后更新: 2026-04-26 10:00          │
│  ├─ [Refresh System Memory]  ← 手动按钮  │
│  └─ 预览: 用户偏好使用 Rust + React...   │
│                                         │
│  [Project Memories]                     │
│  ├─ KittyNest      最后更新: 自动       │
│  ├─ AnotherApp     最后更新: 自动       │
│  └─ ...                                 │
│                                         │
│  [Graph Stats]                          │
│  ├─ 实体总数: 128                       │
│  ├─ 关系总数: 342                       │
│  └─ [Explore Graph]  ← 预留入口         │
│                                         │
└─────────────────────────────────────────┘
```

### 7.2 按钮状态与反馈

| 按钮 | 常态 | 执行中 | 完成 | 失败 |
|------|------|--------|------|------|
| Refresh System Memory | 可点击 | Loading + "Aggregating..." | "Updated at 10:00" | 红色错误提示 |

---

## 8. 关键模块划分（Rust）

建议新增/扩展以下模块：

| 模块 | 职责 |
|------|------|
| `src/memory.rs` | Session/Project/System 三级记忆的生成、读取、增量合并 |
| `src/graph.rs` | CozoDB 连接管理、实体/关系的写入与查询封装 |
| `src/llm/prompts/memory.rs` | 各级记忆的 System Prompt 和用户 Prompt 模板 |
| `src/commands.rs` | 新增 `refresh_system_memory`, `get_memory_stats` 等 Tauri command |
| `src/db.rs` | 扩展 memory 相关表的 schema migration（如有必要） |

---

## 9. 实现里程碑

### Milestone 1: Session 级记忆 + 实体抽取（基础能力）
- [ ] 定义 LLM 输出契约（Markdown + JSON）
- [ ] 实现 `memory.rs` 中的 `generate_session_memory`
- [ ] 在 `store_session_analysis` 成功后自动调用 Session 记忆生成
- [ ] 验证文件写入路径和内容格式

### Milestone 2: 图数据库接入（CozoDB）
- [ ] 集成 CozoDB（SQLite-backed）到 Rust 工程
- [ ] 定义 CozoScript schema（entity / relation / entity_alias）
- [ ] 实现 `graph.rs` 的写入接口
- [ ] Session 记忆生成时同步写入实体关系

### Milestone 3: 项目级记忆增量更新（自动级联）
- [ ] 实现 `incremental_merge_project_memory`
- [ ] 设计增量合并 Prompt
- [ ] Session 记忆生成后自动触发项目记忆更新
- [ ] 验证幂等性和冲突消解效果

### Milestone 4: 系统级记忆 + UI（用户可见）
- [ ] 实现 `generate_system_memory`（手动触发）
- [ ] 改造 `MemoryView` 页面，展示系统记忆和项目记忆列表
- [ ] 添加 "Refresh System Memory" 按钮及状态反馈
- [ ] 图数据库统计信息展示（实体数、关系数）

### Milestone 5: 优化与边界处理
- [ ] 项目级记忆 Token 过大时的分段/压缩策略
- [ ] 实体消歧（同名不同义）
- [ ] 记忆版本回退（保留最近 N 版项目记忆）
- [ ] 图查询 UI 预留（低优先级）

---

## 10. 风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| LLM 增量合并导致记忆漂移 | 项目记忆质量下降 | 保留来源标注，定期全量重建项目记忆作为校验 |
| CozoDB Rust binding 成熟度 | 集成困难 | 备选方案：纯 SQLite 节点+边表 + 应用层图遍历 |
| Session 过多导致项目记忆过长 | Token 爆炸 | 引入记忆压缩/分层摘要，或按时间窗口分片 |
| 实体抽取噪声 | 图数据库垃圾数据 | 设置置信度阈值，定期清理孤立节点 |
| 系统级记忆手动触发被遗忘 | 记忆陈旧 | UI 添加"N 天未更新"提示，未来可配置定时提醒 |

---

## 11. 附录

### 11.1 目录结构总览

```
~/.kittynest/
├── config.toml
├── kittynest.sqlite              # 现有索引数据库
├── kittynest_graph.db            # CozoDB 图数据库 (SQLite-backed)
├── projects/                     # 现有项目产物
│   └── ...
└── memories/                     # 新增记忆存储
    ├── sessions/
    │   └── <session_slug>/
    │       └── memory.md
    ├── projects/
    │   └── <project_name>.md
    └── system/
        └── memory.md
```

### 11.2 与现有模块的交互点

- `analysis.rs:store_session_analysis()` → 成功后调用 `memory::generate_session_memory()`
- `analysis.rs:run_next_analysis_job()` → review_project job 完成后不触发记忆更新（review 是代码层面，非 session 层面）
- `commands.rs` → 新增 `refresh_system_memory` command，供前端 Memory 页调用
- `App.tsx` → `MemoryView` 组件扩展为完整记忆管理界面

---

*本报告作为 Memory Module 的实现依据。后续编码阶段如需调整，应同步更新本文档。*
