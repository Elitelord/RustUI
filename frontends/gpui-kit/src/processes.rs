//! Process table: the core lite page, on gpui-kit's virtualized `DataTable`.
//!
//! Grouping, sorting and selection come from `tmp_model::groups`, shared with
//! every other port. End task is disabled in the lab, since there is no real
//! process behind a synthetic row.

use std::collections::HashSet;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::table::{
    Column, ColumnSort, DataTable, TableDelegate, TableEvent, TableState,
};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use tmp_model::groups::{self, Row, Selection, Sort, SortKey};
use tmp_model::{Proc, derived, fmt, palette};

/// One table row, owned: `groups::Group` borrows the process list, but the
/// table keeps its rows between frames.
struct DisplayRow {
    kind: RowKind,
    selection: Selection,
    cpu: f64,
    mem: f64,
    gpu: f64,
    disk: f64,
    net: f64,
}

enum RowKind {
    Group {
        name: SharedString,
        count: usize,
        expanded: bool,
        /// The PID, when the group is a single process.
        pid: Option<u32>,
    },
    Child {
        pid: u32,
        role: Option<SharedString>,
        title: SharedString,
    },
}

impl DisplayRow {
    fn title(&self) -> SharedString {
        match &self.kind {
            RowKind::Group { name, .. } => name.clone(),
            RowKind::Child { role, title, .. } => role.clone().unwrap_or_else(|| title.clone()),
        }
    }

    /// What a screen reader should say for the row. GPUI exposes no plain text
    /// to accessibility, so the row's cells are invisible without this.
    fn accessible_label(&self) -> String {
        let mut parts = vec![match &self.kind {
            RowKind::Group { name, count, .. } if *count > 1 => {
                format!("{name}, {count} processes")
            }
            RowKind::Group { name, .. } => name.to_string(),
            RowKind::Child { role, title, .. } => match (role, title.is_empty()) {
                (Some(role), false) => format!("{role}, {title}"),
                (Some(role), true) => role.to_string(),
                (None, false) => title.to_string(),
                (None, true) => "Process".to_string(),
            },
        }];
        parts.push(format!("CPU {:.1}%", self.cpu));
        parts.push(format!("Memory {}", fmt::mb(self.mem)));
        if self.gpu >= 0.05 {
            parts.push(format!("GPU {:.1}%", self.gpu));
        }
        if self.disk >= 1.0 {
            parts.push(format!("Disk {}", fmt::rate(self.disk)));
        }
        if self.net >= 1.0 {
            parts.push(format!("Network {}", fmt::rate(self.net)));
        }
        parts.join(", ")
    }

    fn pid(&self) -> Option<u32> {
        match &self.kind {
            RowKind::Group { pid, .. } => *pid,
            RowKind::Child { pid, .. } => Some(*pid),
        }
    }
}

pub struct ProcDelegate {
    owner: WeakEntity<ProcessesView>,
    procs: Vec<Proc>,
    filter: String,
    sort: Sort,
    expanded: HashSet<String>,
    rows: Vec<DisplayRow>,
    group_count: usize,
}

impl ProcDelegate {
    fn new(owner: WeakEntity<ProcessesView>, filter: String) -> Self {
        Self {
            owner,
            procs: Vec::new(),
            filter,
            sort: Sort::default(),
            expanded: HashSet::new(),
            rows: Vec::new(),
            group_count: 0,
        }
    }

    /// Recompute the visible rows from the process list, filter, sort and
    /// expanded groups.
    fn rebuild(&mut self) {
        let groups = groups::build_groups(&self.procs, &self.filter, self.sort);
        let display = groups::flatten(&groups, |name| self.expanded.contains(name));
        self.group_count = groups.len();
        self.rows = display
            .iter()
            .map(|row| match row {
                Row::Group(ix) => {
                    let g = &groups[*ix];
                    DisplayRow {
                        kind: RowKind::Group {
                            name: g.name.to_string().into(),
                            count: g.procs.len(),
                            expanded: self.expanded.contains(g.name),
                            pid: (!g.is_multi()).then(|| g.procs[0].pid),
                        },
                        selection: g.selection(),
                        cpu: g.cpu,
                        mem: g.mem,
                        gpu: g.gpu,
                        disk: g.disk,
                        net: g.net,
                    }
                }
                Row::Child(p) => {
                    let (role, title) = groups::child_label(p);
                    DisplayRow {
                        kind: RowKind::Child {
                            pid: p.pid,
                            role: role.map(|r| r.to_string().into()),
                            title: title.into(),
                        },
                        selection: Selection::Pid(p.pid),
                        cpu: p.cpu_percent,
                        mem: p.memory_mb,
                        gpu: p.gpu_percent,
                        disk: p.disk_bps,
                        net: p.net_bps,
                    }
                }
            })
            .collect();
    }

    fn toggle(&mut self, name: &str) {
        if !self.expanded.remove(name) {
            self.expanded.insert(name.to_string());
        }
        self.rebuild();
    }
}

fn color(rgb: palette::Rgb) -> Hsla {
    gpui_kit::rgb(rgb.hex()).into()
}

impl TableDelegate for ProcDelegate {
    fn columns_count(&self, _: &App) -> usize {
        SortKey::COLUMNS.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        let key = SortKey::COLUMNS[col_ix];
        let column = Column::new(key.label(), key.label()).sortable();
        match key {
            SortKey::Name => column.width(px(320.)).min_width(px(200.)),
            // Lite opens sorted by CPU, highest first.
            SortKey::Cpu => column.width(px(80.)).text_right().descending(),
            SortKey::Gpu => column.width(px(70.)).text_right(),
            _ => column.width(px(100.)).text_right(),
        }
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) {
        self.sort = match sort {
            ColumnSort::Ascending => Sort {
                key: SortKey::COLUMNS[col_ix],
                ascending: true,
            },
            ColumnSort::Descending => Sort {
                key: SortKey::COLUMNS[col_ix],
                ascending: false,
            },
            ColumnSort::Default => Sort::default(),
        };
        self.rebuild();
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some(row) = self.rows.get(row_ix) else {
            return div().into_any_element();
        };
        let mono = cx.theme().mono_font_family.clone();
        let muted = cx.theme().muted_foreground;

        match SortKey::COLUMNS[col_ix] {
            SortKey::Name => match &row.kind {
                RowKind::Group {
                    name,
                    count,
                    expanded,
                    ..
                } => {
                    let expander = if *count > 1 {
                        let name = name.clone();
                        div()
                            .id(("expand", row_ix))
                            .cursor_pointer()
                            .child(Icon::new(if *expanded {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
                            }))
                            .on_click(cx.listener(move |table, _, _, cx| {
                                cx.stop_propagation();
                                table.delegate_mut().toggle(&name);
                                cx.notify();
                            }))
                            .into_any_element()
                    } else {
                        div().w(px(16.)).into_any_element()
                    };
                    h_flex()
                        .gap_1()
                        .overflow_hidden()
                        .child(expander)
                        .child(div().truncate().child(name.clone()))
                        .when(*count > 1, |this| {
                            this.child(div().text_color(muted).child(format!("({count})")))
                        })
                        .into_any_element()
                }
                RowKind::Child { role, title, .. } => {
                    let text = match (role, title.is_empty()) {
                        (Some(role), false) => format!("{role} — {title}"),
                        (Some(role), true) => role.to_string(),
                        (None, false) => title.to_string(),
                        (None, true) => "Process".to_string(),
                    };
                    div()
                        .pl(px(30.))
                        .truncate()
                        .text_color(muted)
                        .child(text)
                        .into_any_element()
                }
            },
            SortKey::Cpu => right(percent(row.cpu, true, mono)),
            SortKey::Gpu => right(percent(row.gpu, false, mono)),
            SortKey::Memory => right(div().font_family(mono).child(fmt::mb(row.mem))),
            SortKey::Disk => right(rate(row.disk, mono, cx)),
            SortKey::Net => right(rate(row.net, mono, cx)),
        }
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        let tr = div().id(("row", row_ix));
        let Some(row) = self.rows.get(row_ix) else {
            return tr;
        };
        let tr = tr.aria_label(row.accessible_label());
        let RowKind::Group {
            name,
            count,
            expanded,
            ..
        } = &row.kind
        else {
            return tr;
        };
        if *count < 2 {
            return tr;
        }
        // Expand and Collapse from assistive tools (UI Automation's
        // ExpandCollapsePattern), mirroring the chevron.
        let table = cx.entity().downgrade();
        let action = if *expanded {
            gpui_kit::accesskit::Action::Collapse
        } else {
            gpui_kit::accesskit::Action::Expand
        };
        let name = name.clone();
        tr.aria_expanded(*expanded)
            .on_a11y_action(action, move |_, _, cx| {
                let _ = table.update(cx, |table, cx| {
                    table.delegate_mut().toggle(&name);
                    cx.notify();
                });
            })
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let key = SortKey::COLUMNS[col_ix];
        if key == SortKey::Name {
            div().size_full().child(key.label()).into_any_element()
        } else {
            right(div().child(key.label()))
        }
    }

    fn context_menu(
        &mut self,
        row_ix: usize,
        menu: PopupMenu,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> PopupMenu {
        let Some(row) = self.rows.get(row_ix) else {
            return menu;
        };
        let mut menu = menu.label(row.title());
        if let Some(pid) = row.pid() {
            menu = menu.label(format!("PID {pid}"));
        }
        let label = match &row.kind {
            RowKind::Group { count, .. } if *count > 1 => format!("End task (all {count})"),
            _ => "End task".to_string(),
        };
        let owner = self.owner.clone();
        let selection = row.selection.clone();
        menu.separator()
            .item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                let selection = selection.clone();
                let _ = owner.update(cx, |view, cx| view.end_task(selection, cx));
            }))
    }
}

/// Right-aligns a cell. `DataTable` ignores `Column::text_right()` in v0.8.0
/// (neither the header nor the body cells read the column's alignment), so the
/// numeric columns align themselves.
fn right(cell: impl IntoElement) -> AnyElement {
    h_flex()
        .size_full()
        .justify_end()
        .child(cell)
        .into_any_element()
}

/// A percentage, colored by load. `show_zero` = false blanks near-zero values
/// (GPU is 0 for almost every process).
fn percent(value: f64, show_zero: bool, mono: SharedString) -> AnyElement {
    if value < 0.05 && !show_zero {
        return div().into_any_element();
    }
    div()
        .font_family(mono)
        .text_color(color(palette::level(derived::TABLE_CPU.level(value))))
        .child(format!("{value:.1}%"))
        .into_any_element()
}

/// Bytes per second; blank when idle to keep the columns quiet.
fn rate(bps: f64, mono: SharedString, cx: &App) -> AnyElement {
    if bps < 1.0 {
        return div().into_any_element();
    }
    div()
        .font_family(mono)
        .text_color(cx.theme().primary)
        .child(fmt::rate(bps))
        .into_any_element()
}

pub struct ProcessesView {
    filter: Entity<InputState>,
    table: Entity<TableState<ProcDelegate>>,
    selected: Option<Selection>,
    /// Last End task message, shown until dismissed.
    message: Option<String>,
    total: usize,
    _subscriptions: Vec<Subscription>,
}

impl ProcessesView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Lab only: TMP_LAB_FILTER pre-fills the filter, so repros and
        // screenshots can start on a specific row without typing.
        let initial = std::env::var("TMP_LAB_FILTER").unwrap_or_default();
        let filter = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Filter by name…")
                .default_value(initial.clone())
        });
        let owner = cx.entity().downgrade();
        let table = cx.new(|cx| {
            TableState::new(ProcDelegate::new(owner, initial), window, cx).col_movable(false)
        });

        let subscriptions = vec![
            cx.subscribe_in(&filter, window, |this, input, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = input.read(cx).value().to_string();
                    this.table.update(cx, |table, cx| {
                        let delegate = table.delegate_mut();
                        delegate.filter = value;
                        delegate.rebuild();
                        cx.notify();
                    });
                    cx.notify();
                }
            }),
            cx.subscribe_in(&table, window, |this, table, event, _, cx| {
                if let TableEvent::SelectRow(ix) = event {
                    this.selected = table
                        .read(cx)
                        .delegate()
                        .rows
                        .get(*ix)
                        .map(|row| row.selection.clone());
                    cx.notify();
                }
            }),
            // The table selects by row index, but the rows re-sort every tick.
            // Whenever they change (new data, filter, sort, expand), move the
            // table's selection back onto the selected process.
            cx.observe(&table, |this, _, cx| this.sync_selection(cx)),
        ];

        Self {
            filter,
            table,
            selected: None,
            message: None,
            total: 0,
            _subscriptions: subscriptions,
        }
    }

    /// New data from the poll thread.
    pub fn set_procs(&mut self, procs: Vec<Proc>, _: &mut Window, cx: &mut Context<Self>) {
        self.total = procs.len();
        self.table.update(cx, |table, cx| {
            let delegate = table.delegate_mut();
            delegate.procs = procs;
            delegate.rebuild();
            cx.notify();
        });
        cx.notify();
    }

    /// Keep the table's highlighted row on the selected process. Clears the
    /// selection when that process is no longer listed.
    fn sync_selection(&mut self, cx: &mut Context<Self>) {
        let Some(selected) = self.selected.clone() else {
            return;
        };
        let table = self.table.read(cx);
        let wanted = table
            .delegate()
            .rows
            .iter()
            .position(|row| row.selection == selected);
        if wanted == table.selected_row() {
            return;
        }
        match wanted {
            Some(ix) => self
                .table
                .update(cx, |table, cx| table.set_selected_row(ix, cx)),
            None => {
                self.selected = None;
                self.table.update(cx, |table, cx| table.clear_selection(cx));
                cx.notify();
            }
        }
    }

    /// End the selection: a single PID, or every instance in a group. In the
    /// lab this only reports what it would do.
    fn end_task(&mut self, selection: Selection, cx: &mut Context<Self>) {
        let delegate = self.table.read(cx).delegate();
        let groups = groups::build_groups(&delegate.procs, &delegate.filter, delegate.sort);
        let pids = selection.pids(&groups);
        self.message = Some(format!(
            "Lab build: End task is disabled. It would end {} process(es): {:?}",
            pids.len(),
            pids
        ));
        cx.notify();
    }
}

impl Render for ProcessesView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let group_count = self.table.read(cx).delegate().group_count;
        let selected = self.selected.clone();

        v_flex()
            .size_full()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .child(Input::new(&self.filter).w(px(220.)).cleanable(true))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{group_count} apps · {} processes", self.total)),
                    )
                    .child(
                        Button::new("end-task")
                            .danger()
                            .label("End task")
                            .disabled(selected.is_none())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(selection) = this.selected.clone() {
                                    this.end_task(selection, cx);
                                }
                            })),
                    ),
            )
            .when_some(self.message.clone(), |this, message| {
                this.child(
                    h_flex()
                        .gap_2()
                        .p_2()
                        .rounded_md()
                        .border_1()
                        .border_color(cx.theme().danger)
                        .child(div().flex_1().child(message))
                        .child(
                            Button::new("dismiss")
                                .label("Dismiss")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.message = None;
                                    cx.notify();
                                })),
                        ),
                )
            })
            .child(DataTable::new(&self.table).stripe(true))
    }
}
