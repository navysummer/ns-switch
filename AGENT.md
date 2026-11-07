# ns-switch — 项目功能规划清单

> 跨平台 AI 编程助手 API 供应商切换与管理软件。本文件是**功能规划清单**，为后续开发提供范围与结构依据。
> 技术栈：**Rust + GPUI**（Zed 的 GUI 框架，不用 Web 前端）。
> 功能参照 [cc-switch](https://github.com/farion1231/cc-switch)（Tauri + React）的能力模型，**独立设计、不照抄**——Rust 原生界面、配置即文件、最小侵入切换。
> 界面风格参照 `ns-embed-ide`（暖墨金深色主题，见第 8 章）。

---

## 1. 项目概述

| 项 | 内容 |
|---|---|
| 名称 | ns-switch（商量切换器） |
| 定位 | 一键切换 AI 编程助手（Claude Code / Codex / Gemini CLI 等）的 API 供应商，并集中管理 MCP / Skills / Prompts |
| 支持工具 | Claude Code、Claude Desktop、Codex、Gemini CLI、Grok Build（切换/直连+路由）、OpenCode、OpenClaw、Hermes、Pi、MiniMax Code、Kimi Code、Oh My Pi（共存） |
| 核心机制 | 供应商切换只改写客户端配置文件里的**关键字段**（地板），其余字节不碰 |
| 平台 | Windows / macOS / Linux |
| 技术栈 | Rust + GPUI |
| 参考 | cc-switch（只参考能力模型，不复制实现/界面） |

### 1.1 核心设计原则

1. **最小侵入**：切换只替换关键字段（端点、密钥、模型名、协议），插件/hooks/MCP/用户自建设置与注释原样保留；删除全部配置前先切走，保证工具永远可用。
2. **所有权层（地板/投影）**：每个工具的关键字段与供应商独有字段由纯函数清单定义；供应商行 → 目标值投影，按格式保序改写（JSON/TOML/.env），解析不了就停，不猜不改。
3. **配置即文件**：供应商列表与设置存 `~/.ns-switch/config.json`；客户端配置是唯一被改写对象，首次改写前自动备份。
4. **SSOT 单源**：供应商副本不落盘，所有状态以 `config.json` 为准，界面为投影。
5. **供应商预设可替换**：90+ 供应商预设为数据（Rust 常量表），可增可换；分类（官方/云服务商/聚合/第三方/自定义/本地）。
6. **代理可选**：直连模式下工具直连供应商；本地路由模式经本机代理做 API 格式转换与故障转移，退出时先写回直连配置。

### 1.2 工程纪律（所有修改必须遵守）

1. **零警告、零错误**：`cargo check --workspace` 必须 0 警告、0 错误，`cargo build` 可通过，`cargo test --workspace` 全绿。禁止用 `#[allow(dead_code)]` 等属性掩盖本项目的警告；未使用的代码必须真正接入界面/逻辑，否则删除。
2. **完整实现**：规格列出的功能必须真实、完整实现并验证。禁止用 `unimplemented!()`/`todo!()`/空函数体/硬编码返回值冒充完成；禁止界面控件"存在但点击无反应"；禁止以"预留/后续再做"交付核心路径。无法在当前迭代完成的能力必须明确标注**未完成**，列出缺口与验收方式。
3. **交付即验证**：提交前本机实际运行（`cargo check/build/test` + 启动应用走通关键路径 + 真实切换一次供应商并核对客户端配置文件）；UI 改动尽量截图核对。
4. **文档同步**：`AGENT.md` / `README.md` 的范围与完成状态必须与代码一致。

---

## 2. 技术架构

```
┌─────────────────────────────────────────────────────┐
│                    GPUI 界面层 (ns-ui)               │
│  侧栏(工具列表) · 供应商卡片 · 添加/编辑对话框 · 搜索   │
│  MCP/Skills/Prompts 面板 · 用量 · 会话 · 设置 · 托盘   │
├─────────────────────────────────────────────────────┤
│                   核心层 (crate)                     │
│  store (config.json) · live (地板/投影/补丁引擎)      │
│  presets (供应商预设表) · proxy (本地路由) · usage    │
│  tools (工具探测) · mcp · skills · prompts · sessions│
├─────────────────────────────────────────────────────┤
│              目标工具客户端配置 (被改写对象)            │
│  ~/.claude/settings.json · ~/.codex/config.toml      │
│  ~/.gemini/settings.json · 其余工具配置文件            │
└─────────────────────────────────────────────────────┘
```

### 2.1 建议 crate 划分

| crate | 职责 |
|---|---|
| `ns-switch`（bin） | GPUI 入口、窗口、启动、托盘 |
| `ns-ui` | 界面组件：侧栏、供应商卡片、对话框、面板、主题令牌 |
| `ns-store` | `~/.ns-switch/config.json` 读写、供应商 CRUD、设置、SSOT |
| `ns-live` | 地板清单、供应商→目标值投影、按格式补丁改写（JSON/TOML/.env）、首写备份 |
| `ns-presets` | 供应商预设数据表、搜索别名、模型元数据、图标 |
| `ns-proxy` | 本地路由：API 格式转换（Anthropic↔OpenAI↔Gemini）、故障转移、熔断 |
| `ns-usage` | 用量查询脚本/配额/余额、请求日志 |
| `ns-sessions` | 各工具会话日志解析、阅读视图、恢复命令 |
| `ns-tools` | 工具发现、版本探测、安装指引 |
| `ns-mcp` | MCP 服务器统一管理、跨工具同步 |
| `ns-skills` | Skills 仓库/安装/更新、同步到工具 |
| `ns-prompts` | 提示词库、写回 CLAUDE.md/AGENTS.md/GEMINI.md |

> crate 划分可在实现阶段调整；此处仅作规划约束。`ns-ui` 依赖 GPUI，核心 crate 不依赖界面。

---

## 3. 功能模块总览

| # | 模块 | 说明 |
|---|---|---|
| F1 | 供应商管理 | 预设添加、CRUD、一键切换、拖拽排序、导入导出、搜索 |
| F2 | 切换引擎 | 地板字段改写、备份、冲突检测、回滚 |
| F3 | 本地路由 | API 格式转换、每工具开关、故障转移、熔断、整流器 |
| F4 | 聚合模式 | 多供应商模型合并列表、默认供应商、按模型路由 |
| F5 | MCP 管理 | 统一 MCP 面板、跨工具同步、导入 |
| F6 | Skills 管理 | 仓库/安装/更新、同步到工具 |
| F7 | Prompts 管理 | 提示词库、Markdown 编辑器、写回提示文件 |
| F8 | 用量统计 | 套餐额度/余额、请求日志、趋势图 |
| F9 | 会话管理 | 会话列表/搜索/阅读视图、恢复命令 |
| F10 | 系统托盘 | 托盘快速切换、状态显示 |
| F11 | 工具管理 | 工具版本探测、安装/升级、重复安装诊断 |
| F12 | 项目与账号 | 项目快照切换、托管账号（Copilot/ChatGPT/xAI） |
| F13 | 云同步 | WebDAV / S3 兼容存储同步 |
| F14 | 界面框架 | 侧栏、主题、设置页、快捷键、欢迎页、通知 |
| F15 | 辅助功能 | 深链导入、自动更新、备份管理、环境冲突告警 |

---

## 4. 详细功能清单

### F1 供应商管理

#### F1.1 供应商数据模型（`config.json`）

```jsonc
{
  "providers": {
    "<id>": {
      "id": "claude-xxx",
      "name": "某聚合",
      "category": "aggregator",        // official | cn_official | cloud_provider | aggregator | third_party | custom | local
      "settingsConfig": { },            // 应用配置对象：Claude 为 settings.json 关键字段；Codex 为 { auth, config }
      "websiteUrl": null,
      "createdAt": 0, "sortIndex": 0,
      "notes": null,
      "icon": "openai", "iconColor": "#00A67E",
      "inFailoverQueue": false
    }
  },
  "current": { "claude": "<id>", "codex": "<id>" },
  "settings": { "theme": "dark", "visibleApps": {}, "appOrder": [] }
}
```

- SSOT 模式：不写供应商副本文件，所有状态以 `config.json` 为准。
- 供应商元数据（`meta`，如用量脚本模板）只存 `config.json`，不写入 live 配置。
- 原子写：写临时文件 + rename；启动时校验并回退损坏文件。

#### F1.2 预设库

- 供应商预设表（90+）：按工具分类（claude/codex/gemini/grok…），含中文名/域名搜索别名、模型元数据、分类、图标。
- 预设即数据（Rust 常量表），不硬编码进逻辑；供应商的 `settingsConfig` 由预设模板 + 用户输入（endpoint/key/model）生成。
- 新增供应商流程：选工具 → 选预设（可搜索）→ 填关键字段 → 保存。
- 官方供应商（Claude Official / OpenAI Official / Google Official / Grok Official）内建，可删可加回。

#### F1.3 供应商操作

- 添加/编辑（编辑显示切换后配置文件将长什么样）。
- 删除：切换类工具**至少保留一个在用供应商**，删当前需先切走；共存类工具可直接删。
- 一键切换（见 F2）。
- 拖拽排序（`sortIndex`）。
- 导入/导出（JSON，含校验）。
- 供应商搜索（名称/中文名/域名）。
- 重复/测试连通性（可选）。

### F2 切换引擎（核心）

#### F2.1 地板（关键字段）清单

- 每个工具定义**地板**：连接相关键（端点、凭据、模型名、协议选择器、独有兼容开关/窗口值）。
- Claude Code：`env` 内 `ANTHROPIC_*`/`AWS_*`/`VERTEX_REGION_*` 前缀 + 命名键 + `CLAUDE_CODE_USE_*` 协议选择器 + 顶层 `apiKeyHelper` 配套键。
- Codex：`auth`（`OPENAI_API_KEY` 等）+ `config` 的 `model_provider`/`model`/`model_catalog_json` + `model_providers` 表。
- Gemini：`settings.json` 关键字段 + `.env` 的 `GOOGLE_API_KEY`/`GEMINI_API_KEY`。
- 其余工具：各自的连接/鉴权键（TOML/JSON/YAML/.env）。
- 地板清单是纯函数谓词 + 常量清单，前端/测试镜像与后端由测试保证一致。

#### F2.2 投影与补丁

- 供应商行 → 目标值投影（纯函数，不读写文件）。
- 按格式保序改写：JSON（保留缩进/注释/键序）、TOML（保留注释/键序）、`.env`。
- 解析不了 → 停止并报错，不猜不改；失败不留下半写状态。
- 首写备份：首次改写每个配置文件前备份到 `~/.ns-switch/backups/live-first-write/`。
- 写锁（单写者）、写后重读比对（确认落盘内容）。

#### F2.3 切换语义

- 直连模式：切换 = 读目标投影 → 补丁改写 live 配置文件。
- 路由模式：live 配置文件指向 `127.0.0.1` 与占位 key，真实供应商存 `config.json`；切换只改路由指针，即时生效。
- 切换后：更新 `current`、通知事件、刷新界面；Codex/Gemini/Grok 提示需重启终端（Claude Code 热切换）。
- 退出：路由模式下先写回直连配置再退出（下次启动重连）。
- 环境冲突检测：写入前检查目标配置当前是否被另一供应商/手动配置占用，冲突告警。

### F3 本地路由（代理）

- 每工具开关（Claude Code / Codex / Gemini CLI / Grok Build 各自独立）。
- API 格式转换：Anthropic Messages ↔ OpenAI Chat Completions ↔ OpenAI Responses ↔ Gemini Native。
- 请求转发、`/v1` 路径映射、流式（SSE）透传。
- 故障转移：按队列顺序尝试下一供应商，熔断器（失败阈值/冷却）、健康探测。
- 整流器：自动修正某些上游无法处理的请求（Thinking 签名、图像不支持回退）。
- 官方供应商默认不走代理（Codex OpenAI Official 例外）。
- 代理状态/监听地址/故障转移设置在设置页；请求日志（请求模型→实际模型）。

### F4 聚合模式

- Claude Code / Codex 每工具开关；添加/移除供应商、选默认。
- 模型合并列表写回客户端模型目录；请求按所选模型路由到对应供应商。
- 不提供故障转移；Codex 改聚合列表需重启 daemon；Claude Code 即时生效。
- 移除/修改时保留一个默认供应商（不可删除当前默认）。

### F5 MCP 管理

- 统一 MCP 面板：所有 MCP 服务器一处管理。
- 选择同步目标（Claude Code / Codex / Gemini CLI / Grok Build / OpenCode / Hermes / Pi / MiniMax Code / Kimi Code / Oh My Pi）。
- 从各工具导入既有配置、Deep Link 导入。
- 增删/启用禁用、搜索、跨工具启用/禁用全部。

### F6 Skills 管理

- 搜索 skills.sh、从 GitHub 仓库/ZIP 一键安装。
- 检查更新、一键全部更新。
- 同步策略：自动 / 仅符号链接 / 仅复制；可选存 `~/.agents/skills`。
- 每工具启用/禁用全部。

### F7 Prompts 管理

- 每工具提示词库 + Markdown 编辑器。
- 启用写回该工具提示文件（CLAUDE.md / AGENTS.md / GEMINI.md）；写回前先把文件既有内容存回库，不丢失。
- Pi 可编辑 SYSTEM.md / APPEND_SYSTEM.md / 提示词模板。

### F8 用量统计

- 供应商卡片/托盘显示剩余额度与重置倒计时（官方订阅配额、Coding Plan 5h/周/月、账户余额）。
- 自定义用量脚本（JavaScript 模板，在 Rust 侧以 mini-JS 引擎或受管子进程执行）。
- 请求日志：扫描各工具本地会话日志，按供应商/模型统计请求、token、缓存命中率、输出速度、花费。
- 趋势图、全年热力图、按请求明细。

### F9 会话管理

- 列表/搜索（标题/目录/消息/会话 ID）、按项目或时间分组、批量删除。
- 阅读视图：文本/思考/工具调用/工具结果/图像分开展示；每轮折叠为一行摘要（步数/文件改动/失败，失败标红）。
- 右侧大纲、会话内搜索、Markdown 导出、复制单轮/整会话、恢复命令（macOS 一键在终端恢复）。

### F10 系统托盘

- 托盘图标 + 菜单：每个支持切换的工具列出供应商，点击即切换。
- 状态显示（当前供应商、路由中、用量摘要）。
- 单击打开主窗口；退出托盘可选最小化。

### F11 工具管理

- 列出各 CLI 工具（Claude Code / Codex 等）当前与最新版本。
- 安装/升级/全部升级、重复安装诊断。
- Windows 下管理 WSL 内工具（平台分支）。

### F12 项目与账号

- 项目快照：保存某工具的当前供应商 + MCP + Skills + Prompts 为一项目；一键整组切换；切换后自动回存上一项目。
- 项目切换器（顶部，默认隐藏，设置开启）。
- 托管账号（Beta）：GitHub Copilot / ChatGPT / xAI 登录，用订阅作为供应商（需路由模式）。

### F13 云同步

- WebDAV（坚果云/Nextcloud/Synology 等）与 S3 兼容存储（AWS S3/R2/OSS/COS）。
- 手动/自动同步、冲突处理；或把 `~/.ns-switch` 放入云盘目录。

### F14 界面框架

- **侧栏**：工具列表（图标+名称，可排序/隐藏）、页面切换（供应商/路由/MCP/Skills/Prompts/用量/会话/应用/设置）。
- **供应商卡片**：图标、名称、当前标记、状态徽章（直连/路由/聚合）、用量脚注、操作菜单（编辑/删除/切换/重复/测试）。
- **添加/编辑对话框**：预设选择（搜索）、关键字段表单、配置预览。
- **主题**：暖墨金深色（第 8 章），浅色预留。
- **设置页**：通用（主题/开机自启/自动更新）、数据（备份/占用）、本地路由（监听地址/故障转移）、云同步。
- **通知**：操作成功/失败 toast、错误可点击跳转。
- **快捷键**：切换、搜索、新增等。
- **欢迎页**：首次运行引导、空状态。

### F15 辅助功能

- 深链导入（`nsswitch://`：导入供应商/MCP/Skills/Prompts）。
- 自动更新（检查/下载/安装，平台差异分支）。
- 备份管理：查看所有备份位置与占用，手动备份/恢复。
- 环境冲突告警横幅（启动/切换时检测）。
- 崩溃恢复（会话状态保存/恢复，预留）。

---

## 5. 里程碑划分（建议实现顺序）

| 阶段 | 目标 | 覆盖模块 |
|---|---|---|
| **M0 脚手架** | Rust + GPUI 空壳可运行，主窗口、暖墨金主题、侧栏、供应商页占位 | F14（基础） |
| **M1 存储与预设** | `config.json` SSOT、供应商 CRUD、预设表、原子写 | F1 |
| **M2 切换引擎** | 地板清单、投影、JSON/TOML/.env 补丁、首写备份、写锁 | F2 |
| **M3 界面·供应商** | 卡片、添加/编辑对话框、搜索、拖拽排序、导入导出 | F1、F14 |
| **M4 切换集成** | 界面一键切换、冲突检测、切换事件、提示需重启 | F2 |
| **M5 本地路由** | HTTP 代理、API 格式转换、每工具开关、故障转移、熔断 | F3 |
| **M6 托盘** | 托盘图标、快速切换菜单、状态 | F10 |
| **M7 MCP/Skills/Prompts** | 三个统一面板 + 跨工具同步 | F5、F6、F7 |
| **M8 聚合模式** | 模型合并列表、按模型路由 | F4 |
| **M9 用量统计** | 用量脚本/配额/余额、请求日志 | F8 |
| **M10 会话管理** | 会话解析、阅读视图、恢复 | F9 |
| **M11 工具/项目/账号** | 版本探测、项目快照、托管账号 | F11、F12 |
| **M12 辅助与体验** | 深链、自动更新、备份、云同步、设置页 | F13、F14、F15 |

> 里程碑顺序可调整；**M1 与 M2（存储 + 切换引擎）是全局依赖，建议最先稳定**。

### 5.1 当前实现状态（以本仓库代码为准）

✅ 已实现并验证：

- **M0 脚手架**：Cargo 工作区（ns-switch bin + ns-ui/ns-store/ns-live/ns-presets）、GPUI 主窗口、暖墨金主题令牌（移植 ns-embed-ide theme.rs）、通用组件（品牌徽标/主按钮/幽灵按钮/胶囊/状态点）、侧栏（13 工具列表、切换/共存徽章、9 页面切换）、顶栏（品牌 + 添加供应商）、供应商页（卡片列表 + 空态 + 欢迎页）、状态栏（状态点/当前工具/版本）；`cargo check --workspace` 0 警告 0 错误，`cargo test --workspace` 14 用例全绿，`target/debug/ns-switch` 启动不崩溃。
- **M1 存储与预设（切片）**：`ns-store` —— `~/.ns-switch/config.json` SSOT 读写（原子写临时文件 + rename）、损坏回退（备份 `.corrupt-<ts>` 后回默认）、供应商 CRUD、当前选择、设置；`ns-presets` —— 工具枚举（13 工具，切换/共存/路由/聚合能力）、供应商数据模型（SSOT 模式）、供应商分类、预设表（Claude Code/Codex/Gemini CLI 首批 17 个：官方 + 88API/OpenRouter/PPIO/AIGoCode/CCSub/SiliconFlow/Ollama/Fenno 等）、搜索（名称/别名/域名）、供应商 → settingsConfig 关键字段生成模板；`ns-live` —— Claude Code `env` 地板清单（前缀/命名键/协议选择器/`CLAUDE_CODE_SKIP_*_AUTH`）。

🚧 未完成（不得声称已实现）：

- **M1 其余**：预设表扩至 90+（其余工具）、供应商卡片真实数据渲染接线、导入导出 UI。
- **M2–M12**：M2 切换引擎其余（Codex/Gemini 地板、投影、补丁引擎、首写备份、写锁）、M3 界面·供应商其余（对话框/搜索/排序/导入导出）、M4 切换集成、M5 本地路由、M6 托盘、M7 MCP/Skills/Prompts、M8 聚合、M9 用量、M10 会话、M11 工具/项目/账号、M12 辅助与体验 —— 均未开始，不得声称已实现。

---

## 6. 与 cc-switch 的差异化（只参考能力，不照抄）

| 维度 | cc-switch | 本项目 |
|---|---|---|
| 技术栈 | Tauri 2 + React + Tailwind | Rust + GPUI 原生 |
| 界面 | Web 渲染，CSS 主题 | GPUI 原生，暖墨金主题（ns-embed-ide 风格） |
| 供应商数据 | TypeScript 预设 + Rust 后端 | Rust 常量表（单一语言，无前后端分裂） |
| 切换引擎 | 前后端分离（invoke） | 纯 Rust 核心，UI 直调，可单测 |
| 托盘 | Tauri tray | GPUI/系统托盘（平台分支） |
| 代理 | Rust hyper 服务 | Rust 异步 HTTP 服务（同层复用） |
| 配置格式 | JSONC/TOML 保留注释改写 | 保序补丁引擎（JSON/TOML/.env），原则一致 |

---

## 7. 关键风险与约束

1. **GPUI 生态年轻**：组件库、托盘、系统集成需验证；界面层抽象隔离，避免阻塞核心逻辑。
2. **工具配置格式多**：JSON/JSONC/TOML/YAML/.env，注释保留与保序改写难度大；补丁引擎先行，解析不了就停。
3. **代理格式转换复杂**：Anthropic/OpenAI/Gemini 差异大（消息结构/工具/流式/图像）；分阶段实现，先 Anthropic↔OpenAI。
4. **跨平台差异**：Windows 配置路径/进程/托盘与 Unix 不同；路径抽象 + 平台分支。
5. **12 个工具面广**：按优先级逐个接入（先 Claude Code/Codex/Gemini，其余共存工具后置），避免一次铺开。
6. **不破坏用户配置**：地板改写必须精确、可备份、可回滚；这是本项目生命线，任何不确定都停在改写前。

---

## 8. 界面风格（设计语言）

> 整体为**暖墨金深色主题**（参照 `ns-embed-ide`）：墨色底、鎏金点缀、玉/朱辅色；宋/楷体字系、大写字距、圆角卡片、毛玻璃、柔光晕与星点纹理，文人器物气质。以下为设计令牌与组件规范，界面实现遵循此语言。

### 8.1 色彩令牌

| 令牌 | 值 | 用途 |
|---|---|---|
| `--bg-0` | `#0f0d0a` | 最底层背景 |
| `--bg-1` | `#17130d` | 背景渐变起点/面板衬底 |
| `--bg-2` | `#211a11` | 悬浮/抬升层 |
| `--panel` | `rgba(38,29,18,0.6)` | 半透明面板（毛玻璃） |
| `--panel-solid` | `#201a11` | 实体面板（模态等） |
| `--stroke` | `rgba(219,181,121,0.14)` | 默认描边 |
| `--stroke-strong` | `rgba(219,181,121,0.32)` | 强调描边（hover/虚线环） |
| `--text` | `#f0e6d2` | 主文字（暖米白） |
| `--text-dim` | `#b8a98c` | 次文字 |
| `--text-faint` | `#8a7c63` | 弱文字 |
| `--accent` | `#c9a25f` | 主强调（鎏金） |
| `--accent-2` | `#a57a3d` | 强调暗部 |
| `--accent-3` | `#d8b97f` | 强调亮部 |
| `--jade` | `#6f9b8a` | 辅色·玉（成功/状态） |
| `--cinnabar` | `#b0564a` | 辅色·朱（告警/装饰） |
| `--danger` | `#cf6b5e` | 错误 |
| `--grad` | `linear-gradient(135deg,#d8b97f 0%,#c9a25f 48%,#8a6b35 130%)` | 主渐变（按钮/徽标/播放钮） |

### 8.2 字体

- 正文字系：`PingFang SC, Songti SC, STSong, Kaiti SC, STKaiti, KaiTi, Microsoft YaHei, serif`。
- 标题字距 4–5px、正文/按钮字距 1px，按钮字号 13.5px、正文 12.5–13.5px。
- 时间/数字使用 `tabular-nums` 等宽数字。

### 8.3 形状与投影

- 圆角：卡片 14px、大卡片/模态 20px、输入框/按钮 12px、状态徽标/头像 999px。
- 投影：`0 18px 50px -12px rgba(0,0,0,0.7)`；强调元素附加金色色晕 `0 6–8px 24px -6px rgba(201,162,95,0.55~0.65)`。

### 8.4 表面与层次

- 顶栏/状态栏：`rgba(23,19,13,0.78)` + `blur(18px)` + 1px 描边，毛玻璃。
- 背景：`bg-1 → bg-0` 竖向渐变，叠加三团径向光晕（金/玉/朱）。
- 装饰纹理：固定"墨脉"星点层（6 处彩色 1–1.4px 点，380px 周期），`mix-blend-mode: screen`、`opacity 0.28`，位于内容之下。
- 模态遮罩：`rgba(8,6,4,0.62)` + `blur(8px)`。

### 8.5 组件规范

| 组件 | 规范 |
|---|---|
| 主按钮 | 金色渐变底、深字 `#221407`、字重 600、字距 1px；hover 提亮 + 上浮 1–2px |
| 幽灵按钮 | `--panel` 底 + 1px `--stroke` 描边、次文字；hover 提亮/描边加强 |
| 圆形控制钮 | 38–46px 圆形，半透明暖底 + 描边；hover 放大 1.06 |
| 输入框 | 圆角 12px、底色 `rgba(255,235,200,0.05)` + 描边；聚焦 `--accent` 描边 + 3px 光晕 |
| 模态卡片 | `--panel-solid` 底、20px 圆角、投影、标题 18px/700/字距 4px |
| 状态徽标 | 胶囊（999px）、同色系半透明底 + 描边（玉/朱/金） |
| 品牌徽标 | 圆形 `radial-gradient(circle at 35% 30%, 亮, 金 58%, 暗金)` + 1px 亮描边 + 内圈暗环 + 金色投影，中央"切"字 |

### 8.6 动效

- 交互：按钮/控件 `transition: all 0.18s ease`。
- 模态进入：`fadeIn 0.2s ease`（遮罩）+ `popIn 0.22s cubic-bezier(0.2,0.9,0.3,1.2)`（卡片）。
- 空状态图标：`floaty 4s ease-in-out infinite` + 外圈虚线环 `spin 18s linear infinite`。
- 出现/消失：滑动、淡入淡出约 `0.35s ease`。

> 深色优先；浅色主题预留。视觉元素采用线性/纯色简洁风格，配合墨金配色。

---

## 9. 参考对照（实现时对照，不复制）

- cc-switch 后端：`src-tauri/src/{config,provider,live,proxy,mode}.rs`、`live/{floor,project,patch,engine}.rs`。
- cc-switch 前端：`src/components/providers/`、`src/components/shell/`、`src/config/*Presets.ts`、`src/types.ts`。
- 界面风格：`ns-embed-ide/crates/ns-ide/src/{theme,widgets}.rs`、`AGENT.md` 第 8 章。
