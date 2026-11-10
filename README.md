# ns-switch（商量切换器）

跨平台 AI 编程助手 API 供应商一键切换与管理软件，功能参照 [cc-switch](https://github.com/farion1231/cc-switch) 的能力模型，界面风格参照 `ns-embed-ide`（暖墨金深色主题）。

- **技术栈**：Rust + GPUI（Zed 的 GUI 框架）
- **定位**：切换 Claude Code / Codex / Gemini CLI / Grok Build 等的 API 供应商，并管理 MCP / Skills / Prompts
- **平台**：Windows / macOS / Linux

功能规划、里程碑与界面规范见 [AGENTS.md](AGENTS.md)。

## 构建与运行

```bash
cargo run -p ns-switch        # 运行
cargo check --workspace       # 检查（0 警告、0 错误）
cargo test --workspace        # 测试
```

## 里程碑状态

| 阶段 | 内容 | 状态 |
|---|---|---|
| M0 | Rust + GPUI 脚手架、暖墨金主题、侧栏、顶栏、供应商页 | ✅ 已实现 |
| M1 | `config.json` SSOT、供应商 CRUD、预设表（615 条） | ✅ 已实现 |
| M2 | 切换引擎：地板/投影/补丁（JSON/TOML/.env）、首写备份、写锁 | ✅ 已实现 |
| M3 | 界面·供应商：卡片、添加/编辑对话框、搜索、拖拽排序、导入导出 | ✅ 已实现 |
| M4 | 切换集成：一键切换、冲突检测、切换提示 | ✅ 已实现 |
| M5 | 本地路由：API 格式转换、故障转移、熔断 | ✅ 已实现 |
| M6 | 系统托盘：快速切换菜单、状态、打开/退出 | ✅ 已实现 |
| M7 | MCP/Skills/Prompts 统一面板 + 跨工具同步 | ✅ 已实现 |
| M8 | 聚合模式：模型合并、按模型路由 | ✅ 已实现 |
| M9 | 用量统计：脚本/配额/余额、会话日志、趋势图 | ✅ 已实现 |
| M10 | 会话管理：列表/搜索/阅读视图、恢复命令 | ✅ 已实现 |
| M11 | 工具管理、项目快照、托管账号 | ⚠️ 部分（账号为模拟标记） |
| M12 | 深链、单实例转发、自动更新、WebDAV 同步、设置页、i18n | ⚠️ 部分（更新手动、同步仅 WebDAV） |

> 已实现里程碑的真实完成度与已知缺口以 `AGENTS.md §5.1` 为准；托管账号、自动更新、云同步为部分实现。

## crate 划分

| crate | 职责 |
|---|---|
| `ns-switch` | GPUI 入口、主窗口、界面状态、托盘、深链、更新、i18n |
| `ns-ui` | 主题令牌、通用组件（移植 ns-embed-ide 风格） |
| `ns-store` | `~/.ns-switch/config.json` SSOT 读写、原子写、损坏回退、项目/账号/同步 |
| `ns-live` | 切换引擎：地板清单、投影、补丁改写 |
| `ns-presets` | 工具枚举（13）、供应商数据模型、预设表（615 条） |
| `ns-proxy` | 本地路由：API 格式转换、故障转移、熔断、整流器 |
| `ns-mcp` | MCP 统一管理、跨工具读写适配（Claude/Codex/Gemini） |
| `ns-skills` | Skills 仓库/安装/同步（Symlink/Copy）、跨工具目录 |
| `ns-prompts` | 提示词库、写回 CLAUDE.md/AGENTS.md/GEMINI.md |
| `ns-usage` | 用量脚本执行、价格表、会话日志统计 |
| `ns-sessions` | 会话解析、轮次构建、导出、恢复命令 |
| `ns-tools` | 工具探测/版本、安装/卸载、重复诊断 |

## 目录约定

- `~/.ns-switch/config.json` — SSOT 状态（供应商、当前选择、设置）
- `~/.ns-switch/backups/live-first-write/` — 首次改写每个客户端配置文件前的备份
- `~/.ns-switch/backups/` — 损坏回退（`.corrupt-<ts>`）

## 深链协议

`nsswitch://<action>?k=v`：`import-provider` / `import-mcp` / `import-skill` / `import-prompt` / `switch-provider` / `import-repo`。

注册：`scripts/register-deeplink.sh`（macOS 需 .app 包；Linux 写 .desktop；Windows 手动注册表）。单实例经回环 TCP 47137 转发二次实例深链。
