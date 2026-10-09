//! Process table: the core lite page.
//!
//! Virtualized with `egui_extras::TableBuilder` (only visible rows are laid
//! out), mirroring what `@tanstack/react-virtual` does for the React
//! `ProcessTable`. Grouping, sorting and selection come from
//! `tmp_model::groups`, shared with every other port. End task is disabled in
//! the lab, since there is no real process behind a synthetic row.

use std::collections::HashSet;

use eframe::egui;
use egui_extras::{Column, TableBuilder, TableRow};
use tmp_model::groups::{self, Group, Row, Selection, Sort, SortKey};
use tmp_model::{Proc, derived, fmt};

use super::theme;

pub struct ProcessesView {
    filter: String,
    sort: Sort,
    selected: Option<Selection>,
    expanded: HashSet<String>,
    /// Last End task message, shown as a dismissable banner.
    error: Option<String>,
}

impl Default for ProcessesView {
    fn default() -> Self {
        Self {
            // Lab only: TMP_LAB_FILTER pre-fills the filter, so repros and
            // screenshots can start on a specific row without typing.
            filter: std::env::var("TMP_LAB_FILTER").unwrap_or_default(),
            sort: Sort::default(),
            selected: None,
            expanded: HashSet::new(),
            error: None,
        }
    }
}

impl ProcessesView {
    pub fn ui(&mut self, ui: &mut egui::Ui, procs: &[Proc]) {
        let total = procs.len();
        let groups = groups::build_groups(procs, &self.filter, self.sort);

        self.toolbar(ui, total, &groups);
        if let Some(err) = self.error.clone() {
            error_banner(ui, &err, || self.error = None);
        }
        ui.add_space(4.0);

        self.table(ui, &groups);
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, total: usize, groups: &[Group<'_>]) {
        let group_count = groups.len();
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("Filter by name…")
                    .desired_width(220.0),
            );
            if !self.filter.is_empty() && clear_button(ui).clicked() {
                self.filter.clear();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let enabled = self.selected.is_some();
                let btn = egui::Button::new(
                    egui::RichText::new("End task").color(if enabled {
                        theme::RED
                    } else {
                        theme::TEXT_MUTED
                    }),
                )
                .stroke(egui::Stroke::new(1.0, theme::BORDER_STRONG));
                if ui.add_enabled(enabled, btn).clicked() {
                    self.kill_selection(groups);
                }
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(format!("{group_count} apps · {total} processes"))
                        .color(theme::TEXT_SECONDARY),
                );
            });
        });
    }

    /// End the current selection: a single PID, or every instance in a group.
    /// In the lab this only reports what it would do. Synthetic rows have no
    /// process behind them.
    fn kill_selection(&mut self, groups: &[Group<'_>]) {
        let Some(selection) = self.selected.take() else {
            return;
        };
        let pids = selection.pids(groups);
        self.error = Some(format!(
            "Lab build: End task is disabled. It would end {} process(es): {:?}",
            pids.len(),
            pids
        ));
    }

    fn table(&mut self, ui: &mut egui::Ui, groups: &[Group<'_>]) {
        // Snapshot everything the closures need so they never borrow `self` (the
        // header mutates sort state; the body reads selection — egui can't hold
        // both borrows across the builder). Interactions are collected into
        // locals and applied to `self` afterward.
        let sort = self.sort.key;
        let ascending = self.sort.ascending;
        let selected = self.selected.clone();
        let expanded = self.expanded.clone();

        let display = groups::flatten(groups, |name| expanded.contains(name));

        let mut sort_click: Option<SortKey> = None;
        let mut toggle: Option<String> = None;
        let mut select: Option<Selection> = None;
        let mut kill: Option<Selection> = None;

        TableBuilder::new(ui)
            .striped(true)
            .resizable(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::remainder().at_least(200.0).clip(true)) // name
            .column(Column::exact(66.0)) // cpu
            .column(Column::exact(92.0)) // memory
            .column(Column::exact(58.0)) // gpu
            .column(Column::exact(92.0)) // disk
            .column(Column::exact(92.0)) // network
            .sense(egui::Sense::click())
            .header(26.0, |mut header| {
                header.col(|ui| {
                    if header_label(ui, "Name", sort == SortKey::Name, ascending, false) {
                        sort_click = Some(SortKey::Name);
                    }
                });
                for (label, key) in [
                    ("CPU", SortKey::Cpu),
                    ("Memory", SortKey::Memory),
                    ("GPU", SortKey::Gpu),
                    ("Disk", SortKey::Disk),
                    ("Network", SortKey::Net),
                ] {
                    header.col(|ui| {
                        if header_label(ui, label, sort == key, ascending, true) {
                            sort_click = Some(key);
                        }
                    });
                }
            })
            .body(|body| {
                body.rows(22.0, display.len(), |mut row| match &display[row.index()] {
                    Row::Group(gi) => {
                        let g = &groups[*gi];
                        let multi = g.is_multi();
                        let this = g.selection();
                        row.set_selected(selected.as_ref() == Some(&this));

                        row.col(|ui| {
                            ui.add_space(2.0);
                            if multi {
                                let open = expanded.contains(g.name);
                                if expander(ui, open).clicked() {
                                    toggle = Some(g.name.to_string());
                                }
                                ui.add_space(2.0);
                            } else {
                                ui.add_space(16.0);
                            }
                            // Long names end in "…" and leave room for the count. The column
                            // clips too: egui_extras grows a non-clipping column to fit its
                            // widest visible cell, so a long name scrolling into view used to
                            // widen the table and push every column right.
                            let count = multi.then(|| format!("({})", g.procs.len()));
                            let reserve = count.as_ref().map_or(0.0, |c| 8.0 * c.len() as f32 + 8.0);
                            ui.scope(|ui| {
                                ui.set_max_width((ui.available_width() - reserve).max(40.0));
                                ui.add(
                                    egui::Label::new(egui::RichText::new(g.name).color(theme::TEXT_PRIMARY))
                                        .truncate(),
                                );
                            });
                            if let Some(count) = count {
                                ui.label(egui::RichText::new(count).color(theme::TEXT_MUTED));
                            }
                        });
                        pct_cell(&mut row, g.cpu, true);
                        mem_cell(&mut row, g.mem);
                        pct_cell(&mut row, g.gpu, false);
                        rate_cell(&mut row, g.disk);
                        rate_cell(&mut row, g.net);

                        let resp = row.response();
                        if resp.clicked() {
                            select = Some(this.clone());
                        }
                        let menu_sel = this;
                        let pid_opt = if multi { None } else { Some(g.procs[0].pid) };
                        context_menu(&resp, g.name, pid_opt, multi, g.procs.len(), || {
                            kill = Some(menu_sel.clone());
                        });
                    }
                    Row::Child(p) => {
                        row.set_selected(selected == Some(Selection::Pid(p.pid)));
                        row.col(|ui| {
                            ui.add_space(30.0);
                            let (role, title) = groups::child_label(p);
                            match (role, title.is_empty()) {
                                (Some(t), false) => {
                                    ui.label(egui::RichText::new(t).color(theme::TEXT_SECONDARY));
                                    ui.label(
                                        egui::RichText::new(format!("— {title}"))
                                            .color(theme::TEXT_MUTED),
                                    );
                                }
                                (Some(t), true) => {
                                    ui.label(egui::RichText::new(t).color(theme::TEXT_SECONDARY));
                                }
                                (None, false) => {
                                    ui.label(egui::RichText::new(title).color(theme::TEXT_SECONDARY));
                                }
                                (None, true) => {
                                    ui.label(
                                        egui::RichText::new("Process").color(theme::TEXT_MUTED),
                                    );
                                }
                            }
                        });
                        pct_cell(&mut row, p.cpu_percent, true);
                        mem_cell(&mut row, p.memory_mb);
                        pct_cell(&mut row, p.gpu_percent, false);
                        rate_cell(&mut row, p.disk_bps);
                        rate_cell(&mut row, p.net_bps);

                        let resp = row.response();
                        if resp.clicked() {
                            select = Some(Selection::Pid(p.pid));
                        }
                        let pid = p.pid;
                        context_menu(&resp, &p.name, Some(pid), false, 1, || {
                            kill = Some(Selection::Pid(pid));
                        });
                    }
                });
            });

        // Apply collected interactions.
        if let Some(k) = sort_click {
            self.sort.click(k);
        }
        if let Some(name) = toggle {
            if !self.expanded.remove(&name) {
                self.expanded.insert(name);
            }
        }
        if let Some(s) = select {
            self.selected = Some(s);
        }
        if let Some(s) = kill {
            self.selected = Some(s);
            self.kill_selection(groups);
        }
    }
}

/// A sortable column header. Returns true when clicked. The sort-direction
/// arrow is painter-drawn (small, sized to the text) rather than a font glyph,
/// which rendered oversized.
fn header_label(ui: &mut egui::Ui, text: &str, active: bool, ascending: bool, right: bool) -> bool {
    let color = if active { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY };
    let widget =
        egui::Label::new(egui::RichText::new(text).color(color).strong()).sense(egui::Sense::click());
    if right {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            if active {
                sort_arrow(ui, ascending);
            }
            ui.add(widget).clicked()
        })
        .inner
    } else {
        let clicked = ui.add(widget).clicked();
        if active {
            sort_arrow(ui, ascending);
        }
        clicked
    }
}

/// Small painter-drawn sort triangle (▲ ascending / ▼ descending).
fn sort_arrow(ui: &mut egui::Ui, ascending: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(9.0, 14.0), egui::Sense::hover());
    let r = rect.shrink2(egui::vec2(1.5, 4.5));
    let pts = if ascending {
        vec![
            egui::pos2(r.center().x, r.top()),
            egui::pos2(r.left(), r.bottom()),
            egui::pos2(r.right(), r.bottom()),
        ]
    } else {
        vec![
            egui::pos2(r.left(), r.top()),
            egui::pos2(r.right(), r.top()),
            egui::pos2(r.center().x, r.bottom()),
        ]
    };
    ui.painter()
        .add(egui::Shape::convex_polygon(pts, theme::TEXT_PRIMARY, egui::Stroke::NONE));
}

/// Painter-drawn expand/collapse triangle (▸ / ▾) — no font glyph dependency.
fn expander(ui: &mut egui::Ui, open: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::click());
    let color = if resp.hovered() {
        theme::TEXT_SECONDARY
    } else {
        theme::TEXT_MUTED
    };
    let r = rect.shrink(3.5);
    let pts = if open {
        vec![
            egui::pos2(r.left(), r.top()),
            egui::pos2(r.right(), r.top()),
            egui::pos2(r.center().x, r.bottom()),
        ]
    } else {
        vec![
            egui::pos2(r.left(), r.top()),
            egui::pos2(r.right(), r.center().y),
            egui::pos2(r.left(), r.bottom()),
        ]
    };
    ui.painter()
        .add(egui::Shape::convex_polygon(pts, color, egui::Stroke::NONE));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Painter-drawn circle-with-✕ clear affordance — no font glyph dependency.
fn clear_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::click());
    let color = if resp.hovered() {
        theme::TEXT_SECONDARY
    } else {
        theme::TEXT_MUTED
    };
    let c = rect.center();
    let p = ui.painter();
    p.circle_stroke(c, 7.0, egui::Stroke::new(1.0, color));
    let d = 2.8;
    let s = egui::Stroke::new(1.2, color);
    p.line_segment([c + egui::vec2(-d, -d), c + egui::vec2(d, d)], s);
    p.line_segment([c + egui::vec2(d, -d), c + egui::vec2(-d, d)], s);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// Right-aligned percentage cell. `show_zero`=false blanks near-zero values
/// (used for GPU, which is 0 for almost every process).
fn pct_cell(row: &mut TableRow<'_, '_>, value: f64, show_zero: bool) {
    row.col(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            if value < 0.05 && !show_zero {
                return;
            }
            let c = theme::level(derived::TABLE_CPU.level(value));
            ui.label(egui::RichText::new(format!("{value:.1}%")).monospace().color(c));
        });
    });
}

fn mem_cell(row: &mut TableRow<'_, '_>, mem_mb: f64) {
    row.col(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(fmt::mb(mem_mb))
                    .monospace()
                    .color(theme::TEXT_SECONDARY),
            );
        });
    });
}

/// Right-aligned bytes/sec cell; blank when idle to keep the columns quiet.
fn rate_cell(row: &mut TableRow<'_, '_>, bps: f64) {
    row.col(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            if bps < 1.0 {
                return;
            }
            ui.label(
                egui::RichText::new(fmt::rate(bps))
                    .monospace()
                    .color(theme::ACCENT),
            );
        });
    });
}

fn context_menu(
    resp: &egui::Response,
    title: &str,
    pid: Option<u32>,
    is_group: bool,
    count: usize,
    mut on_kill: impl FnMut(),
) {
    resp.context_menu(|ui| {
        ui.label(egui::RichText::new(title).color(theme::TEXT_SECONDARY));
        if let Some(pid) = pid {
            ui.label(egui::RichText::new(format!("PID {pid}")).monospace().color(theme::TEXT_MUTED));
        }
        ui.separator();
        let label = if is_group {
            format!("End task (all {count})")
        } else {
            "End task".to_string()
        };
        if ui.button(egui::RichText::new(label).color(theme::RED)).clicked() {
            on_kill();
            ui.close();
        }
    });
}

fn error_banner(ui: &mut egui::Ui, msg: &str, mut dismiss: impl FnMut()) {
    egui::Frame::NONE
        .fill(egui::Color32::from_rgb(40, 20, 20))
        .stroke(egui::Stroke::new(1.0, theme::RED))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(msg).color(theme::TEXT_PRIMARY));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Dismiss").clicked() {
                        dismiss();
                    }
                });
            });
        });
}
