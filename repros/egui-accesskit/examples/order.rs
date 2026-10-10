//! Order of AccessKit children compared with on-screen order.
//!
//! Screen readers read children in tree order. They should match reading order:
//! top to bottom, then left to right.
//!
//!     cargo run --example order

use eframe::egui;

fn main() -> eframe::Result {
    eframe::run_ui_native("egui order repro", Default::default(), |ui, _frame| {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label("1 top");
            ui.label("2 middle");
            ui.label("3 bottom");
            ui.horizontal(|ui| {
                ui.label("4 left");
                ui.label("5 center");
                ui.label("6 right");
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label("9 added first, drawn rightmost");
                ui.label("8 added second");
                ui.label("7 added third, drawn leftmost");
            });
        });
    })
}
