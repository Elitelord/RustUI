// GUI subsystem: no console window when launched from Explorer or a shortcut.
#![windows_subsystem = "windows"]

//! TaskManager+ Lite's egui UI, ported from `TaskManagerPlus/src-tauri/src/lite`
//! onto `tmp-model`. The UI code is lite's own. Data now comes from
//! `tmp_model::Synthetic`, and the processes table uses the shared grouping
//! and sort code instead of its own copy.
//!
//!   theme      palette / fonts / egui style, ported from tokens.css
//!   processes  the virtualized process table
//!   perf       Performance page cards
//!   battery    Battery page
//!   settings   Settings page
//!   app        window shell: sidebar nav, header strip, view dispatch

mod app;
mod battery;
mod perf;
mod processes;
mod settings;
mod theme;

use eframe::egui;
use tmp_model::settings::LiteSettings;

use app::LiteApp;

fn main() -> eframe::Result<()> {
    // Loaded here so window options such as always-on-top apply before the
    // viewport is built.
    let saved = LiteSettings::load();

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1100.0, 740.0])
        .with_min_inner_size([720.0, 480.0])
        .with_title("TaskManager+ Lite (egui lab)");
    if saved.always_on_top {
        viewport = viewport.with_always_on_top();
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "TaskManager+ Lite (egui lab)",
        options,
        Box::new(move |cc| Ok(Box::new(LiteApp::new(cc, saved)))),
    )
}
