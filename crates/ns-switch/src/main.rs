//! ns-switch — 商量切换器：AI 编程助手 API 供应商一键切换。
//!
//! 入口：GPUI 应用、主窗口、顶栏/侧栏/供应商页。

mod app;

use gpui::{App, AppContext, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point, px, size};



fn main() {
    Application::new().run(move |cx: &mut App| {
        cx.bind_keys(app::bindings());

        let bounds = Bounds::centered(None, size(px(1200.), px(760.)), cx);
        cx.open_window(
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
            |_window, cx| cx.new(|cx| app::SwitchApp::new(cx)),
        )
        .expect("failed to open ns-switch window");

        cx.activate(true);
    });
}
