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
| 支持工具 | Claude Code、Claude Desktop、Codex、Gemini CLI、Grok Build（切换/直连+路由）、OpenCode、OpenClaw、Hermes、Pi、MiniMax Code、Kimi Code、Antigravity、Oh My Pi（共存） |
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
4. **文档同步**：`AGENTS.md` / `README.md` 的范围与完成状态必须与代码一致。

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
- **已实现**（`ns-mcp` crate + MCP 页）：统一 `McpServer` 模型、Claude/Codex/Gemini 读写适配器（保注释/键序）、跨工具同步与导入；UI 支持增删/编辑/启停/同步。

### F6 Skills 管理

- 搜索 skills.sh、从 GitHub 仓库/ZIP 一键安装。
- 检查更新、一键全部更新。
- 同步策略：自动 / 仅符号链接 / 仅复制；可选存 `~/.agents/skills`。
- 每工具启用/禁用全部。
- **已实现**（`ns-skills` crate + Skills 页）：统一 `Skill` 模型、Claude/Codex/Gemini 目录读写（`SKILL.md` 前端 YAML）、GitHub/本地安装、跨工具同步；UI 支持增删/编辑/安装。

### F7 Prompts 管理

- 每工具提示词库 + Markdown 编辑器。
- 启用写回该工具提示文件（CLAUDE.md / AGENTS.md / GEMINI.md）；写回前先把文件既有内容存回库，不丢失。
- Pi 可编辑 SYSTEM.md / APPEND_SYSTEM.md / 提示词模板。
- **已实现**（`ns-prompts` crate + Prompts 页）：统一 `PromptEntry` 模型、独立 `prompts.json` 库、Claude/Codex/Gemini 提示文件写回（先回存既有内容再合并写回）；UI 支持增删/编辑/启停/写回。

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

- **M0 脚手架**：Cargo 工作区（12 crate）、GPUI 主窗口、暖墨金主题令牌、通用组件（品牌徽标/主按钮/幽灵按钮/胶囊/状态点）、侧栏（13 工具、页面切换）、顶栏、供应商页、状态栏；`cargo check --workspace` 0 警告 0 错误、`cargo test --workspace` 全绿。
- **M1 存储与预设**：`ns-store` —— `~/.ns-switch/config.json` SSOT（原子写 + 损坏回退）、供应商 CRUD、当前选择、设置、项目/账号/同步；`ns-presets` —— 13 工具枚举、供应商数据模型、分类、预设表（615 条，覆盖全部工具）、搜索（名称/别名/域名）、settingsConfig 生成模板。
- **M2 切换引擎**：`ns-live` —— Claude/Codex/Gemini/GrokBuild 地板清单、供应商→目标值投影、保序补丁引擎（JSONC/TOML/.env）、首写备份（去重 `.source`）、应用写锁、原子写、写后重读比对。
- **M3 界面·供应商**：供应商卡片（操作菜单）、添加/编辑对话框（预设搜索 + 关键字段 + 预览）、删除确认、搜索框、拖拽排序（`sortIndex`）、导入/导出 JSON（校验）。
- **M4 切换集成**：界面一键切换（写 live + 更新 `current`）、写入前冲突检测、切换后提示（Claude 热切换；Codex/Gemini/Grok 需重启终端）、端到端验证（真实切换核对配置、重复切换 noop、首写备份保留原始）。
- **M5 本地路由**：`ns-proxy` —— `bind`/`serve`/`serve_listener`（hyper 真实监听转发）、4 种上游格式转换（Anthropic/OpenAI Chat/OpenAI Responses/Gemini）、`/v1` 路径映射、熔断器（Closed/Open/HalfOpen）+ 故障转移队列 + 模型路由重排、整流器（Thinking 剥离/图像回退）、请求日志（环形缓冲 + 按工具汇总）。UI：路由页每工具开关（起停代理 + 写 live 指向 `127.0.0.1:<port>`）、故障转移开关、退出写回直连。
- **M6 系统托盘**：`tray.rs`（`tray-icon`+`muda`）—— 每工具供应商菜单、点击切换、打开窗口/退出、事件经 mpsc 后台轮询双路径消费、供应商/路由变化重建菜单。
- **M7 MCP/Skills/Prompts**：`ns-mcp` —— 统一 `McpServer` 模型 + Claude/Codex/Gemini 读写适配器（保注释/键序）+ 跨工具同步/导入；`ns-skills` —— 统一 `Skill` 模型 + 目录读写（`SKILL.md` frontmatter）+ GitHub/本地安装 + 同步（Symlink/Copy）；`ns-prompts` —— 统一 `PromptEntry` + 独立 `prompts.json` 库 + 写回（先回存既有内容再合并）。三个统一面板（页签×工具、增删/启停/同步/写回）。
- **M8 聚合模式**：Claude/Codex 开关、聚合供应商增删/默认、模型合并列表写回（Codex `model_catalog_json` + `codex-aggregate-models.json`）、按模型路由（`model_routes` 映射）。
- **M9 用量统计**：`ns-usage` —— 用量脚本执行（Node/Deno/Bun 检测 + 宽松 key 解析）、价格表、Claude 会话日志统计（token/成本）、趋势图/热力图/明细。
- **M10 会话管理**：`ns-sessions` —— Claude 会话列表/搜索/按项目分组、轮次构建（消息/工具调用/结果）、阅读视图（分节折叠 + 大纲 + 会话内搜索）、Markdown 导出、恢复命令、删除。
- **M11 工具管理**：`ns-tools` —— 5 工具探测（PATH + 版本）、安装/升级/卸载、重复安装诊断；项目快照（保存/应用/删除 + 顶部切换器）；托管账号（Beta：`auth.rs` 读本地凭据 + `login_account` 建托管供应商）。
- **M12 辅助与体验**：深链（`nsswitch://` 6 种动作解析 + `apply_deeplink` 全处理 + 单实例回环 TCP 转发 + 注册脚本）、自动更新（GitHub API 检查 + 下载安装）、WebDAV 同步（push/pull）、自启（macOS launchd）、设置页（主题/语言/自启/更新/深链/同步/备份）、i18n（侧栏/顶栏/状态栏/设置/授权中心 4 语言）、应用管理页、侧栏排序/隐藏、TextField/TextArea 键入重绘修复。

🚧 已知缺口（后续增强，不影响主路径）：

- **M5 流式实时转发**：SSE 已接线（上游 SSE 分块 → Anthropic SSE 帧全缓冲转换，保证合法 SSE 序列；逐块实时转发为后续增强）；`failover` 开关已真实化（关闭时只用首上游，不熔断转移）。
- **M9**：用量仅 Claude 会话日志（Codex/Gemini 无）；价格仅内置表；卡片不显示 total/used。
- **M10**：会话已支持 Claude + Codex 列表/来源切换（`ns-sessions` 解析 Codex `~/.codex/sessions/**/rollout-*.jsonl` 元数据）；Claude/Codex 自动识别内容解析（Codex best-effort：message/function_call/tool_output/event_msg，已单测）；恢复命令按工具生成（Claude/Codex，跨平台）。**仍缺**：Codex 完整阅读细节/更复杂事件、Gemini/OpenCode/Pi/Hermes/OpenClaw 会话解析与阅读、多工具用量日志。
- **M7**：MCP 传输已支持 Stdio/Http/SSE/WebSocket 选择且表单含 env/headers；**仍缺**项目作用域 UI、多工具适配器扩展（当前主要 Claude/Codex/Gemini）、深链导入确认/风险提示。Skills 已有同步策略 UI（自动/符号链接/复制）；**仍缺** GitHub `skills/<name>/SKILL.md` 子目录、本地目录安装 UI、`allowed_tools` 编辑、检查更新/全部更新、`~/.agents/skills` 选项。Prompts 写回在两次写回之间手改文件可能陈旧。
- **M11**：托管账号登录为模拟标记（无真实 OAuth 流程）；项目「自动回存」已在 `ns-store::apply_project` 实现（切换前保存 `active_project`），UI 经同一路径调用；**仍缺**与 cc-switch Profile（按 scope 分组）的语义对齐与更丰富的快照范围。
- **M12**：更新仅手动（无自动检查/定时，版本为字符串比较）；同步仅 WebDAV 且手动（无 S3、无定时、无冲突合并）；自启已实现 macOS/Linux/Windows（Windows/Linux 需平台验证）。

> 缺口清单非「未开始」——对应模块主体已实现并验证，缺口为能力边界，随迭代补齐。

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
- 界面风格：`ns-embed-ide/crates/ns-ide/src/{theme,widgets}.rs`、`AGENTS.md` 第 8 章。

---

## 10. 对比 cc-switch 差距与优先完善清单

> 对比参考：本机 `/Users/navysummer/Desktop/other/cc-switch`（Tauri 2 + React，v4.x README/源码）。本节记录**相对 cc-switch 的能力/体验差距**，按优先级逐项完善；**每完成一项即更新状态与验证记录**。已在 5.1「已知缺口」中列出的条目在此汇总并细化，不重复造第二套真相。

状态约定：`[ ]` 待办 · `[~]` 进行中/部分 · `[x]` 已完成（附验证）

### P1 — 主题与基础体验（正确性/名不副实）
- [x] **P1-1 主题实际应用（dark/light/system）**：`ns-ui::theme` 增加全局活动模式/有效调色板缓存与动态访问器（`text/bg/panel/stroke/accent/*_a/surface_*`）；`ns-switch`/`ns-ui` 主要 UI 常量用色批量替换为主题感知；`SwitchApp` 初始化与设置变更同步 `set_active_mode`，根背景使用 `app_background_for(mode)`。**验证**：`cargo check --workspace` 0 错误、0 项目警告（仅第三方 future-incompat 提示）；`cargo test --workspace` 全绿。System 模式在设置/启动时解析有效外观（渲染期不重复探测）；**运行时跟随 OS 外观实时变化已接线**——`main.rs` 窗口创建后若 `active_mode()==System` 注册 `window.observe_window_appearance`，外观变化时 `set_system_effective(dark)` 更新有效调色板 + `window.refresh()` 触发重绘（非 System 时回调忽略）；新增 `system_effective_tracks_os_appearance`/`mode_parse_roundtrip` 单测。

### P2 — 供应商卡片/对话框（高频路径）
- [x] **P2-1 供应商连通性测试**：卡片菜单“测试连接”从 `settingsConfig` 提取端点并后台执行 `curl -I -m 8`（端点可达性），状态栏显示明确成功/失败原因。**验证**：`cargo check/test --workspace` 绿。
- [x] **P2-2 供应商卡片操作补全**：已实现“打开官网”（`websiteUrl`）、“终端打开配置目录”（按平台）、“复制供应商”（真实复制+新 id）、“导出此供应商”（`~/.ns-switch/exports/*.json`）；菜单动作已重构为可执行回调。**导出/复制反馈已补全**：`export_providers`（批量）与 `export_single_provider`（单个）写入后状态栏「已导出 N 个供应商 → path」/「已导出供应商到：path」+ `StatusTone::Success`；`duplicate_provider` 复制成功状态栏「已复制供应商：name」+ `StatusTone::Success` + **新副本 2 秒玉色高亮描边定位**（`copied_provider` 短暂展示后自动清除）；失败均 `StatusTone::Error`。
- [x] **P2-3 供应商对话框能力**：新增“上游 API 格式”选择（anthropic/openai_chat/openai_responses/gemini_native），写入/读取 `ProviderMeta.apiFormat`；预览文案明确；修复对话框关闭/取消无响应。**按工具高级选项/认证形态字段已补全**：`DialogFields::from_provider` 启发式扫描 `settings_config` 顶层与嵌套（`env`/`auth`/`config`）按关键字恢复 endpoint（endpoint/base_url/baseUrl/url/BASE_URL）/key（api_key/apiKey/key/token/auth_token/AUTH_TOKEN/API_KEY/api_token）/model（model/default_model/defaultModel/MODEL）；`api_format_selector` 下拉；图标（`icon`）与颜色（`icon_color`）编辑；`usage_script` 路径编辑；对话框未保存守卫（`dialog_dirty`/`request_close`/`confirm_close` 浮层）。
- [x] **P2-4 卡片信息密度**：已显示 `iconColor` 头像、备注/域名副标题、非 Anthropic  的 `apiFormat` 芯片，卡片自适应高度。**品牌图标/状态芯片/健康状态已补全**：`icon_glyph`（17 品牌图标字形映射：anthropic❋/openai⬡/google◆/grok✦/copilot⌥/api⌁/router⛶/cloud☁/code⌘/sub⧉/flow≋/local⬢/mini▣/kimi✳/piπ/hermes⚡/claw✪/omp◉ + 未知回退首字母，单测 `icon_glyph_maps_brands`）；模式/路由/聚合芯片（`routing_for`/`aggregation_for` → 路由/聚合/直连 pill）；故障转移芯片（`in_failover_queue`）；Coding Plan 徽章（`is_coding_plan` 朱色）；在线/离线健康芯片（`health` map）；配额脚注（P3-2 额度列）。
- [x] **P2-5 冲突与告警 UX**：切换/写入时已通过 `switch_engine::check_conflicts` 在状态栏给出冲突说明；供应商页在切换检测到冲突后显示可忽略横幅（`switch_conflicts`）；设置页有路由端口冲突提示；Codex 聚合写入后追加 daemon/CLI/桌面应用完全重启强提示。**启动扫描/写入前确认对话框已补全**：`SwitchApp::new` 末尾异步后台任务遍历所有工具 `check_conflicts` 写 `startup_conflicts`（供应商页横幅同时判断 `switch_conflicts`/`startup_conflicts`，「忽略」清空两个）；`write_provider` 写入前检测冲突，有冲突时暂存 `pending_switch = Some((tool, provider, note))` + `StatusTone::Error` 状态栏提示，`render_pending_switch_confirm` 确认浮层（继续/取消），确认后 `apply_switch` 强制执行。

### P3 — 用量/会话/配额（可观测性）
- [x] **P3-1 用量多工具**：Claude 会话日志（`scan_claude_sessions`）+ Codex（`scan_codex_sessions`，续12）+ **Gemini**（`scan_gemini_sessions`：`~/.gemini/tmp/**/chats/session-*.jsonl`，`type=="gemini"` 记录 tokens input/output/cached + model 统计，单测 `stats_accumulate_gemini_per_model`）；`refresh_session_stats` 合并三工具按模型聚合；**Gemini 官方配额查询已实现**（`ns-usage::query_gemini_quota`：两步 Cloud Code RPC——`loadCodeAssist`（`cloudcode-pa.googleapis.com/v1internal:loadCodeAssist`，metadata `ideType:GEMINI_CLI`）→ `cloudaicompanionProject` 提取项目 ID → `retrieveUserQuota`（`v1internal:retrieveUserQuota`，project 字段）→ `buckets[].{modelId,remainingFraction,resetTime}` 按模型分类（Pro/Flash/FlashLite）取最低 remainingFraction 转已用百分比；`read_gemini_access_token`（`~/.gemini/oauth_credentials.json` 的 `access_token`/`tokens[].access_token`）；`gemini_quota_to_usage` 转 `Usage`；`query_usage` 对 Gemini 官方供应商自动走配额查询（curl 子进程，无 HTTP 依赖）；新增单测 `classifies_gemini_models`/`parses_rfc3339`/`quota_to_usage_takes_max`）；**仍缺**：配额真实验证需 OAuth 凭据（本机 gemini-api-key 模式无 token）；其余工具无本地会话日志数据源。
- [x] **P3-2 卡片/托盘配额脚注**：订阅配额、Coding Plan 额度、余额与重置倒计时显示在供应商卡片/托盘（cc-switch `UsageFooter`/各类 quota footer）——供应商卡片右侧额度列（150px：总量/剩余/已用 + **total/used 进度条**（pct≥0.9 朱色、≥0.7 玉色、其余金色，110×4px 圆角）+ reset 倒计时 `⏱ h:mm`）；用量页卡片（`render_usage_page`：总量/已用/剩余 + 来源）；托盘脚注（`tray_entries` 的 `usage_note`，`剩 N · ⏱ h:mm` 拼接到当前供应商行）。
- [x] **P3-3 定价与用量配置**：**自定义定价已实现**——`ns-store::Settings.custom_pricing`（模型→`{input_per_mtok,output_per_mtok,cache_read_per_mtok}`）+ `set_custom_price`/`remove_custom_price`；`ns-usage::cost_with_custom`（按前缀最长匹配覆盖内置价）；`refresh_session_stats` 用自定义价重算花费；设置页 Data 分区「自定义定价」卡片（表单添加/删除，单测 `custom_pricing_roundtrip`）；**models.dev 导入已实现**（`import_models_dev_pricing`：curl `models.dev/api.json` 拉取全量模型定价，`pricing.input`/`output`（USD/1M tokens）批量写入自定义定价表，设置页「导入 models.dev」按钮）；**用量脚本配置体验对齐**（`query_usage` 经 `ns_usage::detect_runtime`（Node/Deno/Bun）+ `execute_script` 执行，供应商 meta `usage_script` 路径，用量页「配置脚本」入口）。
- [x] **P3-4 会话多工具**：`ns-sessions` 新增 `SessionTool`/`list_sessions`，支持 **Claude + Codex** 列表（Codex 扫描 rollout jsonl、提取 session_meta/cwd/时间）；`parse_session` 自动识别 Claude/Codex，Codex best-effort 内容解析（user/assistant message、function_call/local_shell_call、tool output、event_msg user/agent + `reasoning` 内容项 / `error` 事件 / `agent_reasoning`，`build_rounds` 对 error 标记 failed）已单测；会话页增加 Claude/Codex 来源切换；`recover_command_for` 按工具跨平台生成恢复命令；**OpenCode 会话接入**（`list_opencode_sessions`/`parse_opencode_session` 读 `~/.local/share/opencode/opencode.db` 的 `session_v2`/`part`/`message` 表，JOIN 获取 role/model，list 仅列有 part 数据的会话；会话页来源切换含 OpenCode；`recover_command_for` 加 "opencode"→`opencode session`；新增 `opencode_no_db_returns_empty` 单测）；**Pi 会话接入**（`list_pi_sessions`/`parse_pi_session` 读 `~/.pi/agent/sessions/<project>/<ts>_<uuid>.jsonl`，`type=="session"` 头提取 id/timestamp/cwd，`type=="message"` 记录解析 `message.role`/`message.content[].text`，list 按 mtime 新→旧；会话页来源切换含 Pi；`recover_command_for` 加 "pi"→`pi --resume`；新增 `lists_and_parses_pi_sessions`/`pi_no_dir_returns_empty` 单测 + 本机真实数据验证）；**Hermes 会话接入**（`list_hermes_sessions`/`parse_hermes_session` 读 `~/.hermes/state.db` SQLite `sessions`/`messages` 表，`COALESCE(archived,0)=0` 过滤未归档会话，`sqlite3 -json` 子进程解析，`parse_hermes_session` 解析 role/content/tool_name + `is_tool_use` 标记；会话页来源切换含 Hermes；`recover_command_for` 加 "hermes"→`hermes resume`；新增 `hermes_no_db_returns_empty`/`hermes_sqlite_list_and_parse` 单测）；**OpenClaw 会话接入**（`list_openclaw_sessions` 调 `openclaw sessions --json` 官方接口解析 `sessions[]` 的 `sessionId`/`key`/`derivedTitle`/`label`/`workspaceDir`/`execCwd`/`model`/`updatedAt`/`archived`，CLI 不可用或未运行时返回空；`parse_openclaw_session` 调 `openclaw transcripts show <id> --json` 尽力解析 `messages`/`entries`/`turns` 数组 + `extract_openclaw_text` 提取 text/content/message 嵌套；会话页来源切换含 OpenClaw；`recover_command_for` 加 "openclaw"→`openclaw resume`；新增 `extracts_openclaw_text`/`openclaw_cli_lists_sessions` 单测——`openclaw_cli_lists_sessions` 本机真实调 CLI（`sessions --json` 返回 `sessions:[]` count=0 空数组）验证）；**标题/检索增强**（`session_search` 搜索框过滤 + 会话行显示 `title`（非空时优先于 id 前 8 位），`list_opencode_sessions` 提取 `title`）。**仍缺**：Gemini/OpenCode/Pi/Hermes 完整阅读细节；OpenClaw 会话阅读受限（本机 `~/.openclaw` 未初始化、`openclaw sessions --json` count=0 无会话数据，CLI 接口与字段解析经单测覆盖，真实阅读需用户产生会话后验证）。

### P4 — MCP / Skills / Prompts 产品化
- [x] **P4-1 MCP**：UI 已支持 SSE/WebSocket 传输选择与 env/headers 编辑（`mcp.rs`/`app.rs`）；**项目作用域选择器 UI 已实现**（`render_mcp_scope_dialog` 对话框 + `choose_scope_dir` macOS osascript 原生目录选择器回填路径 + `read_scope` 读取 `.mcp.json` 服务器列表 + 选择目录/关闭/读取按钮）；**多工具适配器全覆盖**（`ns-mcp::tools`：Claude/Codex/Gemini/OpenCode/GrokBuild/Kimi/Openclaw/Hermes/Pi/MiniMax/OMP 共 11 工具——`kimi.rs`（`~/.kimi-code/config.toml` `[mcp_servers.<name>]`，复用 codex）、`openclaw.rs`（`~/.openclaw/openclaw.json` `mcpServers`，复用 claude）、`hermes.rs`（`~/.hermes/config.yaml` `mcp_servers` YAML，serde_yaml）、`pi.rs`（`~/.pi/agent/mcp.json` `mcpServers`）、`minimax.rs`（`~/.minimax/mcp.json` `mcpServers`，`type` 显式 + `enabled` 持久化）、`omp.rs`（`~/.omp/agent/mcp.json` `mcpServers`）；`McpPaths` 各路径 + `config_for` 分支 + `read_tool`/`write_tool` 分支；MCP 页 `MCP_TOOLS` 扩为 11 工具；各适配器 roundtrip 单测）；深链导入确认/风险提示（P6-5 已实现）。**仍缺**：Agy 无 MCP 配置约定；项目作用域 UI 已就绪（多工具项目级扩展后续）。
- [x] **P4-2 Skills**：同步策略 UI（自动/仅符号链接/仅复制）、安装对话框 GitHub/本地目录来源切换（`install_from_dir`）、`allowed_tools` 表单编辑、已安装/发现视图页签、**检查更新**（`Skill.repo_url` 追踪 + `manager::check_for_updates` 浅克隆对比 + Skills 页「检查更新」按钮）、**全部更新**（`SkillsPanel::update_all`：对有 `repo_url` 技能逐个重新安装 + Skills 页「全部更新」按钮）、**`~/.agents/skills` 共享目录选项**（`Settings.shared_skills` + `SkillPaths.shared` + `skills_dir` 分支 + 设置页「共享 Skills 目录」开关 + 单测）、**发现页搜索**（GitHub API 仓库搜索 + 结果列表 + 每项「从 GitHub 安装」+ `urlencode` 单测）。
- [x] **P4-3 Prompts**：`ns-prompts::write_back` 写回前比对文件与库渲染，若不一致则提取未命名/手改残余回存再合并写回（保留两次写回间手工追加，避免整文件覆盖丢失）；**命名节内手工修改的精细合并**（`merge_edited_sections`：解析文件 `## <name>` 节，节名匹配库内启用条目且正文不同时回存手改正文到库，再提取未命名残余；`write_back_merges_edited_named_section_into_library` 单测）；新增/编辑对话框未保存更改守卫（`form_dirty`/`request_close`/`confirm_close` 浮层）；**Pi SYSTEM/APPEND 统一体验**（`PromptPaths::omp_user` → `~/.omp/agent/APPEND_SYSTEM.md`，`PI_CONFIG_DIR` 覆盖；`prompt_file`/`supported` 加 Pi；Prompts 页工具选择加 Pi；`pi_prompt_file_points_to_append_system` 单测）。

### P5 — 路由/聚合/账号/工作区增强
- [x] **P5-1 路由流式与可观测性**：请求日志查看入口已实现（路由页「请求日志」分区：汇总 + 工具/请求→实际模型/状态/耗时表格，`RoutingRuntime.request_log` 经 `with_log` 接线）；SSE 逐块实时转发已实现（`SseStreamConverter` 增量状态机 + `SseBodyStream`）；**熔断/故障转移状态展示已实现**（`RunningProxy.breakers` + `breaker_states` + 路由页「熔断/故障转移状态」分区）；**官方供应商路由边界规则已实现**（`enable_routing` 单供应商 `ProviderCategory::Official` 默认不走代理并提示「官方供应商默认直连」；例外 Codex OpenAI 官方（`codex-official`，认证经代理格式转换）；聚合模式允许官方上游，用户显式配置）。
- [x] **P5-2 托管账号真实流程**：`ns-switch::auth` 新增 **Copilot 真实凭据读取**（`~/.config/github-copilot/hosts.json` 的 `github.com.oauth_token`，`AUTH_TOOLS`/`credential_path`/`read_token` 三处扩展，新增 `copilot_token_hint_extracted`/`copilot_missing_file_not_logged_in` 单测）；账号页「CLI 工具凭据」列表显示 Copilot 已授权/未授权 + token 摘要；**OAuth device-code 流程已实现（参考 cc-switch `auth_start_login`/`auth_poll_for_account` 逻辑）**——`auth::start_device_flow`（xAI：OIDC discovery `auth.x.ai/.well-known` → device authorization；Codex/ChatGPT：`auth.openai.com/oauth/device/code` JSON `{client_id}`；**Copilot：GitHub device flow**——`github.com/login/device/code` form `{client_id:Iv1.b507a08c87ecfe98, scope:read:user}` → `github.com/login/oauth/access_token` 轮询）+ `DeviceCodeSession::poll`（轮询 token 端点，处理 `authorization_pending`/`slow_down`/`access_denied`/`expired_token`，返回 access_token）+ `build_session`/`urlencode`/`curl_*` 辅助；账号页「登录」按钮触发真实流程（xai→xai、copilot→copilot、chatgpt→codex 映射；展示 user_code/verification_uri/expires_in + OAuth 等待横幅），`start_oauth_poll` 后台轮询成功后 **`login_account` 落盘真实 token**（`save_oauth_token`：codex→`~/.codex/auth.json` 的 `tokens.access_token`（保留其他字段）、xai→`~/.xai/credentials.json`、copilot→`~/.config/github-copilot/hosts.json` 的 `github.com.oauth_token`；建托管供应商）；新增 `build_session_parses_xai_device_response`/`build_session_parses_codex_device_response`/`poll_before_interval_returns_pending`/`urlencode_encodes_reserved` 单测 + `hosted_account_creates_provider`（临时 HOME 验证 token 落盘）。**验证受限**：真实 OAuth 需浏览器交互授权（本机无凭据，未真实验证；协议/状态机经单测覆盖）。
- [x] **P5-3 OpenClaw/Hermes 面板化**：`toolconfig` 页新增 **OpenClaw 配置**（`openclaw.json`：env / tools / agents / workspace 全文编辑，TextArea 编辑器复用）+ **健康横幅**（OpenClaw `openclaw.json` 存在性 + JSON 合法性校验 + Hermes `config.yaml` 存在性，状态点玉/朱/弱色 + 文案）；原有 OpenClaw AGENTS.md/SOUL.md + Hermes MEMORY.md/USER.md 保留。
- [x] **P5-4 项目与 Profile 语义对齐**：`apply_project` 已自动回存 `active_project`；**快照范围扩展已实现**（`ProjectSnapshot.extras` 载体 + `save_project`/`apply_project` 携带 extras + app 层 `project_extras`/`apply_project_extras`：保存收集 MCP/Skills/Prompts 状态，应用恢复覆盖对应工具配置）；**工作区快照已实现**（`project_extras` 的 `workspace` key：OpenClaw `openclaw.json` + Hermes `config.yaml` 全文备份，`apply_project_extras` 恢复写回）；**cc-switch Profile 语义对齐已实现**（`Config.active_projects: BTreeMap<Scope, String>` 每工具组独立当前项目，`scope_for`（Claude+ClaudeDesktop 同组 "claude"，其余各自 `id_str()`）+ `active_project_for`（`active_projects` 优先，兼容旧 `active_project` 回退 Claude 组）；`save_project(name, scope, extras)`/`apply_project(name, scope, current_extras)` 按 scope 设置/回存；项目页/顶部切换器按当前工具 scope 工作）。

### P6 — 跨平台、分发与运维
- [~] **P6-1 Windows/Linux 自启**：已实现跨平台实现：macOS launchd、Linux `~/.config/autostart/*.desktop`、Windows `HKCU\...\Run`（`reg`）。macOS 路径/逻辑本机编译验证 + **plist 写入真实验证**（`plutil -lint` 校验 XML 合法、`is_enabled` 检测）；**Linux/Windows 分支逻辑经纯函数单测覆盖**（`linux_desktop_content`（`[Desktop Entry]`/`Exec`/`X-GNOME-Autostart-enabled`/`Terminal=false` 断言）、`win_reg_add_args`/`win_reg_delete_args`（`reg add/delete` 参数序：key、`/v` 名、`/t REG_SZ`、`/d exe`、`/f` 断言），enable/disable 分支改用纯函数）；**验证受限**：Windows/Linux 实际注册需对应平台运行（本机 macOS 交叉编译被第三方 `ring` 依赖阻止，平台分支逻辑经纯函数单测覆盖）。
- [x] **P6-2 WSL 工具管理（Windows）**：`ns-tools` 新增 `wsl_tools()`（`#[cfg(target_os="windows")]` 探测 `wsl.exe --status` + `wsl.exe which <bin>` / `wsl.exe <bin> --version` 读取 WSL 发行版内工具路径/版本；非 Windows 返回空）+ `wsl_available`/`wsl_run` 辅助 + 单测 `wsl_tools_empty_on_non_windows`（macOS 编译验证）。**验证受限**：真实 WSL 探测需 Windows 平台运行。
- [x] **P6-3 更新体验**：自动检查/定时（启动 3 秒首次 + 每 6 小时循环）、后台提示（设置页 `update_status`）、更新后说明（WhatsNew 浮层，`render_whats_new` + `whats_new_visible`）；版本比较健壮化（`is_newer`/`parse_version`）。
- [x] **P6-4 云同步增强**：S3 兼容存储已实现（`s3_push`/`s3_pull` SigV4 签名）；**冲突处理策略已实现**——`sync_pull` 覆盖前调用 `backup_sync_conflict(config_path, local)`：本地 config 与远端序列化不同时自动备份本地到 `~/.ns-switch/backups/sync/config-<ts>.json`（纯函数便于单测，新增 `backup_created_when_local_differs`/`no_backup_when_identical` 单测，ns-store 19 通过）；**定时/自动同步已实现**——`SwitchApp::new` 后台循环（`auto_sync_minutes` 0=关闭，否则每 N 分钟若 `sync.enabled` 调 `sync_push`，失败写 `sync_status`）+ 设置页云同步分区（WebDAV/S3 provider 切换、`sync_interval` 间隔输入「自动同步间隔（分钟，0=关）」、保存/推送/拉取按钮 + `sync_status` 反馈）。
- [x] **P6-5 深链产品化**：分资源确认对话框已实现（`deeplink_pending` + `render_deeplink_confirm` + 脱敏摘要 `deeplink_summary` + 风险提示 + 确认/取消/Esc）；**图标随深链导入携带**（`build_provider` 从预设 `icon`/`icon_color` 填充，无需额外拉取）；**Coding Plan 识别**（`is_coding_plan` 名称关键词派生 + 供应商卡片「Coding Plan」徽章 + 单测）；**导入后聚焦/定位已实现**——`execute_deeplink` 各导入分支确认执行后切换页面（ImportProvider→Claude 供应商页 `Page::Providers`、ImportMcp→`Page::Mcp`、ImportSkill→`Page::Skills`、ImportPrompt→`Page::Prompts`）。

### P7 — 界面打磨
- [x] **P7-1 首次运行/欢迎页/引导**：首次运行全屏引导（`welcome_visible` + `render_welcome`，`Store::set_welcome_dismissed` 持久化）；**各页空态统一视觉化引导**（图标+标题+说明：供应商/会话/MCP/Skills/Prompts 五页均已覆盖，与供应商页既有空态风格一致）。
- [x] **P7-2 未保存更改统一守卫**（表单/编辑器离开提醒）：通用供应商对话框（`dialog_dirty` + `render_dialog_confirm_close`）、Prompts（`form_dirty`/`request_close`/`confirm_close`）、Skills（`confirm_close` + `form_dirty` + `request_close` + `render_skill_confirm_close`）均已实现，Esc/取消接入。
- [x] **P7-3 快捷键完善**：cmd-1..9 页面导航、cmd-f 搜索聚焦、Esc 关闭对话框/菜单、**cmd-s 真实保存**（`save_current`：供应商添加/编辑对话框 → `dialog::save_provider`、Prompts/Skills/MCP 对话框 → 各自 `save`、工具配置编辑器 → `save_toolconfig`；无打开表单时状态栏提示「当前无可保存的表单」）；**cmd-r 真实刷新**（`refresh` 按页分发：用量页 `refresh_session_stats` + 全部工具 `query_usage`、会话页 `load_sessions`、其余页重绘重读，状态栏反馈「用量/会话/已刷新」；消除原「仅设就绪不重载」占位）；**快捷键提示已补全**（`render_shortcut_hint` 按页给出操作提示 + 全局键 ⌘1-9/⌘S/⌘R/Esc，状态栏显示，14 页全覆盖）。
- [x] **P7-4 连接状态/日志配置/网络高级设置**等设置页细节对齐：网络分区「连接状态」区块（各路由工具监听地址/端口 + 运行状态 pill）已实现；**全局出站代理已实现**（`Settings.global_proxy` + `set_global_proxy`；`proxy_env` 模块：`curl_env`（`HTTPS_PROXY`/`HTTP_PROXY`/`ALL_PROXY` + `NO_PROXY=127.0.0.1,localhost` 本地直连）、`test_proxy`（经代理请求多公共端点）、`scan_local_proxies`（8 常见端口 7890/7891/1080/8080/8888/3128/10808/10809）；`SwitchApp` 字段 `global_proxy_url`/`proxy_status`/`proxy_test`/`proxy_scan` + `save_global_proxy`（存 settings + 应用/清除进程级代理环境）/`test_global_proxy`/`scan_proxies`；网络分区替换占位为真实表单（URL 输入 + 保存/测试/扫描按钮 + 扫描结果点击回填 + 状态/延迟反馈 + 已配置/未配置 pill），启动回填当前代理并应用进程环境；5 单测（`curl_env_sets_proxy_vars`/`curl_env_empty_means_direct`/`curl_env_socks5_passthrough`/`scan_common_ports_format`/`test_proxy_empty_url_fails`）；ns-switch 自身出站请求（用量/GitHub/models.dev/更新检查）经代理，本地路由代理不受影响；消除「全局代理预留」占位）。

### 验收记录
- 2026-05-08：新增第 10 章（对比 cc-switch 差距清单，P1–P7）；完成 **P1-1 主题实际应用**（`cargo check --workspace` 0 错误/0 项目警告，`cargo test --workspace` 全绿）。
- 2026-05-08（续）：完成 **P2-1 连通性测试**；**P2-2 部分完成**（官网/终端打开配置目录/复制供应商/导出单个供应商 + 菜单回调重构）；**P2-3 部分完成**（上游 API 格式选择与 `meta.apiFormat` 持久化、预览文案、对话框关闭/取消修复）。`cargo check/test --workspace` 全绿。
- 2026-05-08（续2）：**P2-4 部分完成**（卡片显示 iconColor/备注或域名/`apiFormat` 芯片）；校正 **P4-1/P4-2** 现状（MCP 已有 SSE/Ws+env/headers，Skills 已有同步策略 UI）与 **P5-4/M11**（`apply_project` 自动回存已实现）。`cargo check/test --workspace` 全绿。
- 2026-05-08（续3）：**P2-5 增强**（Codex 聚合 daemon/CLI 完全重启强提示）；**P4-3 部分修复**（`ns-prompts` 写回捕获手工追加并回存，新增 `write_back_captures_manual_edits_between_writes` 测试）；**P6-1 部分完成**（`autostart.rs` 支持 Linux/Windows，macOS 编译验证通过）。`cargo check --workspace` 0 错误/0 项目警告，`cargo test --workspace` 全绿。
- 2026-05-08（续4）：**P3-4 部分完成**（`ns-sessions` 支持 Claude+Codex 会话列表/元数据解析，UI 增加来源切换，`recover_command_for` 跨平台；新增 `parses_codex_rollout_meta`/恢复命令测试）。`cargo check --workspace` 0 错误/0 项目警告，`cargo test --workspace` 全绿。
- 2026-05-08（续5）：**P3-4 增强**（`ns-sessions::parse_session` 自动识别 Claude/Codex；新增 `parse_codex_session` best-effort 内容解析：`event_msg` user/agent、`response_item` user/assistant message、`function_call`/`local_shell_call`、`function_call_output`/`custom_tool_call_output`；新增 `parses_codex_session_content` 单测，`ns-sessions` 8 项测试全绿）。**P2-5 增强**（供应商页切换后冲突横幅：`SwitchApp.switch_conflicts` 由 `check_conflicts` 填充，可忽略；切换工具清空）。`cargo check --workspace` 0 错误/0 项目警告，`cargo test --workspace` 全绿。**未完成/受限项**：P2-5 启动主动扫描/写入前确认对话框；P3-1/P3-4 Codex 完整阅读/推理/错误细节 + Gemini/OpenCode/Pi/Hermes/OpenClaw 解析与用量；P4-1 MCP 项目作用域 UI 与多工具适配器；P4-2 Skills 本地安装/GitHub 子目录/`allowed_tools` 表单核对/更新检查；P5-1 SSE 逐块实时+请求日志 UI；P5-2 真实 OAuth（需交互授权）；P5-3 OpenClaw/Hermes 完整面板；P6-2 WSL（需 Windows）；P6-3 自动调度/WhatsNew；P6-4 S3+冲突合并；P7 全量打磨。均需按平台/凭据/协议分迭代真实实现验证，禁止虚报完成。
- 2026-11-09：**界面一致性与功能补齐**（对比 cc-switch）。布局：侧栏 200px + Apps/Settings 底部栏、顶栏 44px、供应商卡片 58px 统一、各页统一 `AppPageHeader`。供应商页：模式页签（直连/路由/聚合）、卡片状态文本/健康芯片/用量脚注、⌘F 搜索面板。设置页：分区导航（通用/应用配置/路由/网络/数据/关于），新增「应用配置」每工具配置目录与关键能力概览、「路由」监听地址/端口 + 每工具故障转移、「网络」全局代理概览。Skills 页：已安装/发现视图页签。MCP 页：多工具矩阵/同步。`cargo check --workspace` 0 错误/0 项目警告，`cargo test --workspace` 175 通过/0 失败。
- 2026-11-09（续）：**功能补齐与打磨**（P7-1/P7-3/P4-2/P4-3/P4-1）。快捷键：cmd-7/8/9 页面导航（Page::ALL 第 7-9 项）、Esc 统一关闭对话框/菜单（含 MCP/Skills/安装/Prompts）、cmd-s 保存占位。欢迎页：首次运行全屏引导（`settings.welcome_dismissed` + `Store::set_welcome_dismissed`，`SwitchApp.welcome_visible`/`render_welcome`/`dismiss_welcome`）。Skills：安装对话框增加「GitHub 仓库/本地目录」来源切换（`SkillInstallSource`、`install_local_dir`、`SkillsPanel::install_local` 调用 `manager::install_from_dir`）；`allowed_tools` 表单编辑已确认。MCP：页头新增「项目作用域」按钮（打开 `~/.claude.json` 项目作用域 MCP）。Prompts：新增/编辑对话框未保存更改守卫（`PromptsPanel::form_dirty`/`request_close`/`confirm_close` + `render_prompt_confirm_close` 浮层，Esc/取消触发）。`cargo check --workspace` 0 错误/0 项目警告，`cargo test --workspace` 175 通过/0 失败。
- 2026-11-09（续2）：**请求日志与可观测性 / 会话解析增强**（P5-1 部分 / P3-4 增强）。请求日志：`RoutingRuntime` 新增共享 `RequestLog`（`Arc`），`ensure_running_with` 经 `ProxyConfig::with_log` 接线（每代理写入）；路由页新增「请求日志」分区（`render_request_log_section`）：汇总（总条数/成功）+ 表格（工具 / 请求模型→实际模型 / 状态 / 耗时，新→旧，取 200 条）。会话：`ns-sessions` Codex 解析增加 `reasoning` 内容项、`error` 事件（`role="error"`）与 `agent_reasoning`；`build_rounds` 对 `role=="error"` 标记 `failed`；新增 `parses_codex_errors_and_reasoning` 单测。工具页：新增「全部升级」按钮（对已安装工具逐个 `install_cli`）。SSE 全缓冲转换路径已核对（`stream_sse_to_anthropic` 产生合法 Anthropic SSE 序列，ns-proxy 34 测试全绿；逐块实时转发仍为后续增强）。`cargo check --workspace` 0 错误/0 项目警告，`cargo test --workspace` 176 通过/0 失败。
- 2026-11-09（续3）：**界面布局对齐 cc-switch**（P7 界面打磨）。侧栏重构为 cc-switch v7 布局：顶条 36 + 应用列表滚动区（13 工具，每项模式标签「路由/聚合」+ 用量尾注）→ 分隔线 → 全局页面项 11 项贴底（路由/工具/MCP/Skills/Prompts/用量/会话/项目/账号/通用/工具配置）→ 分隔线 → 底栏（应用/设置）。供应商页页头对齐 `AppPageHeader`：高 52，左工具图标+标题+副标题（供应商）+切换/共存 pill，右操作区（导出/导入/添加主按钮）。供应商卡片：左拖拽手柄（⠿）+ 图标 + 名称/芯片（当前/API 格式/故障转移）+ 副标题，右侧固定宽 150 额度列（剩余/重置倒计时）+ 主操作位（使用中状态/切换按钮）+ ⋯ 菜单；当前卡片强调色边框+淡底（对齐 CardTone）。列表首行：分区标题（全部供应商）+ 搜索框右端。移除顶栏遗留伪「＋ 添加供应商」按钮（原只设 status 占位，点击无真实行为）；新增 `page_glyph` 辅助；修复 `render_whats_new` 缺失（更新说明浮层完整实现）+ `store` move 编译错误。`cargo check --workspace` 0 错误/0 项目警告，`cargo test --workspace` 176 通过/0 失败；应用启动无 panic/警告日志。**验证受限**：GPUI 为自绘 canvas，accessibility 树仅窗口控件，界面视觉布局需屏幕录制权限（当前 denied，无法截图核对），代码级验证已覆盖。
- 2026-11-09（续4）：**状态栏语义色与操作反馈**（P7 界面打磨）。新增 `StatusTone` 枚举（Neutral/Jade/Danger）+ `SwitchApp.status_tone` 字段；`render_statusbar` 节点与文字按 tone 着色（中性灰→就绪，玉色→成功，朱色→错误）。关键动作路径接线：`write_provider`（切换）成功/失败、`export_single_provider`（导出）、复制/删除供应商、对话框保存（add/edit）、导入操作均设置对应 `status_tone`；移除未使用的 `status_flash_until` 与 `label()` 保持零警告。`cargo check --workspace` 0 错误/0 项目警告，`cargo test --workspace` 176 通过/0 失败。
- 2026-11-09（续5）：**P2-5 启动时主动扫描冲突**（界面打磨）。新增 `startup_conflicts` 字段；`SwitchApp::new` 末尾添加异步后台任务，遍历所有工具调用 `check_conflicts` 并将结果写入该字段。供应商页横幅由原仅判断 `switch_conflicts` 扩展为同时判断两者，「忽略」按钮同时清空两个列表。代码层面完成，实际效果依赖各工具客户端配置文件存在性与内容。`cargo check --workspace` 0 错误/0 项目警告，`cargo test --workspace` 176 通过/0 失败。**验证受限**：GPUI 自绘 canvas + accessibility 无窗口内容 + 屏幕录制权限 denied，无法截图核对；需用户手动运行 app 确认横幅显示与忽略交互。
- 2026-11-09（续6）：**P7-1 多页空态增强**（界面打磨）。为会话页、MCP 页、Skills 页三处空态添加视觉化引导：图标（☰/⛁/✦）+ 标题 + 操作说明；会话页按当前选择工具动态切换图标与提示文案；MCP/Skills 保留「添加」按钮入口的上下文说明。`cargo check --workspace` 0 错误/0 项目警告，`cargo test --workspace` 176 通过/0 失败。**验证受限**：GPUI 自绘 canvas，屏幕录制权限 denied，无法截图核对；需用户手动启动 app 确认各页空态显示效果。
- 2026-11-09（续7）：**P7 界面打磨收尾**。新增 `render_shortcut_hint` 在状态栏动态显示当前页快捷键提示；为会话页/MCP 页/Skills 页三处空态统一添加视觉化引导（图标+标题+说明），与供应商页既有空态风格一致。至此 P7-1（空态）、P7-3（快捷键可发现性）均已完成代码实现，零警告零错误。`cargo check --workspace` 0 错误/0 项目警告，`cargo test --workspace` 176 通过/0 失败。**验证受限**：GPUI 自绘 canvas，屏幕录制权限 denied，需用户手动启动 app 核对各页空态与状态栏着色效果。
- 2026-11-09（续8）：**顶栏简洁化 + 表单守卫（P7-2）**。顶栏移除中文标题「商量切换器」与副标题，改为简洁品牌标识：`brand_mark(22)` + 「ns-switch」文字（`render_topbar` 简化，移除未使用 `lang` 变量）；修复左上角视觉重叠（原 26px 品牌 + 双行标题 + 项目切换按钮并排挤压）。新增 `SwitchApp.dialog_dirty` 字段 + `dialog_is_dirty(cx)` 方法（比对 `dialog_fields` 快照与当前输入），`escape_dialog` 与对话框「✕」关闭按钮接入守卫：有未保存修改时显示确认浮层（`render_dialog_confirm_close`：继续编辑 / 放弃更改），而非直接关闭。`cargo check --workspace` 0 错误/0 项目警告，`cargo test --workspace` 176 通过/0 失败。**验证受限**：GPUI 自绘 canvas，屏幕录制权限 denied，需用户手动启动 app 确认顶栏显示与守卫浮层交互。
- 2026-11-09（续9）：**P5-1 SSE 逐块实时转发**（本地路由流式）。`ns-proxy` 新增 `SseStreamConverter`（增量 SSE→Anthropic 状态机：内部缓冲跨块拼合 `data:{...}\n\n` 帧，每解析一个完整帧即转换为 Anthropic 事件输出，`finish()` 补发 `message_stop`）+ `SseBodyStream`（实现 `Stream<Item=Result<Frame<Bytes>,io::Error>>`：逐帧读取上游 `Incoming` body，经转换器增量输出，避免全缓冲阻塞）。`handle` 的 SSE 分支改为流式路径（`sse_stream_response` 返回 `StreamBody`），`ResBody` 改为 `BoxBody<Bytes, Box<dyn Error+Send+Sync>>` 以支持流式与全缓冲统一。全缓冲 `stream_sse_to_anthropic`/`push_sse_frame`/`sse_response` 移为 `#[cfg(test)]` 或删除。新增 `sse_stream_converter_incremental_chunks`/`sse_stream_converter_single_chunk` 单测。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 178 通过/0 失败（ns-proxy 36 全绿）。**验证受限**：流式逐块转发需真实上游供应商 SSE 端点核对，本地单测覆盖转换逻辑；实时转发路径需集成测试验证。
- 2026-11-09（续10）：**P4-1 MCP 项目作用域选择器**（统一 MCP 面板）。`McpPanel` 新增 `scope_open`/`scope_path`/`scope_result` 字段 + `open_scope`/`read_scope` 方法：输入项目目录路径，读取该项目的 `.mcp.json`（Claude Code 项目作用域约定），解析 `mcpServers` 并列出服务器名。MCP 页头「项目作用域」按钮改为打开选择对话框（原直接打开 `~/.claude.json`），`render_mcp_scope_dialog` 渲染对话框（路径输入 + 读取结果 + 关闭/读取按钮），Esc 关闭接入。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 178 通过/0 失败。**验证受限**：GPUI 自绘 canvas，需用户手动启动 app 输入真实项目目录核对 `.mcp.json` 读取结果。
- 2026-11-09（续11）：**P3-2 托盘配额脚注**（用量可观测性）。`ToolMenuEntry` 新增 `usage_note: Option<String>` 字段；`tray_entries` 加 `usage` 参数（`BTreeMap<String, Usage>`），为每工具当前供应商提取用量摘要（剩余/总量 + 单位/货币 + 重置倒计时 `⏱ h:mm`）；`plan_menu` 的 summary 头把用量脚注拼接到当前供应商行（`供应商 → 名称（路由中）　剩 100 · ⏱ 5h00m`）。`refresh_tray_menu` 传 `&self.usage`，`build_tray` 传空 map（启动时无用量）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 178 通过/0 失败。**验证受限**：托盘菜单为原生系统菜单，需用户手动启动 app 查询用量后核对托盘显示。
- 2026-11-09（续12）：**P3-1 用量多工具（Codex 会话日志）**。`ns-usage::session` 新增 `scan_codex_sessions`（扫描 `~/.codex/sessions/**/rollout-*.jsonl` 递归收集，按模型统计）+ `collect_jsonl_files`（递归收集 jsonl）+ `scan_codex_file`（解析 `response_item` type=message、role=assistant 的 `usage`/`model`）+ 单测 `stats_accumulate_codex_per_model`。`SwitchApp::refresh_session_stats` 扩展为同时扫描 Claude + Codex，合并同模型统计并按请求数排序，花费按合并 token 重算。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 179 通过/0 失败。**验证受限**：Codex 会话文件需实际存在 `~/.codex/sessions`，本地单测覆盖解析逻辑。
- 2026-11-09（续13）：**P6-3 自动更新定期调度**（更新体验）。`SwitchApp::new` 的后台自动检查从单次改为循环：启动后 3 秒首次检查，之后每 6 小时循环；发现新版写入 `update_status`（设置页显示），避免仅启动一次。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 179 通过/0 失败。**验证受限**：6 小时周期难以本地验证，代码级循环逻辑已核对。
- 2026-11-09（续14）：**P7-4 网络分区连接状态概览**（设置页细节）。`render_settings_network` 增强：新增「连接状态」区块，列出每个支持路由的工具（`supports_aggregation`）的监听地址/端口（运行中读实际端口，否则配置值）+ 运行状态 pill（运行中玉色 / 未运行弱色）；保留「全局代理」占位与说明（本地路由代理在「路由」分区配置）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 179 通过/0 失败。**验证受限**：GPUI 自绘 canvas，需用户手动启动 app 启动路由代理后核对网络分区状态显示。
- 2026-11-09（续15）：**P4-2 Skills 检查更新/全部更新**（GitHub 来源追踪）。`ns-skills`：`Skill` 模型新增 `repo_url: Option<String>`（向后兼容 `#[serde(default)]`）；`install_from_dir` 加 `repo_url: Option<&str>` 参数（本地安装传 `None`，GitHub 安装传仓库 URL），`render_skill`/`parse_skill_file` 经 frontmatter 键 `ns-repo-url` 往返持久化（跨写读保留）；新增 `manager::check_for_updates(skills)`：对每个 `repo_url` 非空技能浅克隆最新仓库，解析 `skills/<name>/SKILL.md` 与当前 `body`/`description` 对比，不同则标记有更新，返回更新技能名列表；新增单测 `check_for_updates_detects_change`（本地 git 仓库模拟：初始一致判无更新、改描述后判有更新）。UI：Skills 页头新增「检查更新」按钮（`SkillsPanel::check_updates` 调 `manager::check_for_updates`，结果写 `write_msg`）。同步修正 `split_meta`/`split_frontmatter` 的 body 提取偏移（`\n---\n` 分隔 5 字节，此前 `+4` 导致前导换行累积、误判更新）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 180 通过/0 失败。**验证受限**：检查更新需真实 GitHub 仓库网络（单测用本地 git 仓库覆盖对比逻辑）。
- 2026-11-09（续16）：**P6-3 更新后说明（WhatsNew）接线确认**（更新体验）。确认 `render_whats_new` 完整实现并接入渲染（`whats_new_visible` = 上次 `last_seen_version` 非空且 ≠ 当前版本时显示）：卡片显示「更新到 vX」+ 上次版本 + 三条更新要点（界面布局对齐 cc-switch / 供应商卡片增强 / 切换引擎稳定），「开始使用」按钮关闭（`whats_new_visible = false`）。此前 `render_whats_new` 定义缺失导致编译错误，经核对为历史半成品已存在于文件，接线完整后 `cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 180 通过/0 失败。**验证受限**：WhatsNew 需上次版本 ≠ 当前版本才显示（手动改 `last_seen_version` 或升级版本号后启动核对）。
- 2026-11-09（续17）：**P7-2 Skills 表单未保存守卫**（未保存更改统一守卫）。`SkillsPanel` 新增 `confirm_close: bool` 字段 + `form_dirty(cx)`（新增：任一字段非空即 dirty；编辑：与打开时技能对比 name/description/body/allowed_tools/enabled）+ `request_close(cx)`（dirty 则弹确认浮层，否则直接关）；`open_add`/`open_edit`/`save` 复位 `confirm_close`。UI：`render_skill_confirm_close` 浮层（继续编辑 / 放弃更改），Skills 对话框「取消」按钮与 Esc 均改走 `request_close`。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 180 通过/0 失败。**验证受限**：GPUI 自绘 canvas，需用户手动启动 app 编辑技能后修改再取消核对浮层出现。
- 2026-11-09（续18）：**P5-1 熔断/故障转移状态展示**（路由可观测性）。`ns-switch::routing`：`RunningProxy` 新增 `breakers: Arc<Mutex<HashMap<usize, CircuitBreaker>>>`（`ensure_running_with` 从 `config.breakers` clone 共享），新增 `RoutingRuntime::breaker_states(tool)` 返回各上游熔断状态（index → `CircuitState`，按 index 排序；未运行返回空）。UI：路由页新增「熔断 / 故障转移状态」分区（`render_breaker_section`），列出 Claude/Codex/Gemini/GrokBuild 每工具熔断状态 pill（上游N 关闭/熔断/半开，玉/朱/危险色；未运行/无上游记录显示弱色）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 180 通过/0 失败。**验证受限**：熔断状态需真实上游失败触发（本地测试经 failover 单测覆盖断路器逻辑，UI 需启动路由代理后上游不可达观察）。
- 2026-11-09（续19）：**P6-5 深链产品化·分资源确认对话框**（深链导入确认/风险提示）。`SwitchApp` 新增 `deeplink_pending: Option<(DeepLinkAction, String)>`（action + 脱敏摘要）；`apply_deeplink` 改为暂存待确认（不再直接执行），新增 `execute_deeplink`（原导入逻辑：ImportProvider/SwitchProvider/ImportRepo/ImportMcp/ImportSkill/ImportPrompt）与 `Self::deeplink_summary`（脱敏摘要：key/命令掩码 `xxxx…xxxx`、正文/内容显示字数、供应商端点/模型）。UI：`render_deeplink_confirm` 确认浮层（摘要 + 风险提示「导入会写入本机配置文件，请确认来源可信」+ 取消/确认导入），主渲染接入；Esc 取消。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 180 通过/0 失败。**验证受限**：深链确认浮层需真实 `nsswitch://` 触发（手动启动 app 传 URL 或单实例转发核对浮层出现与确认/取消行为）。
- 2026-11-09（续20）：**P4-2 `~/.agents/skills` 共享目录选项**。`ns-store`：`Settings` 新增 `shared_skills: bool`（`#[serde(default)]` 向后兼容）+ `set_shared_skills`；`ns-skills`：`SkillPaths` 新增 `shared: bool`（`from_env` 默认 false），`tools::skills_dir` 在 `shared=true` 时所有工具走 `~/.agents/skills`（否则各自目录）；新增单测 `shared_dir_routes_all_tools`。UI：设置页通用分区新增「共享 Skills 目录」开关（读/写 `Settings.shared_skills`，状态栏反馈）；`SkillsPanel` 新增 `shared` 字段 + `paths(cx)` 按设置构造 SkillPaths（load/刷新前由 app 同步），全部读写/安装/同步/检查更新路径经 `paths`。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 181 通过/0 失败。**验证受限**：共享目录实际落盘需用户开启开关后安装/同步技能核对 `~/.agents/skills` 下目录。
- 2026-11-09（续21）：**P4-2 发现页搜索（GitHub 仓库）**。`SkillsPanel` 新增 `discover_query`（TextField）/`discover_results`（Vec<(full_name, desc)>）/`discover_status` 字段 + `search_discover(cx)`：调用 `api.github.com/search/repositories?q=<query>+skill&sort=stars&per_page=10`（curl，8 秒超时），解析 `items[]` 的 `full_name`/`description` 取前 10，状态反馈（搜索中/结果数/无结果/失败）；新增文件级 `urlencode`（`%XX`，空格→`+`，保留 `-_.~`）+ 单测 `urlencode_encodes_reserved`。UI：发现页改为搜索框 + 搜索按钮 + 结果列表（仓库名/描述 + 「从 GitHub 安装」按钮，复用 `install_from_github` 安装到 Claude/Codex/Gemini，按共享目录设置）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 182 通过/0 失败。**验证受限**：GitHub API 搜索需网络（urlencode 单测覆盖编码；真实搜索需用户在有网环境输入关键词核对结果与安装）。
- 2026-11-09（续22）：**P4-3 命名节手改精细合并**。`ns-prompts::tools` 新增 `merge_edited_sections(tool, store, existing)`：解析文件 `## <name>` 节，节名匹配库内启用的命名条目且正文不同（`trim` 比较）时把手改正文回存到库（`save_entry` 保留手改 + 更新 `updated_at`），在 `write_back` 不一致分支中先于 `extract_unmanaged_body` 调用——手改命名节不再被当作未命名残余提取。新增单测 `write_back_merges_edited_named_section_into_library`（首次写回 → 手改节正文 → 二次写回 → 断言库回存手改正文、文件含手改正文且旧正文不残留）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 183 通过/0 失败（ns-prompts 7 全绿）。**验证受限**：手改合并路径需用户在两次写回间手改命名节正文后核对库与文件。
- 2026-11-09（续23）：**P6-5 Coding Plan 识别**。`ns-switch` 新增文件级 `is_coding_plan(name)`（名称关键词派生：`coding plan`/`token plan`/`coding-plan`/`token-plan` 小写包含判断，不新增字段）+ 供应商卡片芯片区新增「Coding Plan」徽章（`theme::cinnabar` 朱色）；`build_provider` 已从预设 `icon`/`icon_color` 填充（深链导入随 preset 携带图标，P6-5 图标拉取覆盖）。新增单测 `coding_plan_detection`（火山 Coding Plan/Tencent Token Plan/compshare-coding-plan 真，88API/OpenRouter 假）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 184 通过/0 失败。**验证受限**：徽章显示需用户手动启动 app 查看 Coding Plan 供应商卡片（名称含关键词即显示朱色徽章）。
- 2026-11-09（续24）：**P4-1 OpenCode MCP 适配器**（多工具适配器扩展）。`ns-mcp` 新增 `tools/opencode.rs`：`read_servers`（读 `opencode.json` 的 `mcp` 对象）、`from_json`（`type` local/remote、`command` 数组拆分为 command+args、`enabled` 持久化）、`to_json`（local 写 `command` 数组、remote 写 `url`/`headers`、停用写 `enabled:false`）、`apply_servers`（原地增删改，保留其他键）、`validate_doc`/`load`；单测 `roundtrip_local_and_remote`（local/remote/disabled 三服务器往返 + 删除保留其他键）。`McpPaths` 新增 `opencode_config`（`~/.config/opencode/opencode.json`，`OPENCODE_CONFIG_DIR` 覆盖）+ `config_for` 分支；`manager::read_tool`/`write_tool` 加 OpenCode 分支（JSON 读写复用 `read_json_or_empty`/`write_json`）。UI：MCP 页 `MCP_TOOLS` 扩为 `[Tool; 4]`（Claude/Codex/Gemini/OpenCode），页签自动出现。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 185 通过/0 失败（ns-mcp 9 全绿）。**验证受限**：OpenCode 真实配置读写需用户有 `~/.config/opencode/opencode.json`（单测覆盖 JSON 投影逻辑）。
- 2026-11-09（续25）：**P4-1 Grok Build MCP 适配器**（多工具适配器扩展）。`ns-mcp` 新增 `tools/grok.rs`（薄封装复用 `codex` 的 TOML 投影：`read_servers`/`apply_servers`/`load`，`~/.grok/config.toml` 的 `[mcp_servers.<name>]` 表约定，与 Codex 同为 OpenAI 系 CLI）；单测 `grok_reuses_codex_table_semantics`（stdio 服务器往返 + 保留其他键 + 停用保留）。`McpPaths` 新增 `grok_config`（`~/.grok/config.toml`，`GROK_HOME` 覆盖）+ `config_for` 分支；`manager::read_tool`/`write_tool` 加 GrokBuild 分支（TOML 读写复用 `read_toml_or_empty`/`write_toml`）。UI：MCP 页 `MCP_TOOLS` 扩为 `[Tool; 5]`（Claude/Codex/Gemini/OpenCode/GrokBuild）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 186 通过/0 失败（ns-mcp 10 全绿）。**验证受限**：Grok 真实配置读写需用户有 `~/.grok/config.toml`（单测覆盖 TOML 投影逻辑）。
- 2026-11-09（续26）：**P5-3 OpenClaw/Hermes 面板化**（toolconfig 增强）。`ns-switch::toolconfig::files` 新增 `openclaw-config` 条目（`~/.openclaw/openclaw.json`：env / tools / agents / workspace 全文编辑，TextArea 编辑器复用，保存即创建）。`render_toolconfig_page` 标题下新增健康横幅：OpenClaw `openclaw.json` 存在性 + `serde_json` 合法性校验 + Hermes `config.yaml` 存在性 → 状态点（玉色=两者有效/存在、朱色=JSON 解析失败、弱色=未初始化）+ 四态文案。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 186 通过/0 失败。**验证受限**：GPUI 自绘 canvas，健康横幅需用户手动启动 app 核对（openclaw.json 存在且合法时显示玉色状态点）。
- 2026-11-09（续27）：**P7-1 空态丰富化（Prompts 页补齐）**。`render_prompts_page` 空态由纯文本（"该工具暂无提示词条目…"）升级为与其他页统一的视觉化引导：`✎` 图标（48px 圆形 `accent_a(0.08)` 底 + `stroke_a(0.14)` 描边）+ `{tool} 暂无提示词条目` 标题 + 「点击右上角『添加条目』新建提示词，写回时合并到工具提示文件」说明。至此供应商/会话/MCP/Skills/Prompts 五页空态均已图标+标题+说明统一。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 186 通过/0 失败。**验证受限**：GPUI 自绘 canvas，需用户手动启动 app 切换到空工具（无提示词条目）查看空态图标/引导显示。
- 2026-11-09（续28）：**P5-4 快照范围扩展**（MCP/Skills/Prompts 随项目快照保存/恢复）。`ProjectSnapshot` 新增 `extras: BTreeMap<String, serde_json::Value>`（`#[serde(default)]` 向后兼容 + Default impl）；`save_project(name, extras)` 携带扩展状态、`apply_project(name, current_extras) -> extras` 自动回存当前激活项目 extras 并返回目标 extras；app 层新增 `project_extras`（收集 MCP 五工具服务器列表 + Skills 全工具 + Prompts 条目为 JSON 载体）与 `apply_project_extras`（恢复：MCP 先清后写、Skills 逐技能 `write_tool`、Prompts `save_entry`）；供应商页「保存/应用」与顶部菜单「切换项目」三处调用点接入。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿（ns-store 15 通过，含 `project_snapshot_roundtrip`）。**验证受限**：MCP/Skills/Prompts 真实恢复需用户有对应配置（序列化/反序列化与写回 API 经编译与既有单测覆盖，app 层恢复路径为 UI 触发）。
- 2026-11-09（续29）：**P7-3 快捷键提示补全**（界面内可发现性）。`render_shortcut_hint` 由仅 Providers/Settings 两页提示扩展为 14 页全覆盖：每页操作提示（供应商「⌘F 搜索/⌘R 刷新」、路由/聚合/MCP/Skills/Prompts/用量/工具/项目/会话/账号/应用/通用/工具配置/设置各自语义）+ 全局键「⌘1-9 切换页面 · ⌘S 保存 · Esc 关闭」，状态栏常显。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿。**验证受限**：GPUI 自绘 canvas，状态栏提示文本需用户手动启动 app 核对（切页查看对应提示）。
- 2026-11-09（续30）：**P4-3 Pi SYSTEM/APPEND 统一体验**。`PromptPaths::omp_user` 新增（`~/.omp/agent/APPEND_SYSTEM.md`，`PI_CONFIG_DIR` 覆盖配置目录名；追加语义保留默认指令模板，用户条目追加到提示末尾）；`prompt_file`/`supported` 加 `Tool::Pi` 分支；Prompts 页 `PROMPT_TOOLS` 由 4 扩为 5（含 Pi）；新增 `pi_prompt_file_points_to_append_system` 单测（Pi 提示文件路径断言 + supported + OMP 不支持）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿（ns-prompts 8 通过，含新单测）。**验证受限**：Pi 真实写回需用户有 `~/.omp/agent/`（写回 API 与路径经单测覆盖，UI 触发路径为 Prompts 页「写回文件」）。
- 2026-11-09（续31）：**P5-1 官方供应商路由边界规则**。`enable_routing` 新增官方边界检查：单供应商 `ProviderCategory::Official` 默认不走代理（路由状态提示「官方供应商默认直连，不走代理」，不启动代理）；例外 Codex OpenAI 官方（`provider.id == "codex-official"`，认证经代理 Anthropic↔OpenAI 格式转换需代理）；聚合模式不拦截（用户显式配置多上游，官方可在其中）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿。**验证受限**：路由开关为 UI 触发（`toggle_routing`），需用户手动启动 app 选择官方供应商后点路由开关核对提示/不启动。
- 2026-11-09（续32）：**P4-1 项目作用域选择器 UI**。`McpPanel::choose_scope_dir` 新增（macOS `osascript -e "choose folder"` 原生目录选择器，回填 `scope_path` TextField；Linux/Windows 提示手输路径）；MCP 页「项目作用域」对话框新增「选择目录」按钮（与「关闭/读取」并列，`choose_scope_dir` 接入）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿。**验证受限**：目录选择器为 macOS 原生对话框（osascript），需用户手动启动 app 点「选择目录」核对回填与 `.mcp.json` 读取；Linux/Windows 走手输提示。
- 2026-11-10（续33）：**P5-4 cc-switch Profile 语义对齐**。`Config.active_projects: BTreeMap<String, String>` 新增（`#[serde(default)]` 向后兼容；每工具组独立当前项目）；`Store::scope_for(tool)`（Claude+ClaudeDesktop 同组 "claude"，其余各自 `id_str()`）+ `Store::active_project_for(tool)`（`active_projects` 优先，兼容旧 `active_project` 回退 Claude 组）；`save_project(name, scope, extras)`/`apply_project(name, scope, current_extras)` 按 scope 设置当前项目与自动回存（apply 回存当前 scope 的 prev 项目，非全局 active_project）；项目页「当前项目」与顶部切换器改按当前工具 scope 读取/写入。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿（ns-store 15 通过，含 `project_snapshot_roundtrip`）。**验证受限**：per-scope 当前项目切换需用户手动启动 app 在 Claude/Codex 两组分别保存/应用不同项目核对独立激活。
- 2026-11-10（续34）：**P3-1 用量多工具（Gemini 会话日志）**。`ns-usage::session` 新增 `scan_gemini_sessions`（扫描 `~/.gemini/tmp/**/chats/session-*.jsonl` 递归收集，`type=="gemini"` 记录解析 `tokens{input,output,cached,thoughts,tool,total}` + `model`，按模型统计）+ `scan_gemini_file` + 单测 `stats_accumulate_gemini_per_model`（3 记录 2 模型断言）。`SwitchApp::refresh_session_stats` 扩展为同时扫描 Claude + Codex + Gemini，合并同模型统计并按请求数排序。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿（ns-usage 12 通过，含新单测）。**验证受限**：Gemini 会话文件需实际存在 `~/.gemini/tmp/**/chats/session-*.jsonl`（本机无持久会话，单测覆盖解析逻辑）。
- 2026-11-10（续35）：**P3-3 定价与用量配置（自定义定价）**。`ns-store`：`Settings.custom_pricing: BTreeMap<String, serde_json::Value>`（模型→`{input_per_mtok,output_per_mtok,cache_read_per_mtok}`，`#[serde(default)]` 向后兼容）+ `set_custom_price`/`remove_custom_price`（保存）；`ns-usage`：`cost_with_custom(model, in, out, cache, &map)`（自定义表前缀最长匹配覆盖内置价，`CustomPrice::new`）；`SwitchApp::refresh_session_stats` 在 spawn 前读 `settings.custom_pricing` → 自定义表，Claude/Codex/Gemini 合并统计花费全部改 `cost_with_custom`；设置页 Data 分区新增「自定义定价」卡片（`price_model`/`price_input`/`price_output`/`price_cache` TextField + 添加/删除按钮，`add_custom_price` 校验 + 清空表单 + `pricing_status` 反馈）。新增单测 `custom_pricing_roundtrip`（set→重开→断言三字段）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿（ns-store 16 通过，含新单测）。**验证受限**：定价卡片 UI 为 GPUI 自绘 canvas，需用户手动启动 app 在设置页 Data 分区添加/删除自定义定价核对表单与用量重算。
- 2026-11-10（续36）：**P4-1 MCP 多工具适配器全覆盖**。`ns-mcp::tools` 新增 6 个适配器：`kimi.rs`（`~/.kimi-code/config.toml` `[mcp_servers.<name>]`，复用 codex TOML 投影，`KIMI_HOME` 覆盖）、`openclaw.rs`（`~/.openclaw/openclaw.json` `mcpServers`，复用 claude JSON 投影，`OPENCLAW_HOME` 覆盖）、`hermes.rs`（`~/.hermes/config.yaml` `mcp_servers` YAML 映射，serde_yaml 读写，`HERMES_HOME` 覆盖；`read_servers`/`apply_servers`/`validate_doc`/`load`）、`pi.rs`（`~/.pi/agent/mcp.json` `mcpServers`，复用 claude，`PI_CODING_AGENT_DIR` 覆盖）、`minimax.rs`（`~/.minimax/mcp.json` `mcpServers`，`type` 显式 `stdio`/`streamable-http`/`sse` + `enabled` 持久化，`MINIMAX_DATA_DIR`/`MAVIS_DATA_DIR` 覆盖）、`omp.rs`（`~/.omp/agent/mcp.json` `mcpServers`，复用 claude，`PI_CODING_AGENT_DIR` 覆盖）。`McpPaths` 各新增路径 + `config_for` 分支；`manager::read_tool`/`write_tool` 加 6 工具分支；`ns-mcp` 加 `serde_yaml` 依赖 + `read_yaml_or_empty`/`write_yaml` 辅助；MCP 页 `MCP_TOOLS` 从 5 扩为 11（Claude/Codex/Gemini/Opencode/GrokBuild/Kimi/Openclaw/Hermes/Pi/Mcode/Omp）。各适配器 roundtrip 单测（kimi/openclaw/pi/omp 复用既有语义 + hermes YAML stdio/remote + minimax stdio/remote/enabled）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿（ns-mcp 17 通过）。**验证受限**：真实配置读写需用户有对应工具配置文件（单测覆盖投影逻辑）；Agy 无 MCP 配置约定。
- 2026-11-10（续37）：**P4-2 Skills 全部更新**。`SkillsPanel::update_all` 新增（读当前工具全部技能，对有 `repo_url` 的逐个 `install_from_github(repo, name)` 重新安装到 Claude/Codex/Gemini，汇总更新/失败到 `write_msg`，`load` 刷新）；Skills 页头新增「全部更新」按钮（`skills-update-all`，`check_updates` 旁）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿。**验证受限**：全部更新需真实 GitHub 来源技能（UI 触发路径为 Skills 页「全部更新」，网络可用时逐个重装）。
- 2026-11-10（续38）：**P5-4 工作区快照**。`SwitchApp::project_extras` 新增 `workspace` key：遍历 `[("openclaw", ~/.openclaw/openclaw.json), ("hermes", ~/.hermes/config.yaml)]`，存在则全文读入 `{key: text}` 存 extras；`apply_project_extras` 恢复：`workspace` 有值时对每个文件 `create_dir_all(parent)` + 写回全文；恢复状态文案更新为「MCP/Skills/Prompts/工作区」。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿。**验证受限**：工作区快照真实恢复需用户有 `~/.openclaw/openclaw.json` 或 `~/.hermes/config.yaml`（文件读写经编译覆盖，app 层恢复路径为 UI 触发）。
- 2026-11-10（续39）：**P3-4 会话多工具·OpenCode 接入**。`ns-sessions` 新增 `list_opencode_sessions`（读 `~/.local/share/opencode/opencode.db` `session_v2` 表 JOIN `part` 仅列有 part 数据的会话，提取 id/title/directory/time_created）+ `parse_opencode_session`（JOIN `message` 获取 role/model，`part.data` text 类型解析 user/assistant 消息与模型名）；`recover_command_for` 加 "opencode"→`opencode session`；会话页标题/空态图标/空态名称三处 match 加 `SessionTool::OpenCode`；新增 `opencode_no_db_returns_empty` 单测。**真实验证**：本机 `~/.local/share/opencode/opencode.db` 存在，`list_opencode_sessions` 列出 24 个有 part 的会话，`parse_opencode_session` 解析出 user/assistant 消息（含 `mimo-v2.5-free`/`nemotron-3-ultra-free` 模型）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿。
- 2026-11-10（续40）：**P5-2 托管账号·Copilot 真实凭据读取**。`ns-switch::auth` 扩展：`AUTH_TOOLS` 加 `("copilot","GitHub Copilot")`、`credential_path` 加 `~/.config/github-copilot/hosts.json`、`read_token` 加 `github.com.oauth_token` 分支；新增 `copilot_token_hint_extracted`/`copilot_missing_file_not_logged_in` 单测（ns-switch 53 通过）。**验证受限**：本机无 `~/.config/github-copilot/hosts.json`（未登录），凭据读取逻辑经单测覆盖，真实显示需用户 GitHub Copilot CLI 登录后核对账号页。
- 2026-11-10（续41）：**P6-2 WSL 工具管理（代码层平台分支）**。`ns-tools` 新增 `wsl_tools()`：Windows 下探测 `wsl.exe --status`（`wsl_available`）+ `wsl.exe which <bin>`/`wsl.exe <bin> --version`（`wsl_run`）读取 WSL 发行版内工具路径/版本；非 Windows 返回空；新增 `wsl_tools_empty_on_non_windows` 单测（ns-tools 4 通过）。**验证受限**：真实 WSL 探测需 Windows 平台（本机 macOS 仅编译验证）。
- 2026-11-10（续42）：**P6-4 云同步·冲突处理策略**。`ns-store` 提取 `backup_sync_conflict(config_path, local)` 纯函数（本地 config 与远端序列化不同 → 备份到 `~/.ns-switch/backups/sync/config-<ts>.json`），`sync_pull` 覆盖远端前调用；新增 `backup_created_when_local_differs`/`no_backup_when_identical` 单测（ns-store 19 通过）。**验证受限**：真实同步需 WebDAV/S3 凭据（本机无远端，纯函数单测覆盖备份逻辑）。
- 2026-11-10（续43）：**P6-5 深链·导入后聚焦/定位**。`execute_deeplink` 各导入分支确认执行后切换页面：ImportProvider→`active_tool=Claude`+`page=Page::Providers`、ImportMcp→`Page::Mcp`、ImportSkill→`Page::Skills`、ImportPrompt→`Page::Prompts`（均 `cx.notify()`）。至此 **P1–P7 全部待办项完成**（仅剩 P6-1 Windows/Linux 自启平台验证、P5-2 ChatGPT/xAI OAuth、P3-1 Gemini 配额等受限项）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 206 通过/0 失败。
- 2026-11-10（续44）：**P5-2 OAuth device-code 流程（参考 cc-switch 逻辑）+ P3-2/P3-3/P3-4/M10 确认补全**。`ns-switch::auth` 新增完整 OAuth device-code 流程：`start_device_flow(provider)`（xAI：OIDC discovery `auth.x.ai/.well-known/openid-configuration` → `device_authorization_endpoint` form `{client_id,scope}`；Codex/ChatGPT：`auth.openai.com/oauth/device/code` POST JSON `{client_id: app_EMoamEEZ73f0CkXaXp7hrann}`）+ `DeviceCodeSession::poll`（轮询 token 端点，处理 `authorization_pending`/`slow_down`（interval+5）/`access_denied`/`expired_token`，返回 access_token）+ `build_session`（xAI `device_code`/Codex `device_auth_id` 兼容、`verification_uri_complete` 优先）+ `curl_json`/`curl_post_form`/`curl_post_json`/`urlencode` 辅助（纯 curl 子进程，无 reqwest 依赖）；账号页「登录」按钮触发真实流程（展示 user_code/verification_uri/expires_in/interval + OAuth 等待横幅），`start_oauth_poll` 后台轮询（3s 间隔，`login_account` 成功后建托管供应商）；新增 `build_session_parses_xai_device_response`/`build_session_parses_codex_device_response`/`poll_before_interval_returns_pending`/`urlencode_encodes_reserved` 单测。**M10 会话标题增强**：会话行显示 `title`（非空时优先于 id 前 8 位），`list_opencode_sessions` 提取 `title`。确认 **P3-2**（卡片配额进度条 total/used + 托盘脚注）、**P3-3**（models.dev 导入 + 用量脚本 runtime）、**P3-4**（搜索框 + OpenCode）已完成。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 200 通过/0 失败。**验证受限**：OAuth 真实授权需浏览器交互（本机无凭据，协议/状态机经单测覆盖；参考 cc-switch `auth_start_login`/`auth_poll_for_account` 逻辑移植）。
- 2026-11-10（续45）：**M10-Pi 会话解析 + P3-1 Gemini 配额 + P3-3 models.dev 导入完善 + P5-2 Copilot OAuth/token 落盘**。
  - **Pi 会话**：`ns-sessions` 新增 `SessionTool::Pi` + `list_pi_sessions`（`~/.pi/agent/sessions/<project>/<ts>_<uuid>.jsonl`，`type=="session"` 头提取 id/timestamp/cwd） + `parse_pi_session`（`type=="message"` 记录 `message.role`/`message.content[].text`）；`list_sessions`/`recover_command_for`（"pi"→`pi --resume`）加 Pi；会话页标题/空态图标/空态名称/来源切换四处加 Pi；新增 `lists_and_parses_pi_sessions`/`pi_no_dir_returns_empty` 单测 + **本机真实数据验证**（`~/.pi/agent/sessions` 存在，list+parse 真实工作）。ns-sessions 14 通过。
  - **Gemini 配额**：`ns-usage` 新增 `gemini.rs`——`query_gemini_quota`（两步 Cloud Code RPC：`loadCodeAssist`→`cloudaicompanionProject` 提取项目 ID→`retrieveUserQuota`→`buckets[].{modelId,remainingFraction,resetTime}` 按 Pro/Flash/FlashLite 分类取最低 remainingFraction 转已用百分比）、`read_gemini_access_token`（`~/.gemini/oauth_credentials.json`）、`gemini_quota_to_usage`；`query_usage` 对 Gemini 官方供应商自动走配额查询（curl 子进程，无 HTTP 依赖）；新增 `classifies_gemini_models`/`parses_rfc3339`/`quota_to_usage_takes_max` 单测。**验证受限**：需 OAuth 凭据（本机 gemini-api-key 模式无 token）。
  - **models.dev 导入完善**：`ns-usage::import_models_dev`（curl `models.dev/api.json` 下载解析 `models[].cost{input,output,cache_read}` 为 `BTreeMap<model,CustomPrice>`）；`import_models_dev_pricing` 重写用该函数（原解析旧 `pricing` 结构）；新增 `import_models_dev_fetches_prices` 单测（**真实下载 235 提供商解析通过**）。ns-usage 16 通过。
  - **Copilot OAuth + token 落盘**：`auth::start_device_flow` 加 "copilot"（GitHub device flow：`github.com/login/device/code` form `{client_id:Iv1.b507a08c87ecfe98,scope:read:user}`→`github.com/login/oauth/access_token` 轮询）+ `build_session` 加 copilot 默认 verification_uri（`github.com/login/device`）+ `poll` 加 copilot form 分支；app.rs 账号页 provider 映射加 copilot；`Store::login_account` 签名加 `(provider, token)` 并落盘真实 token（`save_oauth_token`：codex→`~/.codex/auth.json` `tokens.access_token`（保留其他字段）、xai→`~/.xai/credentials.json`、copilot→`~/.config/github-copilot/hosts.json` `github.com.oauth_token`）；`hosted_account_creates_provider` 测试用临时 HOME 验证 token 落盘。ns-store 17 通过。
  `cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 216 通过/0 失败。
- 2026-11-10（续46）：**P2-2/P2-3/P2-4/P2-5 确认补全**（第 10 章 P2 段四项由 [~] 转 [x]）。
  - **P2-2 导出/复制反馈**：`export_providers`（批量）/`export_single_provider`（单个）状态栏「已导出 N 个供应商 → path」/「已导出供应商到：path」+ `StatusTone::Success`；`duplicate_provider`「已复制供应商：name」+ 新副本 2 秒玉色高亮描边定位（`copied_provider`）；失败 `StatusTone::Error`。
  - **P2-3 对话框高级选项/认证形态**：`DialogFields::from_provider` 启发式扫描 `settings_config`（顶层/`env`/`auth`/`config` 嵌套）按关键字恢复 endpoint/key/model；`api_format_selector` 下拉；icon/icon_color/usage_script 编辑；未保存守卫（`dialog_dirty`/`request_close`/`confirm_close`）。
  - **P2-4 品牌图标/状态芯片/健康**：`icon_glyph` 17 品牌字形映射（+ 单测）；路由/聚合/直连模式 pill（`routing_for`/`aggregation_for`）；故障转移芯片；Coding Plan 朱色徽章；在线/离线健康芯片（`health` map）；配额脚注（P3-2 额度列）。
  - **P2-5 启动扫描/写入前确认对话框**：`startup_conflicts` 后台扫描（供应商页横幅判断 `switch_conflicts`/`startup_conflicts`）；`write_provider` 冲突时暂存 `pending_switch` + `render_pending_switch_confirm` 确认浮层（继续/取消），确认后 `apply_switch` 强制执行。
  `cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 216 通过/0 失败。
- 2026-11-10（续47）：**P3-4 Hermes 会话解析 + P4-1 Codex 项目级 MCP 扩展**（此前两项受限项完成）。
  - **Hermes 会话**：`ns-sessions` 新增 `SessionTool::Hermes` + `list_hermes_sessions`（`~/.hermes/state.db` SQLite `sessions` 表 `COALESCE(archived,0)=0` 过滤，`sqlite3 -json` 子进程解析，`message_count`/`model`/`last_activity_at` 提取）+ `parse_hermes_session`（`messages` 表 role/content/tool_name 解析，`is_tool_use` 标记）+ `recover_command_for` 加 "hermes"→`hermes resume`；会话页标题/空态图标（⚡）/空态名称/来源切换四处加 Hermes；`open_session_view` 识别 `/.hermes/` 路径 + `parse_hermes_session` 分支；修复 `hermes_sqlite_list_and_parse` 测试（测试前清理遗留 db——schema 变更后旧表不兼容导致查询失败）；新增 `hermes_no_db_returns_empty`/`hermes_sqlite_list_and_parse` 单测（ns-sessions 16 通过）。
  - **Codex 项目级 MCP**：`ns-switch::mcp` 新增 `codex_project_mcp_names(text)` 纯函数（从 Codex 项目级 `.codex/config.toml` 文本提取 `[mcp_servers.<name>]` 节名，排序去重）；`read_scope` 扩展为 `.mcp.json`（Claude）优先 + `.codex/config.toml`（Codex）备选，两个文件均缺失时报未找到；新增 `extracts_codex_project_mcp_names`/`codex_project_mcp_names_empty` 单测（ns-switch 59 通过）。
  `cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 224 通过/0 失败。**受限项**：P3-4 OpenClaw 会话解析（无本机会话数据源——OpenClaw 未安装，会话存储约定不明）；P6-1 Windows/Linux 自启（需对应平台运行验证）。
- 2026-11-10（续48）：**P3-4 OpenClaw 会话解析接入**（此前受限项完成）。`ns-sessions` 新增 `SessionTool::OpenClaw` + `list_openclaw_sessions`（调 `openclaw sessions --json` 官方接口——本机验证返回 `{path, count:0, totalCount:0, sessions:[]}`，解析 `sessions[]` 的 `sessionId`/`key`/`derivedTitle`/`label`/`workspaceDir`/`execCwd`/`model`/`updatedAt`/`archived`）+ `parse_openclaw_session`（调 `openclaw transcripts show <id> --json` 尽力解析 `messages`/`entries`/`turns`/`items` 数组 + `extract_openclaw_text` 提取 text/content/message 嵌套）+ `recover_command_for` 加 "openclaw"→`openclaw resume`；会话页标题/空态图标（❖）/空态名称/来源切换四处加 OpenClaw；`open_session_view` 识别 `openclaw:` 路径前缀 + `parse_openclaw_session` 分支；新增 `extracts_openclaw_text`/`openclaw_cli_lists_sessions` 单测（后者本机真实调 CLI 验证 `sessions --json` 空数组不 panic）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 226 通过/0 失败（ns-sessions 18、ns-switch 59）。**验证受限**：OpenClaw 会话阅读需真实会话（本机 `~/.openclaw` 未初始化、`sessions --json` count=0，CLI 接口与字段解析经单测覆盖，真实阅读需用户产生会话后验证）。**剩余受限项**：P6-1 Windows/Linux 自启（需对应平台运行验证）。
- 2026-11-10（续49）：**P7-3 cmd-s 真实保存**（消除快捷键「存在但无反应」）。`SwitchApp::save_current` 由空函数改为真实分发：供应商添加/编辑对话框（`add_open`/`edit_id`）→ 解析 `dialog_preset` 调 `dialog::save_provider`；Prompts/Skills/MCP 对话框打开 → 各自 `save`；工具配置编辑器（`toolconfig_id` 有值）→ `save_toolconfig`；无打开表单 → 状态栏提示「当前无可保存的表单」。`dialog_fields` 因借用冲突改为 clone 传入。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 226 通过/0 失败（ns-switch 59）。
- 2026-11-10（续50）：**P1-1 运行时跟随 OS 外观**（补齐 P1-1 最后缺口）。`ns-ui::theme` 新增 `set_system_effective(dark)`（`ACTIVE_MODE==System` 时更新 `ACTIVE_EFFECTIVE`，非 System 忽略）；`main.rs` 窗口创建后（`handle.update`）若 `active_mode()==System` 注册 `window.observe_window_appearance`，外观变化回调 `set_system_effective(dark)` + `window.refresh()` 触发重绘；新增 `system_effective_tracks_os_appearance`/`mode_parse_roundtrip` 单测（ns-ui 2 通过）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 226 通过/0 失败。**验证受限**：OS 外观实时切换需用户手动切换系统深/浅色核对（macOS `defaults read -g AppleInterfaceStyle` 与 GPUI appearance 回调）。

- 2026-11-10（续51）：**P6-1 平台分支纯函数单测覆盖**（跨平台自启）。`autostart.rs` 新增纯函数 `linux_desktop_content`（`[Desktop Entry]`/`Type=Application`/`Name`/`Exec`/`X-GNOME-Autostart-enabled`/`Terminal=false`）+ `win_reg_add_args`（`reg add` 参数：key、`/v` 名、`/t REG_SZ`、`/d exe`、`/f`）+ `win_reg_delete_args`（`reg delete` 参数：key、`/v` 名、`/f`），enable/disable 的 Linux/Windows 分支改用纯函数（消除 `reg` 内联参数重复）；`WIN_RUN_KEY`/`WIN_RUN_NAME` 常量 `#[cfg(any(target_os="windows", test))]` 使纯函数可在 macOS 测试编译；新增 `linux_desktop_content_valid`/`win_reg_add_args_wellformed`/`win_reg_delete_args_wellformed` 单测（ns-switch 62 通过）。macOS plist 真实验证（临时 HOME 写入 `com.ns-switch.plist`，`plutil -lint` 返回 OK，`is_enabled` 检测 true）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat）。**验证受限**：Windows/Linux 实际注册需对应平台运行（本机 macOS 交叉编译被第三方 `ring` 依赖阻止；平台分支格式逻辑经纯函数单测覆盖）。

- 2026-11-10（续52）：**P7-3 cmd-r 真实刷新**（消除「快捷键存在但无反应」残留）。`SwitchApp::refresh` 由「仅设状态『就绪』不重载」改为按页分发：用量页 `refresh_session_stats`（重扫 Claude/Codex/Gemini 会话统计）+ 全部工具 `query_usage`（重查配额/余额）、会话页 `load_sessions`（重扫当前工具会话）、其余页重绘重读；状态栏反馈「用量已刷新/会话已刷新/已刷新」。`cmd-r` 原绑定 `RefreshProviders` 只设就绪不实际刷新数据，现消除占位。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿（ns-switch 62 通过）。**验证受限**：GPUI 自绘 canvas，需用户手动启动 app 按 ⌘R 核对用量页/会话页数据重载与状态栏反馈。

- 2026-11-10（续53）：**P7-4 全局出站代理实现**（消除「全局代理预留」占位，对齐 cc-switch `GlobalProxySettings`）。`ns-store`：`Settings.global_proxy: Option<String>`（`#[serde(default)]` 向后兼容）+ `global_proxy()`/`set_global_proxy`（空=直连）+ `Store::set_global_proxy`（保存）；`proxy_env` 模块（`crates/ns-switch/src/proxy_env.rs`）：`curl_env`（`HTTPS_PROXY`/`HTTP_PROXY`/`ALL_PROXY` + `NO_PROXY=127.0.0.1,localhost` 本地直连，空返回仅 NO_PROXY）、`test_proxy`（经代理 curl `-I` 多公共端点 `api.anthropic.com`/`httpbin.org`/`google.com`，8 秒超时，返回 (成功, ms, 错误)）、`scan_local_proxies`（8 常见端口 7890(mixed)/7891/1080/8080/8888/3128/10808/10809，100ms 连接超时，mixed 返回 http+socks5）；`SwitchApp` 字段 `global_proxy_url`/`proxy_status`/`proxy_test`/`proxy_scan` + `save_global_proxy`（URL 前缀校验 http/https/socks → 存 settings → 清除并应用进程级代理环境，curl/git 子进程继承）/`test_global_proxy`（后台异步）/`scan_proxies`；网络分区替换占位为真实表单（标题+已配置/未配置 pill、URL TextField、保存/测试连接/扫描本地按钮、扫描结果 chips 点击回填、状态/延迟反馈、说明「ns-switch 自身出站请求经此代理，本地路由代理不受影响」）；`SwitchApp::new` 启动回填当前代理到输入框 + 应用进程环境；新增 5 单测（proxy_env）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 222 通过/0 失败（ns-switch 67，含 proxy_env 5）。**验证受限**：代理实际转发需真实本地代理服务（如 Clash 7890）；GPUI 自绘 canvas，设置页表单需用户手动启动 app 核对。

- 2026-11-10（续54）：**用量/会话 UI 与功能一致性**（消除「统计卡片显示状态文字不显示数据、图表只扫 Claude」不一致）。统计卡片：`session_stats` 从「仅总请求/总 token 状态文字」改为按模型明细表（`format_tokens` 缩放 token 数：≥1M 显示 M、≥1k 显示 k，含单测）；`ns-usage`：新增 `scan_codex_daily`/`scan_codex_details`（真实 Codex rollout 解析：`world_state.collaboration_mode.model` 提取会话级 model + `token_usage_record` 的 `payload.usage` 提取 token/时间戳，替代旧「assistant message 无 usage」错误路径）+ 重写 `scan_codex_sessions` 复用同一逻辑（统计卡片 Codex 模型从 0 修正为真实 4 模型）；app 层 `merge_daily`/`merge_details` 合并 Claude+Codex 趋势/明细（图表从「仅 Claude」扩展为 Claude+Codex）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 233 通过/0 失败（ns-switch 68 含 format_tokens 单测、ns-usage 16 含 codex 统计单测）。**验证受限**：真实 Codex rollout 已扫描（4 模型/561 请求/5 天趋势），GPUI 自绘 canvas 需用户手动启动 app 核对用量页统计卡片与趋势图显示 Codex 数据。

- 2026-11-10（续55）：**对比 cc-switch UI/功能完善**（本迭代聚焦高频可见改进）。**通用供应商**：卡片新增「同步到应用」按钮（`Store::sync_universal_children` 已存在但界面缺入口，现接线并状态栏反馈）；对话框新增「保存并同步」主按钮（`save_universal_from_dialog` 改为返回 `Option<String>` 保存 id，新建/编辑保存后立即 `sync_universal_children`，对齐 cc-switch「添加后自动同步」语义）。**MCP 面板**：新增「全部启用 / 全部禁用」按钮（`McpPanel::set_all_enabled` 批量写回，逐服务器差异写，反馈已启用/禁用 N 个）。**Skills 面板**：新增「全部启用 / 全部禁用」按钮（`SkillsPanel::set_all_enabled` 批量写回，对齐 cc-switch 每工具全开全关）。**供应商卡片**：新增「模型」副标题行（`provider_model` 从 `settings_config` 顶层/`env`/`auth`/`config` 嵌套按 model/default_model/model_name 关键字提取当前模型，对齐 cc-switch 卡片显示模型）+「官方」徽章（`provider_is_official`：category==official 或无端点且无 key 的 Claude/Codex/Gemini 官方账号）。**供应商添加/编辑对话框**：接口地址字段下方新增「测试连接」按钮（读取 `dialog_inputs.endpoint` 后台 `curl -I -m 8`，状态栏显示 HTTP 码/失败原因，对齐 cc-switch `EndpointSpeedTest`）。**顶栏**：新增侧栏折叠/展开按钮（`«`/`»`，点击切换 `sidebar_visible`，无需仅靠键盘 ⌘B）。**用量页**：每工具卡片新增配额进度条（total+used 已知时按剩余比例渲染 240px 横条，≥90% 朱红、≥70% 橙、其余玉色，对齐 cc-switch `QuotaBars`）。`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿（ns-proxy 网络单测需端口绑定授权，本机验证通过）。**验证受限**：GPUI 自绘 canvas，需用户手动启动 app 核对卡片模型/官方徽章、通用同步、MCP/Skills 全开全关、对话框测试连接、顶栏折叠与用量进度条。

- 2026-11-10（续56）：**对比 cc-switch 继续完善（用量可观测性 + 用量开关 + 熔断参数）**。
  - **用量图表多工具筛选**：`ns-usage` 新增 `scan_gemini_daily`/`scan_gemini_details`（`~/.gemini/tmp/**/chats/session-*.jsonl` 的 `type=="gemini"` 记录 token/model，按会话文件 mtime 归日近似；单测 `gemini_daily_and_details_collect`）；`SwitchApp` 新增 `usage_daily_by_tool`（每工具按日统计）+ `usage_chart_tools`（图表包含工具筛选集）；`refresh_session_stats` 扫描三工具并拆分 `by_tool`；`render_usage_charts` 新增「图表包含 Claude/Codex/Gemini」筛选切换（未初始化全选），趋势图/热力图按选中工具聚合，对齐 cc-switch 用量面板 app filter。
  - **用量查询启用/停用开关**：`ProviderMeta` 新增 `usage_enabled`（`#[serde(default=true, skip_serializing_if=is_true)]` 向后兼容旧 config；单测 `provider_meta_usage_enabled_serde`）；`DialogFields`/`build_provider` 携带 `usage_enabled`；对话框新增「启用用量查询」启停 pill；供应商卡片菜单新增「启用/停用用量查询」（`toggle_usage_enabled` 直接写 meta）；`query_usage` 尊重 `usage_enabled`（停用时插入「用量查询已停用」）；卡片配额脚注与托盘用量摘要仅启用时显示；深链导入 `usage_enabled:true`。
  - **熔断器参数 UI**：`RoutingConfig` 新增 `breaker_threshold`（默认 3，1-20）/`breaker_cooldown`（默认 30 秒，1-300，`#[serde(default)]` 向后兼容；单测 `routing_config_breaker_defaults_and_serde`）；`ProxyConfig::with_breaker`（阈值 min 1、冷却 min 1）+ `ensure_running_with` 传参；路由页每工具卡片新增「熔断阈值 −/＋」与「冷却(秒) −5/＋5」步进控件，保存到 settings，对齐 cc-switch `AutoFailoverConfigPanel` 熔断调参。
  `cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿（ns-presets 11、ns-store 20、ns-usage 17、ns-proxy 网络单测经授权验证通过）。**验证受限**：GPUI 自绘 canvas，需用户手动启动 app 核对用量筛选/用量开关/熔断步进；Gemini 每日趋势按 mtime 归日（会话内无独立时间戳）。

- 2026-11-10（续57）：**对比 cc-switch 继续完善（OpenClaw 每日记忆面板）**。
  - **OpenClaw 每日记忆**：`toolconfig.rs` 新增 `daily_memory_dir`（`~/.openclaw/workspace/memory/`）、`today_daily_memory_filename`（`date +%Y-%m-%d` 本地日期，失败回退 UTC）、`DailyMemoryFile`、`is_daily_memory_filename`（`YYYY-MM-DD.md` 严格校验月 01-12/日 01-31）、`list_daily_memory`（新→旧）、`read_daily_memory`/`write_daily_memory`（原子写）/`delete_daily_memory`、`civil_from_days`（公历转天，ns-usage 同算法；单测 `daily_memory_filename_validation`/`today_filename_is_wellformed`/`daily_memory_read_write_delete_roundtrip`）。
  - **工具配置页每日记忆区**：`render_toolconfig_page` 在 OpenClaw/Hermes 文件列表下新增「OpenClaw 每日记忆」小节——「新建/编辑今日笔记」主按钮（`open_daily_memory` 预填或空白）、今日路径脚注、每日记忆列表（日期/大小/修改时间）每行「编辑」「删除」；`render_daily_memory_editor`（TextArea 编辑器浮层，保存即写盘并刷新列表）、`render_daily_memory_confirm_delete`（删除确认浮层）；`SwitchApp` 新增 `daily_memory_files`/`daily_memory_editing`/`daily_memory_editor`/`daily_memory_status`/`daily_memory_confirm_delete`；`select_page` 进入 ToolConfig 页预载列表。对齐 cc-switch `~/.openclaw/workspace/memory/YYYY-MM-DD.md` 每日记忆能力。
  `cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿（含 toolconfig 6 单测、ns-proxy 网络单测经授权验证通过）。**验证受限**：GPUI 自绘 canvas，需用户手动启动 app 核对工具配置页每日记忆新建/编辑/删除。

- 2026-11-10（续58）：**对比 cc-switch 继续完善（OpenClaw agents.defaults 结构化编辑）**。
  - **agents.defaults 数据模型**：`toolconfig.rs` 新增 `OpenClawAgentsDefaults`（`model.primary`/`model.fallbacks`/`workspace`/`timeoutSeconds`/`contextTokens`/`maxConcurrent`，`#[serde(flatten)] extra` 保留未知字段）、`OpenClawDefaultModel`、`OpenClawAgentsDefaults::from_openclaw_json`（读 `~/.openclaw/openclaw.json` 的 `agents.defaults`，缺失/解析失败返回空）、`save_to_openclaw_json`（重建 agents.defaults、保留其余字段与格式、原子写）、`openclaw_agents_defaults_exists`；单测 `agents_defaults_serde_preserves_unknown`/`agents_defaults_roundtrip_via_tmp_home`。对齐 cc-switch `AgentsDefaultsPanel`（`OpenClawAgentsDefaults` 接口：model.primary/fallbacks、workspace、timeoutSeconds/contextTokens/maxConcurrent）。
  - **结构化编辑 UI**：工具配置页健康横幅下新增「代理默认值」按钮（`open_openclaw_defaults` 预填当前值到 `OpenClawDefaultsInputs` 五个 TextField）；`render_openclaw_defaults_editor` 浮层（主模型/工作区/超时/上下文 token/最大并发字段 + 保存/取消），`save_openclaw_defaults` 解析数值、继承 `extra` 未知字段、写回 `openclaw.json`；状态栏与浮层状态文本反馈。`SwitchApp` 新增 `openclaw_defaults_open`/`oc_defaults`/`oc_defaults_inputs`/`oc_defaults_status`。
  `cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿（ns-switch 73 单测含 toolconfig 8、ns-proxy 网络单测经授权验证通过）。**验证受限**：GPUI 自绘 canvas，需用户手动启动 app 核对工具配置页「代理默认值」保存往返（主模型/超时/并发写回 openclaw.json 且未知字段保留）。

- 2026-11-10（续59）：**对比 cc-switch 继续完善（用量请求明细来源工具）**。
  - **请求明细来源标签**：`ns_usage::RequestDetail` 新增 `tool` 字段（`claude`/`codex`/`gemini`），`collect_claude_file`/`collect_codex_file`/`collect_gemini_details` 各 push 时打上来源标记（单测 `request_detail_tags_source_tool`）；用量页「请求明细」每行新增来源列（Claude/Codex/Gemini，56px 弱色），对齐 cc-switch `RequestDetailPanel` 的 provider/source 维度。
  `cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿（ns-usage 18、ns-switch 73、ns-proxy 网络单测经授权验证通过）。**验证受限**：GPUI 自绘 canvas，需用户手动启动 app 核对用量页请求明细来源列。

- 2026-11-10（续60）：**对比 cc-switch 继续完善（用量图表日期范围预设）**。
  - **日期范围筛选**：`SwitchApp` 新增 `usage_range_days`（默认 30；0 = 全部）；`refresh_session_stats` 捕获该值并按 `scan_claude_daily/scan_codex_daily/scan_gemini_daily(h, range)` 重新扫描；用量图表新增「日期范围」预设按钮行（7 天/14 天/30 天/全部，选中高亮，点击即重扫），趋势图/热力图标题随范围更新。对齐 cc-switch `UsageDateRangePicker` 的 `today/1d/7d/14d/30d/all` 预设（本实现取 7/14/30/全部，去掉 1 天/当天因会话日志按日聚合粒度）。
  `cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿（ns-proxy 网络单测经授权验证通过）。**验证受限**：GPUI 自绘 canvas，需用户手动启动 app 核对用量图表日期范围切换（7/14/30/全部）重扫与标题更新。

- 2026-11-11（续61）：**对比 cc-switch 继续完善（可配置工具配置目录）**。
  - **每工具配置目录覆盖**：`ns-store::Settings` 新增 `config_dirs: BTreeMap<String,String>`（key = 工具 `id_str()`，value = 绝对路径；`#[serde(default)]` 向后兼容）+ `config_dir_for(tool)`/`set_config_dir(tool, dir)`（空串清除覆盖恢复默认）+ `Store::set_config_dir`（保存）；`SwitchApp::tool_config_dir` 改为实例方法，优先读 `config_dir_for` 覆盖，无覆盖走默认 `~/.<tool>`；单测 `config_dir_override_roundtrip`/`config_dir_serde_roundtrip`。
  - **设置页 UI**：应用配置分区每工具行新增「自定义配置目录」输入框（预填当前覆盖值，留空=默认）+「保存目录」按钮（`save_config_dir` 写 store）+「打开配置目录」沿用覆盖；顶部新增配置目录说明/操作状态行。对齐 cc-switch `DirectorySettings`（按 app 自定义 config 目录 + 重置默认）。
  `cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿（ns-store 22、ns-switch 73、ns-proxy 网络单测经授权验证通过）。**验证受限**：GPUI 自绘 canvas，需用户手动启动 app 核对设置→应用配置的自定义目录保存/恢复默认/打开；工具配置路径解析（live 切换/会话扫描）仍按默认路径——配置目录覆盖目前影响设置页「打开配置目录」与展示，live 读写路径仍用工具各自默认路径（后续如需让 live/会话扫描尊重覆盖，需在对应路径解析处接 `config_dir_for`）。

- 2026-11-11（续62）：**配置目录覆盖接入 live 切换引擎**。
  - `switch_engine` 新增 `ConfigDirs` 类型（`BTreeMap<String,String>`）+ `tool_base_dir(tool, home, config_dirs)`（覆盖优先，否则默认 `~/.<tool>`）；`plan_for_tool_in`/`routing_plans_for_tool_in` 增 `config_dirs` 参数并用 `tool_base_dir` 解析路径；`switch_to_in_dirs`/`switch_to_routing_in_dirs`/`check_conflicts_dirs` 为带覆盖变体（原无覆盖入口移除，app.rs 全改 `_dirs` 变体）；`SwitchApp::config_dirs()` 从 store 读 `settings.config_dirs`；`apply_switch`/`write_provider`/`startup 冲突扫描`/`toggle_routing`/`退出写回直连` 均传 `config_dirs`，使 live 切换/冲突检测/路由写回尊重用户配置目录覆盖。单测 `config_dir_override_moves_plan_path`（计划路径落到覆盖目录）。
  `cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 全绿（ns-switch 74、ns-store 22、ns-proxy 网络单测经授权验证通过）。**验证受限**：GPUI 自绘 canvas，需用户手动启动 app 设置自定义配置目录后真实切换一次并核对 live 文件落到覆盖目录。

- 2026-11-11（续63）：**对比 cc-switch 全功能实测 + 两处缺陷修复**。
  - **全功能实测**（cc-switch 4.0.20 本机运行于代理模式，live 配置指向 `127.0.0.1:15721`）：`cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` **250 通过/0 失败**；新增 2 个产品级单测——`every_preset_switches_through_engine`（真实预设表 706 条逐工具 build→switch 引擎→临时 HOME live 文件，核对写入/幂等 noop/无冲突；ClaudeDesktop/Agy 设计上无 live 写入跳过）与 `real_presets_switch_tools_write_key_fields`（Claude/Codex/Gemini/Grok 关键字段落盘 + 首写备份保留原始文件）；真实数据探针：ns-tools 探测 Claude 2.1.322/Codex 0.167.0/Gemini 0.67.0，ns-sessions 列出 Claude 20/Codex 24/OpenCode 24/Pi 7 会话并解析轮次，ns-usage 扫描 Claude 7/Codex 1 会话统计 + models.dev 真实导入 3712 条价，MCP read_tool 7 工具全通 + 写往返保注释，Skills 写往返，store CRUD 往返，代理真实 `bind`/`serve` 把 Anthropic Messages→OpenAI→Anthropic 转换返回 200；单实例深链转发（第二实例退出、主实例存活）；应用启动无 panic/警告日志。
  - **修复一 · 存储路径偏离规范**：`ns-store`/`ns-prompts` 的 `default_data_dir()` 原用 `directories::BaseDirs::config_dir()`，在 macOS 解析为 `~/Library/Application Support/ns-switch`，与 AGENTS.md §1.1.3 规定 `~/.ns-switch/config.json`（及 cc-switch `~/.cc-switch/` 约定）不符——配置实际落到 Application Support。改为 HOME 拼接 `~/.ns-switch`（`NS_SWITCH_HOME` 覆盖仍优先），移除两 crate 与 workspace 的 `directories` 依赖（零警告）；新增单测 `default_data_dir_follows_spec_home_dot_ns_switch`（断言 `~/.ns-switch` 且 `NS_SWITCH_HOME` 优先）。应用重启后配置正确落到 `~/.ns-switch/config.json`（不再写入 Application Support）。
  - **修复二 · Prompts 写回命名节合并误判子标题**：`ns-prompts::merge_edited_sections` 原以任意 `## ` 为分节边界，条目正文自带 `##` 子标题（如 `## 规则`）时正文被截断、手改丢失。改为仅识别已知条目名的节头（`next_known_header` 扫描行首 `## <known-name>`），正文内子标题不再误判；新增单测 `merge_respects_subheadings_inside_entry_body`（正文含子标题手改后库/文件均保留且旧正文不残留）。ns-prompts 9 通过。
  `cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 250 通过/0 失败。**验证受限**：GPUI 自绘 canvas + accessibility 树仅窗口控件 + 屏幕录制权限 denied，无法截图核对界面；界面功能经代码级（单测/探针）与启动无 panic 验证。**已修复的存储路径**：`~/Library/Application Support/ns-switch`（空配置）已清理，配置现按规范落 `~/.ns-switch/config.json`；如旧版本曾在 Application Support 留下供应商数据，需手动迁到 `~/.ns-switch/`。

- 2026-11-11（续64）：**路由/聚合模式实测补全**（续63 未覆盖路径）。
  - **路由模式全往返**（临时 HOME，不碰本机 cc-switch live 配置）：真实 Claude 预设 → 直连写入（真实端点+密钥）→ 路由写入（`http://127.0.0.1:15731` + `ns-switch-routing` 占位 key，真实端点被剥离）→ 退出写回直连（还原真实端点、路由地址不残留）；Gemini 路由写 `settings.json` + `.env` 双文件（`GEMINI_BASE_URL=http://127.0.0.1:15732` + 占位 key）；首写备份保留直连原始内容（3 文件）。`ns-live` `claude_routing_patch`/`gemini_routing_settings_patch`/`gemini_routing_env_patch` 均经 `project::*` 顶层导出。
  - **聚合模式**：`aggregation_model_catalog` 合并 2 供应商模型（`deepseek-flash`/`glm-5.2`/`gpt-5`）去重 + `provider_models` 从 meta 提取；Codex 聚合写回 `codex_patch` 落 `model_catalog_json = "codex-aggregate-models.json"` + `model_provider = "custom"` + `[model_providers.custom]`（`wire_api = "chat"`）。`write_codex_model_catalog` 写 `codex-aggregate-models.json` 到 store 目录。
  `cargo check --workspace` 0 错误/0 项目警告（仅第三方 future-incompat），`cargo test --workspace` 250 通过/0 失败。**验证受限**：路由/聚合真实启停代理需 app UI 触发（GPUI 自绘 canvas 无法截图），本验证经 `ns-live` 补丁引擎 + 探针覆盖写入语义；代理真实转发已在续63 经 `bind`/`serve` 验证。
