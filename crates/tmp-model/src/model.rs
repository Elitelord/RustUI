//! Shared state and the background poll thread.
//!
//! THE GATE (from TaskManagerPlus `docs/LITE_MODE.md`): one background thread
//! samples the source once per interval, folds the sample into the [`Model`],
//! and wakes the UI. Otherwise the UI thread sleeps. A frontend must not also
//! redraw on a free-running frame loop, or the task manager shows up in its
//! own process list.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::source::{Sample, Source};
use crate::types::{CoreCpu, History, Proc, Snapshot};

/// Allowed poll cadence, same clamp as lite.
pub const MIN_INTERVAL_MS: u64 = 250;
pub const MAX_INTERVAL_MS: u64 = 10_000;

/// Everything the pages read. Only the poll thread writes it.
#[derive(Clone, Debug, Default)]
pub struct Model {
    pub snap: Option<Snapshot>,
    pub procs: Vec<Proc>,
    pub per_core: Vec<CoreCpu>,
    pub history: History,
    pub last_error: Option<String>,
    /// Bumped on every poll, so the UI can tell "no data yet" apart from
    /// "stale but present".
    pub tick: u64,
}

impl Model {
    /// Fold in one sample. A failed snapshot keeps the previous one and records
    /// the error. A missing process list keeps the previous rows, so the table
    /// never flickers to empty.
    pub fn apply(&mut self, sample: Sample) {
        match sample.snap {
            Ok(s) => {
                self.history.record(&s);
                self.snap = Some(s);
                self.last_error = None;
            }
            Err(e) => self.last_error = Some(e),
        }
        if let Some(p) = sample.procs {
            self.procs = p;
        }
        self.per_core = sample.per_core;
        self.tick = self.tick.wrapping_add(1);
    }
}

pub type SharedModel = Arc<Mutex<Model>>;

/// Handle to the poll thread. Dropping it stops the thread. If a sample is in
/// progress when that happens, the thread finishes it first.
#[must_use = "the poll thread stops as soon as the Poller is dropped"]
pub struct Poller {
    interval_ms: Arc<AtomicU64>,
    wake: Sender<()>,
}

impl Poller {
    pub fn interval_ms(&self) -> u64 {
        self.interval_ms.load(Ordering::Relaxed)
    }

    /// Change the cadence. The change applies to the wait that is already in
    /// progress. Shortening the interval to less than the time since the last
    /// sample triggers a sample right away. No extra off-cadence samples are
    /// taken, so the graphs' time axis stays even.
    pub fn set_interval_ms(&self, ms: u64) {
        self.interval_ms.store(
            ms.clamp(MIN_INTERVAL_MS, MAX_INTERVAL_MS),
            Ordering::Relaxed,
        );
        let _ = self.wake.send(());
    }
}

/// Start polling `source` every `interval_ms` (clamped to
/// [`MIN_INTERVAL_MS`]..=[`MAX_INTERVAL_MS`]). The first sample is taken
/// immediately. `on_tick` runs on the poll thread after each sample is folded
/// into `model`. Use it to wake the UI, for example egui's `request_repaint`
/// or Slint's `invoke_from_event_loop`.
pub fn spawn_poller<S>(
    mut source: S,
    model: SharedModel,
    interval_ms: u64,
    on_tick: impl Fn() + Send + 'static,
) -> Poller
where
    S: Source + Send + 'static,
{
    let interval = Arc::new(AtomicU64::new(
        interval_ms.clamp(MIN_INTERVAL_MS, MAX_INTERVAL_MS),
    ));
    let (wake, woken) = mpsc::channel::<()>();
    let thread_interval = Arc::clone(&interval);

    thread::Builder::new()
        .name("tmp-model poller".into())
        .spawn(move || {
            loop {
                let sample = source.sample();
                model
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .apply(sample);
                on_tick();

                let sampled_at = Instant::now();
                // Sleep until the next deadline. A wake from set_interval_ms
                // recomputes that deadline from the same sample time.
                loop {
                    let deadline =
                        sampled_at + Duration::from_millis(thread_interval.load(Ordering::Relaxed));
                    let Some(wait) = deadline.checked_duration_since(Instant::now()) else {
                        break;
                    };
                    match woken.recv_timeout(wait) {
                        Ok(()) => continue,
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
            }
        })
        .expect("failed to spawn the poll thread");

    Poller {
        interval_ms: interval,
        wake,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;

    fn snap(cpu: f64) -> Snapshot {
        Snapshot {
            cpu_usage_percent: cpu,
            ..Snapshot::default()
        }
    }

    fn proc(pid: u32) -> Proc {
        Proc {
            pid,
            name: format!("p{pid}"),
            memory_mb: 1.0,
            cpu_percent: 0.0,
            gpu_percent: 0.0,
            disk_bps: 0.0,
            net_bps: 0.0,
            process_type: None,
            window_title: String::new(),
        }
    }

    #[test]
    fn apply_keeps_last_good_data_on_failure() {
        let mut m = Model::default();
        m.apply(Sample {
            snap: Ok(snap(10.0)),
            procs: Some(vec![proc(4)]),
            per_core: vec![],
        });
        m.apply(Sample {
            snap: Err("boom".into()),
            procs: None,
            per_core: vec![],
        });

        assert_eq!(m.snap.as_ref().map(|s| s.cpu_usage_percent), Some(10.0));
        assert_eq!(m.procs.len(), 1);
        assert_eq!(m.last_error.as_deref(), Some("boom"));
        assert_eq!(m.tick, 2);
        assert_eq!(m.history.cpu.points.len(), 1, "failed polls add no history");

        m.apply(Sample {
            snap: Ok(snap(20.0)),
            procs: Some(vec![]),
            per_core: vec![],
        });
        assert_eq!(m.last_error, None);
        assert!(m.procs.is_empty());
    }

    struct Counting(u32);

    impl Source for Counting {
        fn sample(&mut self) -> Sample {
            self.0 += 1;
            Sample {
                snap: Ok(snap(f64::from(self.0))),
                procs: Some(vec![]),
                per_core: vec![],
            }
        }
    }

    fn start(interval_ms: u64) -> (Poller, SharedModel, Arc<AtomicUsize>) {
        let model = SharedModel::default();
        let ticks = Arc::new(AtomicUsize::new(0));
        let t = Arc::clone(&ticks);
        let poller = spawn_poller(Counting(0), Arc::clone(&model), interval_ms, move || {
            t.fetch_add(1, Ordering::SeqCst);
        });
        (poller, model, ticks)
    }

    #[test]
    fn polls_on_the_interval_and_stops_on_drop() {
        let (poller, model, ticks) = start(MIN_INTERVAL_MS);
        thread::sleep(Duration::from_millis(MIN_INTERVAL_MS * 2 + 150));
        let seen = ticks.load(Ordering::SeqCst);
        assert!(seen >= 2, "expected at least 2 ticks, got {seen}");
        // The model updates before on_tick runs, so it can be one ahead.
        assert!(model.lock().unwrap().tick as usize >= seen);

        drop(poller);
        thread::sleep(Duration::from_millis(100));
        let after_drop = ticks.load(Ordering::SeqCst);
        thread::sleep(Duration::from_millis(MIN_INTERVAL_MS * 3));
        assert_eq!(ticks.load(Ordering::SeqCst), after_drop);
    }

    #[test]
    fn shortening_the_interval_applies_to_the_current_wait() {
        let (poller, _model, ticks) = start(MAX_INTERVAL_MS);
        thread::sleep(Duration::from_millis(100));
        assert_eq!(ticks.load(Ordering::SeqCst), 1, "first sample is immediate");

        poller.set_interval_ms(MIN_INTERVAL_MS);
        assert_eq!(poller.interval_ms(), MIN_INTERVAL_MS);
        thread::sleep(Duration::from_millis(MIN_INTERVAL_MS + 200));
        assert!(
            ticks.load(Ordering::SeqCst) >= 2,
            "did not wait out the old 10 s interval"
        );
    }

    #[test]
    fn interval_is_clamped() {
        let (poller, _model, _ticks) = start(1);
        assert_eq!(poller.interval_ms(), MIN_INTERVAL_MS);
        poller.set_interval_ms(u64::MAX);
        assert_eq!(poller.interval_ms(), MAX_INTERVAL_MS);
    }
}
