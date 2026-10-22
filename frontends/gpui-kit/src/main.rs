// GUI subsystem: no console window when launched from Explorer or a shortcut.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! TaskManager+ Lite's UI ported to gpui-kit, on `tmp-model`.
//!
//!   processes  the process table (gpui-kit `DataTable`)
//!   app        window shell and the poll-thread bridge

mod app;
mod processes;

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;
use tmp_model::settings::LiteSettings;

fn main() {
    let saved = LiteSettings::load();

    // Without registered assets every icon (expanders, sort arrows) renders as
    // blank space, with no warning.
    let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);
    app.run(move |cx| {
        gpui_kit::init(cx);
        // Lite is dark-only.
        Theme::change(ThemeMode::Dark, None, cx);

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1100.), px(740.)), cx)),
            window_min_size: Some(size(px(720.), px(480.))),
            titlebar: Some(TitlebarOptions {
                title: Some("TaskManager+ Lite (gpui-kit lab)".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, move |window, cx| {
            cx.new(|cx| app::LiteApp::new(saved, window, cx))
        })
        .expect("open the main window");
        cx.activate(true);
    });
}
