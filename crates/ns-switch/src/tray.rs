//! M6 系统托盘：图标 + 菜单（每工具供应商列表，点击切换）、状态、打开窗口/退出。
//!
//! 基于 `tray-icon` + `muda`（Tauri 同款），不依赖 GPUI 的托盘 API
//! （GPUI 0.2.2 无公开托盘支持）。菜单事件经全局 handler 转到 `mpsc`
//! 通道，由 GPUI 主线程消费并派发到 `SwitchApp`。

use std::sync::mpsc::Sender;

use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use ns_presets::Tool;
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// 托盘菜单动作（供主线程消费）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayAction {
    OpenWindow,
    SwitchProvider { tool: Tool, provider_id: String },
    Quit,
}

/// 一个工具在托盘菜单里的条目。
pub struct ToolMenuEntry {
    pub tool: Tool,
    /// 该工具是否启用本地路由代理。
    pub routing: bool,
    /// (provider_id, provider_name, is_current)
    pub providers: Vec<(String, String, bool)>,
    /// 用量摘要（配额/剩余/重置倒计时；None = 无数据）。
    pub usage_note: Option<String>,
}

/// 托盘实例（持有图标；菜单由 `TrayIcon` 持有）。
pub struct Tray {
    _icon: TrayIcon,
}

/// 菜单事件 id → 动作。
fn parse_action(id: &str) -> Option<TrayAction> {
    if id == "ns-switch:open-window" {
        return Some(TrayAction::OpenWindow);
    }
    if id == "ns-switch:quit" {
        return Some(TrayAction::Quit);
    }
    if let Some(rest) = id.strip_prefix("ns-switch:switch:") {
        let mut parts = rest.splitn(2, ':');
        let tool = parts.next()?;
        let provider_id = parts.next()?.to_string();
        // tool id → Tool
        let tool = Tool::ALL
            .into_iter()
            .find(|t| t.id_str() == tool)?;
        return Some(TrayAction::SwitchProvider { tool, provider_id });
    }
    None
}

/// 生成一个 16x16 模板图标（金色方块，中间留白十字，近似“切”）。
fn make_icon() -> Result<Icon, tray_icon::BadIcon> {
    const SIZE: u32 = 16;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let idx = ((y * SIZE + x) * 4) as usize;
            // 金色 #c9a25f
            let edge = x < 2 || x >= SIZE - 2 || y < 2 || y >= SIZE - 2;
            let center_cross = (x >= 6 && x <= 9) || (y >= 6 && y <= 9);
            if edge || center_cross {
                rgba[idx] = 0xc9;
                rgba[idx + 1] = 0xa2;
                rgba[idx + 2] = 0x5f;
                rgba[idx + 3] = 0xff;
            } else {
                rgba[idx + 3] = 0x00;
            }
        }
    }
    Icon::from_rgba(rgba, SIZE, SIZE)
}

/// 注册全局菜单/托盘事件 handler（一次性；把事件转发到 `tx`）。
///
/// 必须在创建托盘前调用一次。事件来自后台线程，`tx` 由 `SwitchApp`
/// 的后台轮询任务消费（窗口最小化时 render 不运行，需独立消费）。
pub fn register_event_handler(tx: Sender<TrayAction>) {
    let menu_tx = tx.clone();
    MenuEvent::set_event_handler(Some(Box::new(move |event: MenuEvent| {
        if let Some(action) = parse_action(event.id().as_ref()) {
            let _ = menu_tx.send(action);
        }
    })));
    let tray_tx = tx;
    TrayIconEvent::set_event_handler(Some(Box::new(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click { .. } = event {
            let _ = tray_tx.send(TrayAction::OpenWindow);
        }
    })));
}

impl Tray {
    /// 创建托盘图标与初始菜单。
    pub fn new(tool_entries: &[ToolMenuEntry]) -> Result<Self, String> {
        let menu = build_menu(tool_entries)?;
        let icon = make_icon().map_err(|e| format!("图标创建失败: {e}"))?;
        let icon = TrayIconBuilder::new()
            .with_icon_templated(icon)
            .with_tooltip("ns-switch · 商量切换器")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_menu_on_right_click(true)
            .build()
            .map_err(|e| format!("托盘创建失败: {e}"))?;
        Ok(Self { _icon: icon })
    }

    /// 重建菜单（供应商列表变化时调用）。
    pub fn rebuild_menu(&self, tool_entries: &[ToolMenuEntry]) -> Result<(), String> {
        let menu = build_menu(tool_entries)?;
        self._icon.set_menu(Some(Box::new(menu)));
        Ok(())
    }
}

/// 菜单项计划（纯数据；可单测）。
#[derive(Debug, Clone, PartialEq, Eq)]
enum PlanItem {
    Header(String),
    Action { id: String, label: String, enabled: bool },
    Separator,
    Submenu { id: String, label: String, items: Vec<PlanItem> },
}

/// 由条目生成菜单计划（不含任何平台依赖）。
fn plan_menu(tool_entries: &[ToolMenuEntry]) -> Vec<PlanItem> {
    let mut plan = Vec::new();
    // 状态摘要头（每工具：当前供应商 + 路由标记）。
    let summary: Vec<String> = tool_entries
        .iter()
        .filter_map(|e| {
            let name = e
                .providers
                .iter()
                .find(|(_, _, is_current)| *is_current)
                .map(|(_, name, _)| name.clone())?;
            let mut s = if e.routing {
                format!("{} → {name}（路由中）", e.tool.label())
            } else {
                format!("{} → {name}", e.tool.label())
            };
            if let Some(note) = &e.usage_note {
                if !note.is_empty() {
                    s.push_str(&format!("　{note}"));
                }
            }
            Some(s)
        })
        .collect();
    if !summary.is_empty() {
        plan.push(PlanItem::Header(summary.join("　")));
        plan.push(PlanItem::Separator);
    }
    plan.push(PlanItem::Action {
        id: "ns-switch:open-window".into(),
        label: "打开主窗口".into(),
        enabled: true,
    });
    plan.push(PlanItem::Separator);
    for entry in tool_entries {
        let label = if entry.routing {
            format!("{}（路由中）", entry.tool.label())
        } else {
            entry.tool.label().to_string()
        };
        let mut items = Vec::new();
        for (pid, name, is_current) in &entry.providers {
            let item_label = if *is_current {
                format!("✓ {name}")
            } else {
                name.clone()
            };
            items.push(PlanItem::Action {
                id: format!("ns-switch:switch:{}:{pid}", entry.tool.id_str()),
                label: item_label,
                enabled: true,
            });
        }
        if items.is_empty() {
            items.push(PlanItem::Action {
                id: format!("ns-switch:none:{}", entry.tool.id_str()),
                label: "（无供应商）".into(),
                enabled: false,
            });
        }
        plan.push(PlanItem::Submenu {
            id: format!("ns-switch:tool:{}", entry.tool.id_str()),
            label,
            items,
        });
    }
    plan.push(PlanItem::Separator);
    plan.push(PlanItem::Action {
        id: "ns-switch:quit".into(),
        label: "退出 ns-switch".into(),
        enabled: true,
    });
    plan
}

/// 构建托盘菜单（平台层）。
fn build_menu(tool_entries: &[ToolMenuEntry]) -> Result<Menu, String> {
    let menu = Menu::new();
    for item in plan_menu(tool_entries) {
        match item {
            PlanItem::Header(text) => {
                let head = MenuItem::with_id("ns-switch:summary", text, false, None);
                menu.append(&head).map_err(|e| e.to_string())?;
            }
            PlanItem::Action { id, label, enabled } => {
                let it = MenuItem::with_id(id, label, enabled, None);
                menu.append(&it).map_err(|e| e.to_string())?;
            }
            PlanItem::Separator => {
                menu.append(&PredefinedMenuItem::separator())
                    .map_err(|e| e.to_string())?;
            }
            PlanItem::Submenu { id, label, items } => {
                let sub = Submenu::with_id(id, label, true);
                for inner in items {
                    match inner {
                        PlanItem::Action { id, label, enabled } => {
                            let it = MenuItem::with_id(id, label, enabled, None);
                            sub.append(&it).map_err(|e| e.to_string())?;
                        }
                        _ => {}
                    }
                }
                menu.append(&sub).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(menu)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_action_ids() {
        assert_eq!(
            parse_action("ns-switch:open-window"),
            Some(TrayAction::OpenWindow)
        );
        assert_eq!(parse_action("ns-switch:quit"), Some(TrayAction::Quit));
        assert_eq!(
            parse_action("ns-switch:switch:claude:claude-88api-abc"),
            Some(TrayAction::SwitchProvider {
                tool: Tool::Claude,
                provider_id: "claude-88api-abc".into(),
            })
        );
        assert_eq!(parse_action("unknown"), None);
    }

    #[test]
    fn plan_menu_lists_providers_and_status() {
        let entries = vec![ToolMenuEntry {
            tool: Tool::Claude,
            routing: true,
            providers: vec![
                ("claude-a".into(), "供应商甲".into(), true),
                ("claude-b".into(), "供应商乙".into(), false),
            ],
            usage_note: Some("剩 100 · ⏱ 5h00m".into()),
        }];
        let plan = plan_menu(&entries);
        // 摘要头 + 分隔线 + 打开主窗口 + 分隔线 + 子菜单 + 分隔线 + 退出。
        assert_eq!(
            plan.iter().filter(|p| matches!(p, PlanItem::Separator)).count(),
            3
        );
        assert!(plan
            .iter()
            .any(|p| matches!(p, PlanItem::Action { id, .. } if id == "ns-switch:open-window")));
        assert!(plan
            .iter()
            .any(|p| matches!(p, PlanItem::Action { id, .. } if id == "ns-switch:quit")));
        // 摘要头含当前供应商名。
        assert!(plan
            .iter()
            .any(|p| matches!(p, PlanItem::Header(h) if h.contains("供应商甲") && h.contains("路由中"))));
        // Claude 子菜单含 ✓ 当前与供应商乙。
        let claude = plan.iter().find_map(|p| match p {
            PlanItem::Submenu { id, items, .. } if id == "ns-switch:tool:claude" => Some(items),
            _ => None,
        });
        let items = claude.expect("含 Claude 子菜单");
        assert!(items
            .iter()
            .any(|p| matches!(p, PlanItem::Action { label, .. } if label.starts_with('✓'))));
        assert!(items
            .iter()
            .any(|p| matches!(p, PlanItem::Action { label, .. } if label == "供应商乙")));
    }
}
