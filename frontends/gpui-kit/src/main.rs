// GUI subsystem: no console window when launched from Explorer or a shortcut.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! TaskManager+ Lite's UI ported to gpui-kit, on `tmp-model`.
//!
//!   processes  the process table (gpui-kit `DataTable`)
//!   perf       Performance page cards (gpui-kit `AreaChart`)
//!   battery    Battery page
//!   settings   Settings page (gpui-kit `Settings`)
//!   app        window shell (gpui-kit `Sidebar`) and the poll-thread bridge

mod app;
mod battery;
mod perf;
mod processes;
mod settings;

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

        let always_on_top = saved.always_on_top;
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
            if always_on_top {
                keep_on_top(window);
            }
            cx.new(|cx| app::LiteApp::new(saved, window, cx))
        })
        .expect("open the main window");
        cx.activate(true);
    });
}

/// GPUI has no always-on-top option for a normal window: `WindowKind::PopUp`
/// is topmost but is also a captionless tool window with no taskbar button.
/// So the lab sets the flag on the native window itself.
#[cfg(windows)]
fn keep_on_top(window: &Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
    };

    // `Window` has its own `window_handle()`, which returns GPUI's handle.
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    if let RawWindowHandle::Win32(win32) = handle.as_raw() {
        // SAFETY: the handle belongs to a live window GPUI just created.
        unsafe {
            SetWindowPos(
                win32.hwnd.get() as _,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE,
            );
        }
    }
}

#[cfg(not(windows))]
fn keep_on_top(_: &Window) {}
