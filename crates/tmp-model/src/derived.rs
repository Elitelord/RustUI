//! Values the pages compute from a snapshot, and the thresholds that color them.

use crate::types::Snapshot;

/// How loaded or healthy a value is. Ports map this to their own colors, or to
/// [`palette::level`](crate::palette::level).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Level {
    Normal,
    Warn,
    Critical,
}

/// Bands for a value where higher means worse.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Threshold {
    pub warn: f64,
    pub crit: f64,
}

impl Threshold {
    pub const fn new(warn: f64, crit: f64) -> Self {
        Self { warn, crit }
    }

    /// Critical above `crit`, Warn above `warn`, otherwise Normal.
    pub fn level(self, value: f64) -> Level {
        if value > self.crit {
            Level::Critical
        } else if value > self.warn {
            Level::Warn
        } else {
            Level::Normal
        }
    }
}

/// Process table CPU column.
pub const TABLE_CPU: Threshold = Threshold::new(25.0, 60.0);
/// Header CPU and GPU stats.
pub const HEADER_LOAD: Threshold = Threshold::new(50.0, 80.0);
/// Header memory stat.
pub const HEADER_MEMORY: Threshold = Threshold::new(60.0, 85.0);
/// Per-core bars on the Performance page.
pub const CORE: Threshold = Threshold::new(50.0, 80.0);

/// Battery charge, where low is bad: below 20 % is Critical, below 50 % is Warn.
pub fn charge_level(percent: f64) -> Level {
    if percent < 20.0 {
        Level::Critical
    } else if percent < 50.0 {
        Level::Warn
    } else {
        Level::Normal
    }
}

/// Battery health, where low is bad: below 60 % is Critical, below 80 % is Warn.
pub fn health_level(percent: f64) -> Level {
    if percent < 60.0 {
        Level::Critical
    } else if percent < 80.0 {
        Level::Warn
    } else {
        Level::Normal
    }
}

/// Windows-style memory breakdown, in bytes. The four parts add up to `total`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MemoryComposition {
    pub in_use: f64,
    pub modified: f64,
    /// Standby list.
    pub cached: f64,
    pub free: f64,
    pub total: f64,
}

impl MemoryComposition {
    pub fn from_snapshot(s: &Snapshot) -> Self {
        let total = s.total_ram_bytes as f64;
        let avail = s.available_ram_bytes as f64;
        let modified = s.modified_pages_bytes as f64;
        let cached = s.cached_bytes as f64;
        Self {
            in_use: (total - avail - modified).max(0.0),
            modified,
            cached,
            free: (avail - cached).max(0.0),
            total,
        }
    }
}

/// Battery power flow in watts, as the Battery page draws it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PowerFlow {
    /// From the adapter: system draw plus charge power while charging, otherwise 0.
    pub input: f64,
    /// System draw.
    pub output: f64,
    /// Into the battery (positive) or out of it (negative).
    pub net: f64,
    /// Full-scale value for the three bars (at least 1 W).
    pub scale: f64,
}

impl PowerFlow {
    pub fn from_snapshot(s: &Snapshot) -> Self {
        let output = s.power_draw_watts;
        let (input, net) = if s.is_charging {
            (s.charge_rate_watts + output, s.charge_rate_watts)
        } else {
            (0.0, -output)
        };
        Self {
            input,
            output,
            net,
            scale: input.max(output).max(net.abs()).max(1.0),
        }
    }
}

/// False on machines without a battery. The Battery page then shows "No battery detected."
pub fn has_battery(s: &Snapshot) -> bool {
    s.battery_design_capacity_mwh > 0
        || s.battery_full_charge_capacity_mwh > 0
        || s.battery_percent > 0.0
}

/// Full-charge capacity as a share of design capacity, %. `None` when either is unknown.
pub fn battery_health_percent(s: &Snapshot) -> Option<f64> {
    (s.battery_design_capacity_mwh > 0 && s.battery_full_charge_capacity_mwh > 0).then(|| {
        f64::from(s.battery_full_charge_capacity_mwh) / f64::from(s.battery_design_capacity_mwh)
            * 100.0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds_are_exclusive_at_the_bounds() {
        assert_eq!(TABLE_CPU.level(25.0), Level::Normal);
        assert_eq!(TABLE_CPU.level(25.1), Level::Warn);
        assert_eq!(TABLE_CPU.level(60.1), Level::Critical);
        assert_eq!(charge_level(19.9), Level::Critical);
        assert_eq!(charge_level(50.0), Level::Normal);
        assert_eq!(health_level(79.0), Level::Warn);
    }

    #[test]
    fn memory_parts_sum_to_total() {
        let s = Snapshot {
            total_ram_bytes: 32_000,
            available_ram_bytes: 12_000,
            modified_pages_bytes: 500,
            cached_bytes: 7_000,
            ..Snapshot::default()
        };
        let m = MemoryComposition::from_snapshot(&s);
        assert_eq!(
            (m.in_use, m.modified, m.cached, m.free),
            (19_500.0, 500.0, 7_000.0, 5_000.0)
        );
        assert_eq!(m.in_use + m.modified + m.cached + m.free, m.total);
    }

    #[test]
    fn power_flow_charging_and_discharging() {
        let mut s = Snapshot {
            power_draw_watts: 12.0,
            charge_rate_watts: 30.0,
            is_charging: true,
            ..Snapshot::default()
        };
        let f = PowerFlow::from_snapshot(&s);
        assert_eq!(
            (f.input, f.output, f.net, f.scale),
            (42.0, 12.0, 30.0, 42.0)
        );

        s.is_charging = false;
        let f = PowerFlow::from_snapshot(&s);
        assert_eq!(
            (f.input, f.output, f.net, f.scale),
            (0.0, 12.0, -12.0, 12.0)
        );
    }

    #[test]
    fn battery_presence_and_health() {
        assert!(!has_battery(&Snapshot::default()));
        let s = Snapshot {
            battery_design_capacity_mwh: 80_000,
            battery_full_charge_capacity_mwh: 71_200,
            ..Snapshot::default()
        };
        assert!(has_battery(&s));
        assert_eq!(battery_health_percent(&s), Some(89.0));
        assert_eq!(battery_health_percent(&Snapshot::default()), None);
    }
}
