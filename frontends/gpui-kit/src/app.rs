//! The window shell: sidebar navigation, a live header strip, the pages, and
//! the bridge from the poll thread to GPUI.
//!
//! THE GATE (see `tmp_model::model`): the poll thread wakes the UI once per
//! sample and the UI otherwise sleeps. GPUI only redraws a view when it is
//! notified, so the bridge notifies once per tick and nothing else repaints.

use gpui_kit::component::sidebar::{Sidebar, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem};
use gpui_kit::component::{ActiveTheme as _, IconName, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use tmp_model::settings::LiteSettings;
use tmp_model::{
    CoreCpu, History, Poller, SharedModel, Snapshot, Synthetic, derived, fmt, palette, spawn_poller,
};

use crate::perf;
use crate::processes::ProcessesView;

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Processes,
    Performance,
}

impl Page {
    fn from_key(key: &str) -> Self {
        match key {
            "performance" => Page::Performance,
            _ => Page::Processes,
        }
    }
}

pub struct LiteApp {
    model: SharedModel,
    page: Page,
    processes: Entity<ProcessesView>,
    /// The latest data, copied out of the model on each tick.
    snap: Option<Snapshot>,
    history: History,
    per_core: Vec<CoreCpu>,
    poll_error: Option<String>,
    /// Kept alive for the life of the window: dropping it stops the poll thread.
    _poller: Poller,
    _ticks: Task<()>,
}

impl LiteApp {
    pub fn new(saved: LiteSettings, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let model = SharedModel::default();

        // The poll thread can't touch GPUI, so it only signals a channel. A
        // foreground task waits on it and pushes the new data into the views.
        let (wake, woken) = smol::channel::bounded::<()>(1);
        let poller = spawn_poller(Synthetic::default(), model.clone(), saved.refresh_ms, move || {
            // A full channel already holds a pending wake-up, which is enough.
            let _ = wake.try_send(());
        });

        let processes = cx.new(|cx| ProcessesView::new(window, cx));

        let ticks = cx.spawn_in(window, async move |this, cx| {
            while woken.recv().await.is_ok() {
                let alive = this.update_in(cx, |app, window, cx| app.on_tick(window, cx));
                if alive.is_err() {
                    break;
                }
            }
        });

        Self {
            model,
            // Lab only: TMP_LAB_PAGE opens a page directly, for screenshots and audits.
            page: Page::from_key(
                &std::env::var("TMP_LAB_PAGE").unwrap_or_else(|_| saved.default_page.clone()),
            ),
            processes,
            snap: None,
            history: History::default(),
            per_core: Vec::new(),
            poll_error: None,
            _poller: poller,
            _ticks: ticks,
        }
    }

    fn on_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let procs = {
            let model = self
                .model
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            self.snap = model.snap.clone();
            self.history = model.history.clone();
            self.per_core = model.per_core.clone();
            self.poll_error = model.last_error.clone();
            model.procs.clone()
        };
        self.processes
            .update(cx, |view, cx| view.set_procs(procs, window, cx));
        cx.notify();
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let item = |label: &'static str, icon: IconName, page: Page, cx: &mut Context<Self>| {
            SidebarMenuItem::new(label)
                .icon(icon)
                .active(self.page == page)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.page = page;
                    cx.notify();
                }))
        };
        Sidebar::new("nav")
            .w(px(180.))
            .header(
                SidebarHeader::new().child(
                    h_flex()
                        .gap_0p5()
                        .font_semibold()
                        .child("TaskManager")
                        .child(div().text_color(cx.theme().primary).child("+")),
                ),
            )
            .child(SidebarGroup::new("Pages").child(SidebarMenu::new().children([
                item("Processes", IconName::LayoutDashboard, Page::Processes, cx),
                item("Performance", IconName::ChartPie, Page::Performance, cx),
            ])))
    }

    /// The live stats strip above the pages.
    fn header(&self, cx: &App) -> impl IntoElement {
        let stat = |label: &str, value: String, value_color: Hsla| {
            h_flex()
                .gap_2()
                .font_family(cx.theme().mono_font_family.clone())
                .text_sm()
                .child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(label.to_string()),
                )
                .child(div().text_color(value_color).child(value))
        };
        let level = |l| -> Hsla { gpui_kit::rgb(palette::level(l).hex()).into() };
        let accent = cx.theme().primary;

        h_flex()
            .h(px(46.))
            .px_4()
            .gap_5()
            .flex_shrink_0()
            .border_b_1()
            .border_color(cx.theme().border)
            .map(|this| match &self.snap {
                None => this.child(div().text_color(cx.theme().muted_foreground).child("—")),
                Some(s) => {
                    let mem = s.mem_percent();
                    this.child(stat(
                        "CPU",
                        format!("{:.0}%", s.cpu_usage_percent),
                        level(derived::HEADER_LOAD.level(s.cpu_usage_percent)),
                    ))
                    .child(stat(
                        "Memory",
                        format!("{mem:.0}%"),
                        level(derived::HEADER_MEMORY.level(mem)),
                    ))
                    .child(stat(
                        "GPU",
                        format!("{:.0}%", s.gpu_usage_percent),
                        level(derived::HEADER_LOAD.level(s.gpu_usage_percent)),
                    ))
                    .child(stat("Disk", fmt::rate(s.disk_bps()), accent))
                    .child(stat("Net", fmt::rate(s.net_bps()), accent))
                }
            })
    }
}

impl Render for LiteApp {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match (&self.snap, self.page) {
            (None, _) => v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_2()
                .text_color(cx.theme().muted_foreground)
                .child("Reading system data…")
                .children(self.poll_error.clone())
                .into_any_element(),
            (Some(_), Page::Processes) => self.processes.clone().into_any_element(),
            (Some(s), Page::Performance) => {
                perf::page(s, &self.history, &self.per_core, cx).into_any_element()
            }
        };

        h_flex()
            .size_full()
            .bg(cx.theme().background)
            .child(self.sidebar(cx))
            .child(
                v_flex()
                    .flex_1()
                    .h_full()
                    .overflow_hidden()
                    .child(self.header(cx))
                    .child(div().flex_1().min_h_0().p_3().child(content)),
            )
    }
}
