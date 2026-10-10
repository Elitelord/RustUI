//! Same table as `table`, but with the virtualized `TableBody::rows` that real
//! apps use for long lists, and values that change once a second like a live
//! monitor. Set `STATIC=1` to keep the values fixed.
//!
//!     cargo run --example table_virtual

use eframe::egui;
use egui_extras::{Column, TableBuilder};

fn main() -> eframe::Result {
    let live = std::env::var_os("STATIC").is_none();
    let start = std::time::Instant::now();
    eframe::run_ui_native("egui_extras virtual table repro", Default::default(), move |ui, _frame| {
        let tick = if live { start.elapsed().as_secs() } else { 0 };
        if live {
            ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
        }
        egui::CentralPanel::default().show(ui, |ui| {
            TableBuilder::new(ui)
                .striped(true)
                .column(Column::remainder())
                .column(Column::exact(80.0))
                .header(20.0, |mut header| {
                    for title in ["Name", "CPU"] {
                        header.col(|ui| {
                            ui.strong(title);
                        });
                    }
                })
                .body(|body| {
                    body.rows(20.0, 1000, |mut row| {
                        let i = row.index();
                        row.col(|ui| {
                            ui.label(format!("row {i}"));
                        });
                        row.col(|ui| {
                            ui.label(format!("{}.0%", (i as u64 + tick) % 100));
                        });
                    });
                });
        });
    })
}
