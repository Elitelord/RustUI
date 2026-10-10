//! `TextEdit` ignores AccessKit's `SetValue` action.
//!
//! On Windows this is UI Automation's `ValuePattern.SetValue`, which speech
//! input, automation tools and some screen reader commands use to fill fields.
//! `Slider` and `DragValue` handle `Action::SetValue`, but `TextEdit` only
//! handles `SetTextSelection`, so the call is accepted and silently dropped.
//!
//! Expected: after `SetValue("hello")` the field shows "hello".
//! Actual: the field stays empty.
//!
//!     cargo run --example text_edit

use eframe::egui;

fn main() -> eframe::Result {
    let mut text = String::new();
    eframe::run_ui_native("egui TextEdit SetValue repro", Default::default(), move |ui, _frame| {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label("Filter:");
            ui.text_edit_singleline(&mut text);
            ui.label(format!("Current value: {text:?}"));
        });
    })
}
