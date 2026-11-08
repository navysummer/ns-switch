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

use gpui::{App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point, px, size};



fn main() {
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
                    cx.new(|cx| app::SwitchApp::new(cx))
                },
            )
            .expect("failed to open ns-switch window");
        // 供托盘“打开主窗口”激活窗口（共享给后台轮询任务）。
        handle
            .update(cx, |app, _window, _cx| {
                *app.window_handle.lock().unwrap() = Some(handle.clone());
            })
            .ok();

        cx.activate(true);
    });
}
