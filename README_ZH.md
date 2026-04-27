# KittyNest

<p align="center">
  <img src="docs/kittynest-ui-concept.png" alt="KittyNest UI 概念图" width="800"/>
</p>

<p align="center">
  <strong>本地优先的 Claude Code & Codex 记忆追踪器</strong>
</p>

<p align="center">
  <a href="README.md">English</a>
</p>

---

KittyNest 是一款**本地优先、隐私优先的 macOS 桌面应用**，帮助你追踪、整理和理解来自 [Claude Code](https://docs.anthropic.com/en/docs/agents-and-tools/claude-code/overview) 和 [Codex](https://openai.com/index/openai-codex/) 的 AI 编程会话。所有数据都保存在你的本地机器上 —— 会话以 SQLite 索引缓存，分析洞察以可读的 Markdown 文件形式存储。

## ✨ 功能特性

- **🔍 会话发现** — 自动扫描并导入 Claude Code（`~/.claude`）和 Codex（`~/.codex`）的会话数据
- **📁 项目追踪** — 按工作目录分组会话， review 项目健康度，生成项目摘要
- **📝 任务管理** — 将会话归纳到任务中，支持状态流转（讨论中 → 开发中 → 已完成）
- **🧠 记忆系统** — 三级级联记忆（会话级 → 项目级 → 系统级），附带实体关系图谱
- **🤖 LLM 驱动分析** — 使用你自己的 API 密钥分析会话和项目（支持 OpenAI 兼容、Anthropic 兼容等多种接口）
- **📊 仪表盘** — 一目了然地查看活跃项目、待办任务、最近会话和记忆状态
- **🔒 本地优先 & 隐私保护** — 所有数据存储在 `~/.kittynest`；除非显式发送给配置的 LLM 提供商，否则数据不会离开你的机器

## 🏗️ 系统架构

```
┌─────────────────────────────────────────────────────────────┐
│  React 前端 (Vite + TypeScript)                             │
│  仪表盘 · 项目 · 任务 · 会话 · 记忆 · 设置                   │
└──────────────────────┬──────────────────────────────────────┘
                       │ invoke
┌──────────────────────▼──────────────────────────────────────┐
│  Tauri 2 桌面壳                                             │
│  macOS 窗口 · 菜单 · 文件系统权限 · 应用生命周期             │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────┐
│  Rust 后端                                                  │
│  命令 · 适配器 · 任务队列 · LLM 客户端 · Markdown 存储       │
└──────────────────────┬──────────────────────────────────────┘
        ┌──────────────┴──────────────┐
        ▼                             ▼
┌───────────────┐           ┌─────────────────────┐
│  SQLite 索引  │           │  Markdown 存储      │
│  (本地缓存)   │           │  ~/.kittynest/      │
└───────────────┘           └─────────────────────┘
```

## 🚀 快速开始

### 环境要求

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://www.rust-lang.org/tools/install) 1.70+
- macOS（主要目标平台）

### 开发模式

```bash
# 安装前端依赖
npm install

# 开发模式运行（打开 Tauri 窗口）
npm run tauri dev
```

### 构建

```bash
# 构建生产包（.app 和 .dmg）
npm run tauri build
```

构建产物位于 `src-tauri/target/release/bundle/` 目录下。

## 📂 数据存储

所有应用数据均本地存储在 `~/.kittynest/` 目录下：

```
~/.kittynest/
├── config.toml              # LLM 提供商配置
├── kittynest.sqlite         # SQLite 索引数据库
├── kittynest_graph.db       # 实体关系图谱数据库
├── projects/
│   └── <project_slug>/
│       ├── info.md          # 项目摘要
│       ├── progress.md      # 项目进展
│       └── <task_slug>/
│           ├── summary.md
│           ├── user_prompt.md
│           └── <session>.md
└── memories/
    ├── sessions/
    │   └── <session_slug>/
    │       └── memory.md
    ├── projects/
    │   └── <project_name>.md
    └── system/
        └── memory.md
```

## ⚙️ 支持的 LLM 提供商

KittyNest 通过预设支持多种 LLM 提供商：

- OpenRouter
- DeepSeek
- 智谱 GLM
- 百炼
- Kimi（月之暗面）
- StepFun
- MiniMax
- 豆包（Seed）
- ModelScope
- Ollama（本地运行）
- OpenAI 兼容接口

你可以在**设置**页面配置你的提供商。

## 🛠️ 技术栈

| 层级 | 技术 |
|------|------|
| 前端 | React 18, TypeScript, Vite |
| 桌面壳 | Tauri 2 |
| 后端 | Rust |
| 数据库 | SQLite (rusqlite) |
| 图谱数据库 | CozoDB (SQLite 后端) / 纯 SQLite 降级方案 |
| UI 组件 | Lucide React, XYFlow |
| 样式 | 自定义 CSS |

## 🗺️ 路线图

- [x] 项目骨架与配置
- [x] Claude Code / Codex 数据源适配器
- [x] 项目追踪与手动 review
- [x] 历史会话批量分析
- [x] 新增会话增量扫描
- [x] 记忆模块（会话级 / 项目级 / 系统级）
- [x] 任务创建与管理
- [x] 设置页面与 LLM 配置
- [x] macOS 桌面集成
- [ ] 图谱查询 UI
- [ ] 记忆版本管理与回退
- [ ] 自动更新
- [ ] Apple Silicon 优化

## 🤝 参与贡献

欢迎提交 Issue 或 Pull Request！

## 📄 许可证

[MIT](LICENSE)

---

<p align="center">
  用 ❤️ 为本地优先的 AI 会话管理而构建
</p>
