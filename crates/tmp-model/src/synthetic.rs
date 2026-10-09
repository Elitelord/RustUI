//! Deterministic fake telemetry: about 270 processes on a machine modeled on
//! the dev laptop (Ryzen AI 9 HX 370, 32 GB, Radeon 890M, NPU, battery).
//!
//! The same seed always gives the same sequence. That keeps screenshots and
//! upstream repros reproducible, and it needs no native DLL. Over time the
//! data covers everything the pages need to show:
//! - churn: compile bursts spawn and reap `rustc` processes, and browser tabs
//!   open and close
//! - spikes: occasional CPU bursts, plus disk and network bursts on I/O-heavy apps
//! - a leak: one renderer of the fictional "Contoso Sync" grows by 1.5 MB per
//!   tick until it restarts with a new PID
//! - battery: drains with load, starts charging at 25 %, and stops at 95 %
//!
//! One tick stands for one second of machine time, whatever the poll interval,
//! so the rates and the battery drain don't depend on the refresh setting.

use std::f64::consts::TAU;

use crate::source::{Sample, Source};
use crate::types::{CoreCpu, Proc, Snapshot};

const MIB: f64 = 1_048_576.0;
const GIB: f64 = 1_073_741_824.0;

const CPU_NAME: &str = "AMD Ryzen AI 9 HX 370 w/ Radeon 890M";
const CORES: u32 = 12;
const LOGICAL: u32 = 24;
/// Four Zen 5 cores (8 threads) report as P; the eight Zen 5c cores as E.
const P_THREADS: u32 = 8;
const BASE_MHZ: f64 = 2000.0;
const MAX_MHZ: f64 = 5100.0;
/// 32 GB minus the iGPU carve-out.
const TOTAL_RAM: f64 = 31.1 * GIB;
/// Kernel and driver memory that no process row accounts for.
const KERNEL_MB: f64 = 1900.0;
const GPU_NAME: &str = "AMD Radeon(TM) 890M Graphics";
const NPU_NAME: &str = "NPU Compute Accelerator Device";
const BATTERY_DESIGN_MWH: u32 = 80_000;
const BATTERY_FULL_MWH: u32 = 71_200;
const LEAK_MB_PER_TICK: f64 = 1.5;
const LEAK_RESTART_MB: f64 = 2500.0;

/// Settings for [`Synthetic`]. The defaults are what the lab uses for
/// screenshots and benchmarks.
#[derive(Clone, Debug)]
pub struct SyntheticConfig {
    /// The same seed always gives the same sequence of samples.
    pub seed: u64,
    /// Compile bursts, browser tabs opening and closing, and short-lived helpers.
    pub churn: bool,
    /// One "Contoso Sync" renderer that leaks memory and restarts with a new PID at 2.5 GB.
    pub leak: bool,
    /// Chance per tick that the snapshot query fails, rolled again for the
    /// process list. Use it to exercise the stale-data and error paths.
    pub error_rate: f64,
    /// Starting charge and state. `None` simulates a desktop with no battery.
    pub battery: Option<BatteryStart>,
}

#[derive(Clone, Copy, Debug)]
pub struct BatteryStart {
    pub percent: f64,
    pub charging: bool,
}

impl Default for SyntheticConfig {
    fn default() -> Self {
        Self {
            seed: 0x5EED_7A5C,
            churn: true,
            leak: true,
            error_rate: 0.0,
            battery: Some(BatteryStart {
                percent: 78.0,
                charging: false,
            }),
        }
    }
}

/// A [`Source`] that makes up plausible telemetry. See the module docs.
pub struct Synthetic {
    cfg: SyntheticConfig,
    rng: Rng,
    next_pid: u32,
    /// Always sorted by PID: new processes get higher PIDs and are pushed at the end.
    procs: Vec<SimProc>,
    compile_cooldown: u32,
    cached: f64,
    npu: f64,
    npu_left: u32,
    battery: Option<Battery>,
}

struct Battery {
    percent: f64,
    charging: bool,
}

impl Default for Synthetic {
    fn default() -> Self {
        Self::new(SyntheticConfig::default())
    }
}

impl Synthetic {
    pub fn new(cfg: SyntheticConfig) -> Self {
        let mut s = Self {
            rng: Rng(cfg.seed),
            next_pid: 4,
            procs: Vec::new(),
            compile_cooldown: 0,
            cached: 6.0 * GIB,
            npu: 0.0,
            npu_left: 0,
            battery: cfg.battery.map(|b| Battery {
                percent: b.percent.clamp(0.0, 100.0),
                charging: b.charging,
            }),
            cfg,
        };
        for app in CATALOG {
            let n = s.rng.range_u32(app.count.0, app.count.1);
            for i in 0..n as usize {
                let role = app.fixed.get(i).copied().or(app.fill);
                s.spawn(app, role, None);
            }
        }
        if s.cfg.leak
            && let Some(p) = s
                .procs
                .iter_mut()
                .find(|p| p.app.name == CONTOSO && p.role == Some("renderer"))
        {
            p.leak = LEAK_MB_PER_TICK;
        }
        // Start the first compile burst early, so a fresh window shows churn quickly.
        s.compile_cooldown = s.rng.range_u32(5, 15);
        s
    }

    fn spawn(&mut self, app: &'static App, role: Option<&'static str>, ttl: Option<u32>) -> usize {
        let rng = &mut self.rng;
        let pid = self.next_pid;
        self.next_pid += 4 * rng.range_u32(1, 24);

        // With a dedicated GPU process, only that process uses the GPU.
        let has_gpu_process = app.fixed.contains(&"gpu");
        let gpu_base = if app.gpu > 0.0 && (!has_gpu_process || role == Some("gpu")) {
            app.gpu * rng.range(0.5, 1.5)
        } else {
            0.0
        };
        // The window belongs to the main (browser) process, not to renderers.
        let title = if matches!(role, None | Some("main")) {
            app.title.to_string()
        } else {
            String::new()
        };
        let mem_base = rng.range(app.mem.0, app.mem.1);
        let cpu_base = app.cpu * rng.range(0.3, 1.7);

        self.procs.push(SimProc {
            pid,
            app,
            role,
            title,
            ttl,
            mem_base,
            mem: mem_base,
            leak: 0.0,
            cpu_base,
            cpu: cpu_base,
            spike_left: 0,
            spike_to: 0.0,
            gpu_base,
            gpu: gpu_base,
            disk: 0.0,
            disk_left: 0,
            net: 0.0,
            net_left: 0,
        });
        self.procs.len() - 1
    }

    fn step(&mut self) {
        if self.cfg.churn {
            self.churn();
        }
        if self.cfg.leak {
            self.restart_leaker();
        }
        let rng = &mut self.rng;
        for p in &mut self.procs {
            p.step(rng);
        }
        self.procs.retain(|p| p.ttl != Some(0));
    }

    fn churn(&mut self) {
        if self.compile_cooldown == 0 {
            let len = self.rng.range_u32(4, 14);
            self.spawn(app(CARGO), None, Some(len + 1));
            for _ in 0..self.rng.range_u32(3, 10) {
                let ttl = self.rng.range_u32(2, len);
                self.spawn(app(RUSTC), None, Some(ttl));
            }
            self.compile_cooldown = len + self.rng.range_u32(20, 60);
        } else {
            self.compile_cooldown -= 1;
        }

        let tabs: Vec<usize> = (0..self.procs.len())
            .filter(|&i| self.procs[i].app.name == CHROME && self.procs[i].role == Some("renderer"))
            .collect();
        if tabs.len() < 32 && self.rng.chance(0.03) {
            self.spawn(app(CHROME), Some("renderer"), None);
        }
        if tabs.len() > 6 && self.rng.chance(0.03) {
            let i = tabs[self.rng.index(tabs.len())];
            self.procs.remove(i);
        }

        if self.rng.chance(0.02) {
            let ttl = self.rng.range_u32(5, 40);
            self.spawn(app(COM_SURROGATE), None, Some(ttl));
        }
    }

    fn restart_leaker(&mut self) {
        if let Some(i) = self
            .procs
            .iter()
            .position(|p| p.leak > 0.0 && p.mem >= LEAK_RESTART_MB)
        {
            self.procs.remove(i);
            let new = self.spawn(app(CONTOSO), Some("renderer"), None);
            self.procs[new].leak = LEAK_MB_PER_TICK;
        }
    }

    fn machine(&mut self) -> (Snapshot, Vec<CoreCpu>) {
        let (mut cpu, mut mem_mb, mut gpu, mut disk, mut net) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for p in &self.procs {
            cpu += p.cpu;
            mem_mb += p.mem;
            gpu += p.gpu;
            disk += p.disk;
            net += p.net;
        }
        let rng = &mut self.rng;

        let cpu = (cpu + rng.range(0.3, 1.2)).clamp(0.0, 100.0);
        let per_core = spread_cores(rng, cpu);
        let freq = (BASE_MHZ + cpu * 30.0 + rng.normal() * 80.0).clamp(1400.0, MAX_MHZ);

        let used = ((mem_mb + KERNEL_MB) * MIB).min(TOTAL_RAM * 0.98);
        let available = TOTAL_RAM - used;
        self.cached += 0.05 * (available * 0.55 - self.cached);
        self.cached = self.cached.clamp(0.0, available);
        let modified = (220.0 + rng.normal() * 25.0).max(50.0) * MIB;

        let read_share = rng.range(0.4, 0.8);
        let disk_read = disk * read_share + rng.range(0.0, 40_000.0);
        let disk_write = disk * (1.0 - read_share) + rng.range(0.0, 60_000.0);
        let disk_active =
            ((disk_read + disk_write) / 1.5e8 * 100.0 + rng.range(0.0, 1.5)).min(100.0);

        let net_send = net * 0.2 + rng.range(0.0, 2_000.0);
        let net_recv = net * 0.8 + rng.range(0.0, 6_000.0);

        let gpu = (gpu + rng.range(0.2, 1.0)).clamp(0.0, 100.0);

        // The NPU is idle except for occasional bursts (camera effects, local models).
        if self.npu_left > 0 {
            self.npu_left -= 1;
        } else if rng.chance(0.01) {
            self.npu_left = rng.range_u32(5, 20);
            self.npu = rng.range(5.0, 35.0);
        } else {
            self.npu = 0.0;
        }
        let npu = self.npu * rng.range(0.8, 1.2);

        let draw = (5.5 + cpu * 0.22 + gpu * 0.08 + rng.normal() * 0.4).max(3.0);
        let full_wh = f64::from(BATTERY_FULL_MWH) / 1000.0;
        let (battery_percent, is_charging, charge_rate, time_left) = match &mut self.battery {
            None => (0.0, false, 0.0, 0),
            Some(b) => {
                // One tick is one second, so W / Wh / 36 gives % per tick.
                if b.charging {
                    b.percent += charge_watts(b) / full_wh / 36.0;
                    b.charging = b.percent < 95.0;
                } else {
                    b.percent -= draw / full_wh / 36.0;
                    b.charging = b.percent <= 25.0;
                }
                b.percent = b.percent.clamp(0.0, 100.0);
                let left = if b.charging {
                    -1
                } else {
                    (full_wh * b.percent / 100.0 / draw * 3600.0) as i32
                };
                (b.percent, b.charging, charge_watts(b), left)
            }
        };
        let has_battery = self.battery.is_some();

        let snap = Snapshot {
            cpu_usage_percent: cpu,
            cpu_name: CPU_NAME.into(),
            core_count: CORES,
            thread_count: LOGICAL,
            cpu_frequency_mhz: freq,
            cpu_base_frequency_mhz: BASE_MHZ,
            cpu_max_frequency_mhz: MAX_MHZ,
            l1d_cache_kb: 48 * CORES,
            l1i_cache_kb: 32 * CORES,
            l2_cache_kb: 1024 * CORES,
            l3_cache_kb: 24 * 1024,

            total_ram_bytes: TOTAL_RAM as u64,
            used_ram_bytes: used as u64,
            available_ram_bytes: available as u64,
            cached_bytes: self.cached as u64,
            modified_pages_bytes: modified as u64,
            committed_bytes: (used * 1.4) as u64,
            commit_limit_bytes: (TOTAL_RAM + 8.0 * GIB) as u64,
            paged_pool_bytes: ((620.0 + rng.normal() * 10.0) * MIB) as u64,
            non_paged_pool_bytes: ((380.0 + rng.normal() * 6.0) * MIB) as u64,

            disk_read_per_sec: disk_read,
            disk_write_per_sec: disk_write,
            disk_active_percent: disk_active,
            disk_queue_length: (disk_active / 35.0) as u64,

            net_send_per_sec: net_send,
            net_recv_per_sec: net_recv,
            net_link_speed_bps: 2.4e9,

            gpu_usage_percent: gpu,
            gpu_usage_3d_percent: gpu * rng.range(0.6, 0.9),
            gpu_usage_compute_percent: gpu * 0.1 * rng.unit(),
            gpu_name: GPU_NAME.into(),
            gpu_is_integrated: true,
            gpu_memory_total: (512.0 * MIB) as u64,
            gpu_memory_used: ((250.0 + gpu * 2.4) * MIB) as u64,
            gpu_shared_memory_total: (15.5 * GIB) as u64,
            gpu_shared_memory_used: ((1200.0 + gpu * 20.0) * MIB) as u64,
            gpu_temperature: 41.0 + gpu * 0.4 + rng.normal() * 0.5,

            npu_present: true,
            npu_name: NPU_NAME.into(),
            npu_usage_percent: npu,
            npu_dedicated_total_bytes: 0,
            npu_dedicated_used_bytes: 0,
            npu_shared_total_bytes: (15.5 * GIB) as u64,
            npu_shared_used_bytes: ((40.0 + npu * 2.0) * MIB) as u64,

            battery_percent,
            is_charging,
            power_draw_watts: if has_battery { draw } else { 0.0 },
            charge_rate_watts: charge_rate,
            battery_time_remaining: time_left,
            battery_design_capacity_mwh: if has_battery { BATTERY_DESIGN_MWH } else { 0 },
            battery_full_charge_capacity_mwh: if has_battery { BATTERY_FULL_MWH } else { 0 },
            battery_cycle_count: if has_battery { 142 } else { 0 },
            battery_voltage: if has_battery {
                15.2 + battery_percent * 0.015
            } else {
                0.0
            },

            process_count: self.procs.len() as u32,
        };
        (snap, per_core)
    }
}

impl Source for Synthetic {
    fn sample(&mut self) -> Sample {
        self.step();
        let (snap, per_core) = self.machine();
        let snap_failed = self.rng.chance(self.cfg.error_rate);
        let procs_failed = self.rng.chance(self.cfg.error_rate);
        Sample {
            snap: if snap_failed {
                Err("synthetic: performance snapshot query failed".into())
            } else {
                Ok(snap)
            },
            procs: (!procs_failed).then(|| self.procs.iter().map(SimProc::to_proc).collect()),
            per_core,
        }
    }
}

/// Charger input in watts, tapering from 80 % up.
fn charge_watts(b: &Battery) -> f64 {
    if b.charging {
        38.0 * (1.0 - ((b.percent - 80.0) / 40.0).clamp(0.0, 0.75))
    } else {
        0.0
    }
}

/// Spread the total across logical processors. P threads run a little hotter,
/// and the mean stays at `total` unless a core clamps at 100 %.
fn spread_cores(rng: &mut Rng, total: f64) -> Vec<CoreCpu> {
    let raw: Vec<f64> = (0..LOGICAL)
        .map(|i| {
            let weight = if i < P_THREADS { 1.3 } else { 0.85 };
            total * weight * rng.range(0.4, 1.6) + rng.range(0.0, 2.0)
        })
        .collect();
    let sum: f64 = raw.iter().sum();
    let scale = if sum > 0.0 {
        total * f64::from(LOGICAL) / sum
    } else {
        0.0
    };
    raw.iter()
        .zip(0..)
        .map(|(r, i)| CoreCpu {
            core_index: i,
            usage_percent: (r * scale).clamp(0.0, 100.0),
            is_performance_core: i < P_THREADS,
        })
        .collect()
}

struct SimProc {
    pid: u32,
    app: &'static App,
    role: Option<&'static str>,
    title: String,
    /// Ticks left before a short-lived process exits.
    ttl: Option<u32>,
    mem_base: f64,
    mem: f64,
    /// MB added to `mem_base` every tick.
    leak: f64,
    cpu_base: f64,
    cpu: f64,
    spike_left: u32,
    spike_to: f64,
    gpu_base: f64,
    gpu: f64,
    disk: f64,
    disk_left: u32,
    net: f64,
    net_left: u32,
}

impl SimProc {
    fn step(&mut self, rng: &mut Rng) {
        if let Some(t) = &mut self.ttl {
            *t = t.saturating_sub(1);
        }
        let app = self.app;

        // CPU: noise around the base, with occasional spikes lasting a few ticks.
        if self.spike_left > 0 {
            self.spike_left -= 1;
            self.cpu = self.spike_to * rng.range(0.7, 1.0);
        } else if rng.chance(app.spike.0) {
            self.spike_left = rng.range_u32(1, 7);
            self.spike_to = rng.range(app.spike.1 * 0.3, app.spike.1);
            self.cpu = self.spike_to;
        } else {
            let target = self.cpu_base * rng.range(0.2, 1.8);
            self.cpu += 0.6 * (target - self.cpu);
        }

        // Memory drifts toward its base. A leak raises the base every tick.
        self.mem_base += self.leak;
        self.mem =
            (self.mem + 0.2 * (self.mem_base - self.mem) + rng.normal() * self.mem_base * 0.004)
                .max(0.1);

        if self.gpu_base > 0.0 {
            self.gpu = (self.gpu_base * (1.0 + 0.4 * rng.normal())).max(0.0);
        }

        (self.disk, self.disk_left) = burst(rng, app.disk, app.io_chance, self.disk_left);
        (self.net, self.net_left) = burst(rng, app.net, app.io_chance, self.net_left);
    }

    fn to_proc(&self) -> Proc {
        Proc {
            pid: self.pid,
            name: self.app.name.to_string(),
            memory_mb: self.mem,
            cpu_percent: self.cpu,
            gpu_percent: self.gpu,
            disk_bps: self.disk,
            net_bps: self.net,
            process_type: self.role.map(str::to_string),
            window_title: self.title.clone(),
        }
    }
}

/// An I/O burst: up to `size` bytes/s for a few ticks, starting with chance `p` per tick.
fn burst(rng: &mut Rng, size: f64, p: f64, left: u32) -> (f64, u32) {
    if size <= 0.0 {
        (0.0, 0)
    } else if left > 0 {
        (size * rng.range(0.2, 1.0), left - 1)
    } else if rng.chance(p) {
        (size * rng.range(0.2, 1.0), rng.range_u32(1, 9))
    } else {
        (0.0, 0)
    }
}

/// SplitMix64. Tiny, fast, and random enough for fake telemetry.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    /// Uniform in `lo..=hi`.
    fn range_u32(&mut self, lo: u32, hi: u32) -> u32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next_u64() % u64::from(hi - lo + 1)) as u32
    }

    fn index(&mut self, len: usize) -> usize {
        (self.next_u64() % len as u64) as usize
    }

    fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    /// Standard normal, by Box–Muller.
    fn normal(&mut self) -> f64 {
        let u1 = self.unit().max(f64::MIN_POSITIVE);
        let u2 = self.unit();
        (-2.0 * u1.ln()).sqrt() * (TAU * u2).cos()
    }
}

/// One kind of process in the catalog.
struct App {
    /// Display name. Rows group on it.
    name: &'static str,
    /// Instances at startup, inclusive. `(0, 0)` means churn spawns it later.
    count: (u32, u32),
    /// Roles given to the first instances, one each, in order.
    fixed: &'static [&'static str],
    /// Role for each remaining instance.
    fill: Option<&'static str>,
    /// Private working set per instance, MB.
    mem: (f64, f64),
    /// Mean CPU per instance, % of the whole machine.
    cpu: f64,
    /// Chance per tick of a CPU spike, and the spike's maximum height in %.
    spike: (f64, f64),
    /// Mean GPU %. Applies only to the "gpu" process when the app has one,
    /// otherwise to every instance.
    gpu: f64,
    /// Disk and network burst sizes in bytes/s (0 = never), and the per-tick chance of a burst.
    disk: f64,
    net: f64,
    io_chance: f64,
    /// Window title on the main (or only) process. Empty means no window.
    title: &'static str,
}

const APP: App = App {
    name: "",
    count: (1, 1),
    fixed: &[],
    fill: None,
    mem: (2.0, 12.0),
    cpu: 0.02,
    spike: (0.002, 2.0),
    gpu: 0.0,
    disk: 0.0,
    net: 0.0,
    io_chance: 0.02,
    title: "",
};

const CHROME: &str = "Google Chrome";
const CONTOSO: &str = "Contoso Sync";
const COM_SURROGATE: &str = "COM Surrogate";
const CARGO: &str = "cargo";
const RUSTC: &str = "rustc";

fn app(name: &str) -> &'static App {
    CATALOG
        .iter()
        .find(|a| a.name == name)
        .expect("name is in CATALOG")
}

#[rustfmt::skip]
static CATALOG: &[App] = &[
    // Kernel and session
    App { name: "System", mem: (0.1, 0.2), cpu: 0.3, spike: (0.01, 3.0), disk: 2.0e6, io_chance: 0.05, ..APP },
    App { name: "Registry", mem: (8.0, 14.0), cpu: 0.0, ..APP },
    App { name: "Memory Compression", mem: (300.0, 700.0), cpu: 0.05, ..APP },
    App { name: "Secure System", mem: (30.0, 60.0), cpu: 0.0, ..APP },
    App { name: "Client Server Runtime Process", count: (2, 2), mem: (1.0, 2.5), ..APP },
    App { name: "Windows Session Manager", mem: (0.3, 0.5), cpu: 0.0, ..APP },
    App { name: "Windows Start-Up Application", mem: (1.0, 1.5), cpu: 0.0, ..APP },
    App { name: "Windows Logon Application", mem: (2.0, 3.0), cpu: 0.0, ..APP },
    App { name: "Services and Controller app", mem: (5.0, 8.0), ..APP },
    App { name: "Local Security Authority Process", mem: (8.0, 12.0), ..APP },

    // Services
    App { name: "Service Host", count: (100, 112), mem: (1.0, 30.0), cpu: 0.01, spike: (0.001, 1.5), disk: 4.0e5, net: 2.0e5, io_chance: 0.004, ..APP },
    App { name: "Runtime Broker", count: (5, 8), ..APP },
    App { name: COM_SURROGATE, count: (3, 5), mem: (3.0, 10.0), ..APP },
    App { name: "Console Window Host", count: (5, 8), mem: (0.8, 8.0), cpu: 0.0, ..APP },
    App { name: "WMI Provider Host", count: (2, 4), mem: (3.0, 18.0), spike: (0.004, 1.5), ..APP },
    App { name: "Windows Driver Foundation - User-mode Driver Framework Host Process", count: (3, 5), mem: (1.5, 6.0), ..APP },
    App { name: "Spooler SubSystem App", mem: (4.0, 8.0), ..APP },
    App { name: "Antimalware Service Executable", mem: (150.0, 260.0), cpu: 0.2, spike: (0.01, 6.0), disk: 4.0e7, ..APP },
    App { name: "Microsoft Network Realtime Inspection Service", mem: (6.0, 12.0), ..APP },
    App { name: "Microsoft Windows Search Indexer", mem: (30.0, 60.0), cpu: 0.05, disk: 1.5e7, io_chance: 0.015, ..APP },
    App { name: "System Guard Runtime Monitor Broker", mem: (4.0, 8.0), ..APP },
    App { name: "Windows Security Health Service", mem: (4.0, 8.0), ..APP },
    App { name: "Device Association Framework Provider Host", mem: (3.0, 6.0), ..APP },

    // Shell
    App { name: "Desktop Window Manager", mem: (90.0, 160.0), cpu: 0.8, gpu: 2.5, ..APP },
    App { name: "Windows Explorer", mem: (110.0, 180.0), cpu: 0.3, title: "Downloads - File Explorer", ..APP },
    App { name: "Search", mem: (60.0, 120.0), ..APP },
    App { name: "Start", mem: (40.0, 70.0), ..APP },
    App { name: "Windows Shell Experience Host", mem: (30.0, 60.0), ..APP },
    App { name: "Widgets", mem: (30.0, 60.0), ..APP },
    App { name: "Microsoft Edge WebView2", count: (8, 12), fixed: &["main", "gpu", "utility-network"], fill: Some("renderer"), mem: (8.0, 60.0), gpu: 0.3, ..APP },
    App { name: "Shell Infrastructure Host", mem: (15.0, 25.0), ..APP },
    App { name: "Application Frame Host", mem: (15.0, 25.0), ..APP },
    App { name: "Host Process for Windows Tasks", mem: (6.0, 12.0), ..APP },
    App { name: "Usermode Font Driver Host", count: (2, 2), mem: (1.0, 4.0), cpu: 0.0, ..APP },
    App { name: "CTF Loader", mem: (4.0, 10.0), ..APP },
    App { name: "Microsoft Text Input Application", mem: (20.0, 40.0), ..APP },
    App { name: "LockApp", mem: (10.0, 20.0), cpu: 0.0, ..APP },
    App { name: "Windows Audio Device Graph Isolation", mem: (8.0, 14.0), cpu: 0.3, ..APP },
    App { name: "Windows Security notification icon", mem: (2.0, 4.0), cpu: 0.0, ..APP },
    App { name: "Microsoft OneDrive", mem: (60.0, 120.0), cpu: 0.1, disk: 5.0e6, net: 4.0e6, ..APP },

    // Apps
    App { name: CHROME, count: (20, 28), fixed: &["main", "gpu", "utility-network", "utility-storage", "utility-audio", "crashpad"], fill: Some("renderer"), mem: (25.0, 220.0), cpu: 0.08, spike: (0.004, 3.0), gpu: 3.0, net: 1.5e6, io_chance: 0.01, title: "longbridge/gpui-kit · GitHub - Google Chrome", ..APP },
    App { name: "Microsoft Edge", count: (8, 12), fixed: &["main", "gpu", "utility-network", "crashpad"], fill: Some("renderer"), mem: (20.0, 180.0), cpu: 0.05, gpu: 1.5, net: 6.0e5, io_chance: 0.008, title: "Windows App SDK - Microsoft Learn - Microsoft Edge", ..APP },
    App { name: "Visual Studio Code", count: (9, 9), fixed: &["main", "gpu", "shared", "pty-host", "extension-host", "watcher", "utility-network"], fill: Some("renderer"), mem: (40.0, 300.0), cpu: 0.1, spike: (0.01, 4.0), gpu: 1.5, title: "model.rs - RustUI - Visual Studio Code", ..APP },
    App { name: "rust-analyzer", mem: (900.0, 1500.0), cpu: 0.4, spike: (0.03, 30.0), ..APP },
    App { name: "Zed", mem: (250.0, 400.0), cpu: 0.3, gpu: 1.0, title: "RustUI - Zed", ..APP },
    App { name: "Windows Terminal", mem: (60.0, 110.0), cpu: 0.2, gpu: 0.4, title: "PowerShell", ..APP },
    App { name: "Windows PowerShell", count: (1, 2), mem: (60.0, 90.0), cpu: 0.0, ..APP },
    App { name: "Node.js JavaScript Runtime", count: (2, 3), mem: (60.0, 220.0), cpu: 0.1, ..APP },
    App { name: "Discord", count: (5, 6), fixed: &["main", "gpu", "utility-network", "utility-audio"], fill: Some("renderer"), mem: (40.0, 260.0), cpu: 0.05, gpu: 1.0, net: 3.0e5, title: "#general | Rust Programming Language - Discord", ..APP },
    App { name: "Spotify", count: (5, 6), fixed: &["main", "gpu", "utility-network", "utility-audio"], fill: Some("renderer"), mem: (40.0, 200.0), cpu: 0.06, gpu: 0.5, net: 2.5e5, io_chance: 0.05, title: "Spotify Premium", ..APP },
    App { name: "Microsoft Teams", count: (6, 8), fixed: &["main", "gpu"], fill: Some("renderer"), mem: (40.0, 220.0), cpu: 0.05, gpu: 0.8, net: 2.0e5, title: "Chat | Microsoft Teams", ..APP },
    App { name: "Steam", count: (4, 6), fixed: &["main", "gpu", "utility-network"], fill: Some("renderer"), mem: (30.0, 180.0), cpu: 0.03, disk: 1.5e7, net: 2.0e7, io_chance: 0.004, title: "Steam", ..APP },
    App { name: "Docker Desktop", count: (4, 6), fixed: &["main", "gpu", "service"], fill: Some("renderer"), mem: (30.0, 160.0), cpu: 0.03, title: "Docker Desktop", ..APP },
    App { name: "Vmmem", mem: (1800.0, 3200.0), cpu: 1.0, spike: (0.01, 8.0), ..APP },
    App { name: "WSL Host", count: (2, 2), mem: (4.0, 10.0), ..APP },
    App { name: "Notepad", mem: (25.0, 40.0), cpu: 0.0, title: "notes.txt - Notepad", ..APP },
    App { name: CONTOSO, count: (3, 3), fixed: &["main", "service"], fill: Some("renderer"), mem: (60.0, 160.0), cpu: 0.1, net: 3.0e5, title: "Contoso Sync", ..APP },
    App { name: "TaskManager+ Lite", mem: (35.0, 50.0), cpu: 0.3, title: "TaskManager+ Lite", ..APP },

    // Spawned only by churn
    App { name: CARGO, count: (0, 0), mem: (20.0, 60.0), cpu: 0.3, ..APP },
    App { name: RUSTC, count: (0, 0), mem: (150.0, 900.0), cpu: 4.0, spike: (0.0, 0.0), disk: 3.0e6, io_chance: 0.3, ..APP },
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn run(cfg: SyntheticConfig, ticks: usize) -> Vec<Sample> {
        let mut s = Synthetic::new(cfg);
        (0..ticks).map(|_| s.sample()).collect()
    }

    fn pids(sample: &Sample) -> HashSet<u32> {
        sample
            .procs
            .as_ref()
            .unwrap()
            .iter()
            .map(|p| p.pid)
            .collect()
    }

    #[test]
    fn same_seed_same_sequence() {
        let a = run(SyntheticConfig::default(), 50);
        let b = run(SyntheticConfig::default(), 50);
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.snap, y.snap);
            assert_eq!(x.procs, y.procs);
            assert_eq!(x.per_core, y.per_core);
        }
        let c = run(
            SyntheticConfig {
                seed: 1,
                ..SyntheticConfig::default()
            },
            1,
        );
        assert_ne!(a[0].procs, c[0].procs);
    }

    #[test]
    fn population_is_realistic_and_pids_are_valid() {
        for sample in run(SyntheticConfig::default(), 300) {
            let procs = sample.procs.unwrap();
            assert!(
                (210..=330).contains(&procs.len()),
                "{} processes",
                procs.len()
            );
            assert!(
                procs.windows(2).all(|w| w[0].pid < w[1].pid),
                "sorted, unique PIDs"
            );
            assert!(
                procs.iter().all(|p| p.pid % 4 == 0),
                "Windows PIDs are multiples of 4"
            );
            let snap = sample.snap.unwrap();
            assert_eq!(snap.process_count as usize, procs.len());
            assert!((0.0..=100.0).contains(&snap.cpu_usage_percent));
            assert!(snap.used_ram_bytes < snap.total_ram_bytes);
            assert_eq!(sample.per_core.len(), LOGICAL as usize);
        }
    }

    #[test]
    fn churn_replaces_processes() {
        let samples = run(SyntheticConfig::default(), 200);
        let (first, last) = (pids(&samples[0]), pids(&samples[199]));
        assert!(first.difference(&last).count() > 0, "some processes exited");
        assert!(
            last.difference(&first).count() > 0,
            "some processes started"
        );
        let rustc_seen = samples
            .iter()
            .any(|s| s.procs.as_ref().unwrap().iter().any(|p| p.name == RUSTC));
        assert!(rustc_seen, "a compile burst ran");
    }

    #[test]
    fn without_churn_or_leak_the_pid_set_is_stable() {
        let cfg = SyntheticConfig {
            churn: false,
            leak: false,
            ..SyntheticConfig::default()
        };
        let samples = run(cfg, 200);
        assert_eq!(pids(&samples[0]), pids(&samples[199]));
    }

    #[test]
    fn leaker_grows_then_restarts_with_a_new_pid() {
        let mut s = Synthetic::default();
        let leaker = |s: &Sample| {
            s.procs
                .as_ref()
                .unwrap()
                .iter()
                .find(|p| p.name == CONTOSO && p.process_type.as_deref() == Some("renderer"))
                .map(|p| (p.pid, p.memory_mb))
                .unwrap()
        };
        let (pid0, mem0) = leaker(&s.sample());
        let mut restarted = false;
        for tick in 1..3000 {
            let (pid, mem) = leaker(&s.sample());
            if tick == 100 {
                assert!(mem > mem0 + 100.0, "grew from {mem0:.0} to {mem:.0} MB");
            }
            if pid != pid0 {
                assert!(mem < LEAK_RESTART_MB);
                restarted = true;
                break;
            }
        }
        assert!(restarted);
    }

    #[test]
    fn battery_drains_then_charges() {
        let cfg = SyntheticConfig {
            battery: Some(BatteryStart {
                percent: 25.05,
                charging: false,
            }),
            ..SyntheticConfig::default()
        };
        let samples = run(cfg, 200);
        let first = samples[0].snap.as_ref().unwrap();
        assert!(!first.is_charging && first.battery_time_remaining > 0);
        let last = samples[199].snap.as_ref().unwrap();
        assert!(last.is_charging && last.charge_rate_watts > 0.0);
        assert_eq!(last.battery_time_remaining, -1);
    }

    #[test]
    fn no_battery_reads_as_absent() {
        let cfg = SyntheticConfig {
            battery: None,
            ..SyntheticConfig::default()
        };
        let snap = run(cfg, 1).remove(0).snap.unwrap();
        assert!(!crate::derived::has_battery(&snap));
        assert_eq!(snap.power_draw_watts, 0.0);
    }

    #[test]
    fn error_rate_fails_queries() {
        let cfg = SyntheticConfig {
            error_rate: 1.0,
            ..SyntheticConfig::default()
        };
        for sample in run(cfg, 5) {
            assert!(sample.snap.is_err());
            assert!(sample.procs.is_none());
        }
    }

    #[test]
    fn cores_average_to_the_total() {
        for sample in run(SyntheticConfig::default(), 300) {
            let total = sample.snap.unwrap().cpu_usage_percent;
            let mean = sample.per_core.iter().map(|c| c.usage_percent).sum::<f64>()
                / sample.per_core.len() as f64;
            // Equal, unless a core clamped at 100 %, which can only lower the mean.
            if sample.per_core.iter().any(|c| c.usage_percent >= 100.0) {
                assert!(mean <= total + 1e-6, "mean {mean} vs total {total}");
            } else {
                assert!((mean - total).abs() < 1e-6, "mean {mean} vs total {total}");
            }
        }
    }
}
