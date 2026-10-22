//! The window shell, and the bridge from the poll thread to GPUI.
//!
//! THE GATE (see `tmp_model::model`): the poll thread wakes the UI once per
//! sample and the UI otherwise sleeps. GPUI only redraws a view when it is
//! notified, so the bridge notifies once per tick and nothing else repaints.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;
use tmp_model::settings::LiteSettings;
use tmp_model::{Poller, SharedModel, Synthetic, spawn_poller};

use crate::processes::ProcessesView;

pub struct LiteApp {
    model: SharedModel,
    processes: Entity<ProcessesView>,
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
            processes,
            _poller: poller,
            _ticks: ticks,
        }
    }

    fn on_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let procs = self
            .model
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .procs
            .clone();
        self.processes
            .update(cx, |view, cx| view.set_procs(procs, window, cx));
    }
}

impl Render for LiteApp {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p_3()
            .bg(cx.theme().background)
            .child(self.processes.clone())
    }
}
