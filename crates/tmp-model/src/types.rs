//! The data the four pages read. Field names match `taskmanagerplus_lib::ffi`,
//! so a live source can convert with a field-for-field copy.

use std::collections::VecDeque;

/// One process row. Identity and memory come from the process list. CPU, GPU,
/// disk and network come from the per-metric lists and are joined by PID.
#[derive(Clone, Debug, PartialEq)]
pub struct Proc {
    pub pid: u32,
    /// Display name (`display_name`, or the exe name when that is empty). Rows group on this.
    pub name: String,
    /// Private working set, MB.
    pub memory_mb: f64,
    /// Share of the whole machine, %.
    pub cpu_percent: f64,
    pub gpu_percent: f64,
    /// Read + write, bytes/s.
    pub disk_bps: f64,
    /// Send + receive, bytes/s.
    pub net_bps: f64,
    /// Multi-process role slug from the classifier (`renderer`, `gpu`,
    /// `utility`, …). It labels the child rows of an expanded group.
    pub process_type: Option<String>,
    /// Best visible-window title for this PID. Empty for background and helper processes.
    pub window_title: String,
}

/// One logical processor.
#[derive(Clone, Debug, PartialEq)]
pub struct CoreCpu {
    pub core_index: u32,
    pub usage_percent: f64,
    /// P-core when true. The FFI field is an `i32` where nonzero means P.
    pub is_performance_core: bool,
}

/// The subset of `ffi::PerformanceSnapshot` that the lite pages read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    // CPU
    pub cpu_usage_percent: f64,
    pub cpu_name: String,
    pub core_count: u32,
    /// Logical processors.
    pub thread_count: u32,
    pub cpu_frequency_mhz: f64,
    pub cpu_base_frequency_mhz: f64,
    pub cpu_max_frequency_mhz: f64,
    pub l1d_cache_kb: u32,
    pub l1i_cache_kb: u32,
    pub l2_cache_kb: u32,
    pub l3_cache_kb: u32,

    // Memory
    pub total_ram_bytes: u64,
    pub used_ram_bytes: u64,
    pub available_ram_bytes: u64,
    /// Standby list.
    pub cached_bytes: u64,
    pub modified_pages_bytes: u64,
    pub committed_bytes: u64,
    pub commit_limit_bytes: u64,
    pub paged_pool_bytes: u64,
    pub non_paged_pool_bytes: u64,

    // Disk
    pub disk_read_per_sec: f64,
    pub disk_write_per_sec: f64,
    pub disk_active_percent: f64,
    pub disk_queue_length: u64,

    // Network
    pub net_send_per_sec: f64,
    pub net_recv_per_sec: f64,
    /// The name says bits per second, but lite formats it with the bytes/s
    /// formatter. Confirm the unit before copying that.
    pub net_link_speed_bps: f64,

    // GPU
    pub gpu_usage_percent: f64,
    pub gpu_usage_compute_percent: f64,
    pub gpu_name: String,
    pub gpu_is_integrated: bool,
    pub gpu_memory_total: u64,
    pub gpu_memory_used: u64,
    pub gpu_shared_memory_total: u64,
    pub gpu_shared_memory_used: u64,
    pub gpu_temperature: f64,

    // NPU
    pub npu_present: bool,
    pub npu_name: String,
    pub npu_usage_percent: f64,
    pub npu_dedicated_total_bytes: u64,
    pub npu_dedicated_used_bytes: u64,
    pub npu_shared_total_bytes: u64,
    pub npu_shared_used_bytes: u64,

    // Battery
    pub battery_percent: f64,
    pub is_charging: bool,
    pub power_draw_watts: f64,
    pub charge_rate_watts: f64,
    /// Seconds. Not positive when unknown, such as while charging.
    pub battery_time_remaining: i32,
    pub battery_design_capacity_mwh: u32,
    pub battery_full_charge_capacity_mwh: u32,
    pub battery_cycle_count: u32,
    pub battery_voltage: f64,

    pub process_count: u32,
}

impl Snapshot {
    /// Used RAM as a share of total, %.
    pub fn mem_percent(&self) -> f64 {
        if self.total_ram_bytes > 0 {
            self.used_ram_bytes as f64 / self.total_ram_bytes as f64 * 100.0
        } else {
            0.0
        }
    }

    /// Read + write, bytes/s.
    pub fn disk_bps(&self) -> f64 {
        self.disk_read_per_sec + self.disk_write_per_sec
    }

    /// Send + receive, bytes/s.
    pub fn net_bps(&self) -> f64 {
        self.net_send_per_sec + self.net_recv_per_sec
    }
}

/// A fixed-length rolling series for one graph line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Series {
    pub points: VecDeque<f64>,
}

impl Series {
    /// About 2 minutes at the 1 s default cadence.
    pub const CAP: usize = 120;

    pub fn push(&mut self, v: f64) {
        if self.points.len() >= Self::CAP {
            self.points.pop_front();
        }
        self.points.push_back(v);
    }

    pub fn last(&self) -> Option<f64> {
        self.points.back().copied()
    }
}

/// Rolling history for the Performance page graphs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct History {
    /// %
    pub cpu: Series,
    /// %
    pub mem: Series,
    /// Bytes/s, read + write.
    pub disk: Series,
    /// Bytes/s, send + receive.
    pub net: Series,
    /// %
    pub gpu: Series,
    /// %
    pub npu: Series,
}

impl History {
    pub fn record(&mut self, s: &Snapshot) {
        self.cpu.push(s.cpu_usage_percent);
        self.mem.push(s.mem_percent());
        self.disk.push(s.disk_bps());
        self.net.push(s.net_bps());
        self.gpu.push(s.gpu_usage_percent);
        self.npu.push(s.npu_usage_percent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_keeps_the_last_cap_points() {
        let mut s = Series::default();
        for i in 0..(Series::CAP + 5) {
            s.push(i as f64);
        }
        assert_eq!(s.points.len(), Series::CAP);
        assert_eq!(s.points.front().copied(), Some(5.0));
        assert_eq!(s.last(), Some((Series::CAP + 4) as f64));
    }

    #[test]
    fn mem_percent_handles_zero_total() {
        assert_eq!(Snapshot::default().mem_percent(), 0.0);
        let s = Snapshot {
            total_ram_bytes: 200,
            used_ram_bytes: 50,
            ..Snapshot::default()
        };
        assert_eq!(s.mem_percent(), 25.0);
    }
}
