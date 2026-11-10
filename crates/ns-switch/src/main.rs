//! ns-switch — 商量切换器：AI 编程助手 API 供应商一键切换。
//!
//! 入口：GPUI 应用、主窗口、顶栏/侧栏/供应商页。

mod app;
mod dialog;
mod text_field;
mod text_area;
mod routing;
mod switch_engine;
mod tray;
mod mcp;
mod skills;
mod prompts;
mod auth;
mod autostart;
mod i18n;
mod deeplink;
mod gadgets;
mod toolconfig;
mod universal;

mod update;
mod single_instance;
mod proxy_env;

use gpui::{App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point, px, size};



fn main() {
    // 单实例 + 深链：启动参数里的 nsswitch:// 转发给已有实例。
    let argv_urls: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| a.starts_with("nsswitch://"))
        .collect();
    let deeplink_rx = match crate::single_instance::try_primary(argv_urls) {
        crate::single_instance::TryPrimary::Primary(rx) => Some(rx),
        crate::single_instance::TryPrimary::Secondary => {
            // 已有实例在跑，深链已转发：新实例直接退出。
            return;
        }
        crate::single_instance::TryPrimary::Fallback => None,
    };

    Application::new().run(move |cx: &mut App| {
        cx.bind_keys(app::bindings());
        cx.bind_keys(text_field::bindings());
        cx.bind_keys(text_area::bindings());
        let bounds = Bounds::centered(None, size(px(1200.), px(760.)), cx);
        let handle = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(900.), px(600.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("ns-switch".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(16.), px(17.))),
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    // 关闭主窗口 → 最小化（保留托盘运行），而非退出。
                    window.on_window_should_close(cx, |window, _cx| {
                        window.minimize_window();
                        false
                    });
                    cx.new(|cx| app::SwitchApp::new(cx, deeplink_rx))
                },
            )
            .expect("failed to open ns-switch window");
        // 供托盘“打开主窗口”激活窗口（共享给后台轮询任务）。
        handle
            .update(cx, |app, window, _cx| {
                *app.window_handle.lock().unwrap() = Some(handle.clone());
                // P1-1：system 模式下跟随 OS 外观实时变化。
                if ns_ui::theme::active_mode() == ns_ui::theme::ThemeMode::System {
                    let _ = window.observe_window_appearance(move |window, _cx| {
                        let dark = matches!(
                            window.appearance(),
                            gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark
                        );
                        ns_ui::theme::set_system_effective(dark);
                        window.refresh();
                    });
                }
            })
            .ok();

        cx.activate(true);
    });
}
