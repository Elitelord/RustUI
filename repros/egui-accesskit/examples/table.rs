//! egui_extras `Table` exposes no table structure to screen readers.
//!
//! Expected: UI Automation shows a Table (or DataGrid) with rows and cells, so a
//! screen reader can announce "Memory, 120 MB, row 2 of 3".
//! Actual: header and cells are flat Text elements, plus a group with no control
//! type for every cell. Nothing says which value belongs to which row or column.
//!
//!     cargo run --example table

use eframe::egui;
use egui_extras::{Column, TableBuilder};

const ROWS: [(&str, &str, &str); 3] = [
    ("alpha.exe", "1.0%", "50 MB"),
    ("beta.exe", "2.0%", "120 MB"),
    ("gamma.exe", "3.0%", "300 MB"),
];

fn main() -> eframe::Result {
    eframe::run_ui_native("egui_extras table repro", Default::default(), |ui, _frame| {
        egui::CentralPanel::default().show(ui, |ui| {
            TableBuilder::new(ui)
                .striped(true)
                .column(Column::remainder())
                .column(Column::exact(80.0))
                .column(Column::exact(80.0))
                .header(20.0, |mut header| {
                    for title in ["Name", "CPU", "Memory"] {
                        header.col(|ui| {
                            ui.strong(title);
                        });
                    }
                })
                .body(|mut body| {
                    for (name, cpu, mem) in ROWS {
                        body.row(20.0, |mut row| {
                            for text in [name, cpu, mem] {
                                row.col(|ui| {
                                    ui.label(text);
                                });
                            }
                        });
                    }
                });
        });
    })
}
