# ns-switch（商量切换器）

跨平台 AI 编程助手 API 供应商一键切换与管理软件，功能参照 [cc-switch](https://github.com/farion1231/cc-switch) 的能力模型，界面风格参照 `ns-embed-ide`（暖墨金深色主题）。

- **技术栈**：Rust + GPUI（Zed 的 GUI 框架）
- **定位**：切换 Claude Code / Codex / Gemini CLI / Grok Build 等的 API 供应商，并管理 MCP / Skills / Prompts
- **平台**：Windows / macOS / Linux

功能规划、里程碑与界面规范见 [AGENT.md](AGENT.md)。

## 构建与运行

```bash
cargo run -p ns-switch        # 运行
cargo check --workspace       # 检查（0 警告、0 错误）
cargo test --workspace        # 测试
```

## 里程碑状态

| 阶段 | 内容 | 状态 |
|---|---|---|
| M0 | Rust + GPUI 脚手架、暖墨金主题、侧栏、供应商页占位 | 🚧 进行中 |
| M1 | `config.json` SSOT、供应商 CRUD、预设表 | 🔜 |
| M2 | 切换引擎：地板/投影/补丁 | 🔜 |
| M3–M12 | 界面·供应商、切换集成、本地路由、托盘、MCP/Skills/Prompts、聚合、用量、会话、工具/项目/账号、辅助 | 🔜 |

> 未完成里程碑不得声称已实现，见 AGENT.md §5.1。

## crate 划分

| crate | 职责 |
|---|---|
| `ns-switch` | GPUI 入口、主窗口、界面状态 |
| `ns-ui` | 主题令牌、通用组件（移植 ns-embed-ide 风格） |
| `ns-store` | `~/.ns-switch/config.json` SSOT 读写、原子写、损坏回退 |
| `ns-live` | 切换引擎：地板清单、投影、补丁改写 |
| `ns-presets` | 工具枚举、供应商数据模型、预设表 |

## 目录约定

- `~/.ns-switch/config.json` — SSOT 状态（供应商、当前选择、设置）
- `~/.ns-switch/backups/live-first-write/` — 首次改写每个客户端配置文件前的备份
