//! Display formatting shared by every port. These are the same strings lite
//! produces (`lite/fmt.rs`, plus helpers from `perf.rs` and `battery.rs`).

const KIB: f64 = 1024.0;
const MIB: f64 = 1_048_576.0;
const GIB: f64 = 1_073_741_824.0;

/// Bytes/s → "512 B/s", "1.5 KB/s", "12.0 MB/s", "1.20 GB/s".
pub fn rate(bps: f64) -> String {
    if bps < KIB {
        format!("{bps:.0} B/s")
    } else if bps < MIB {
        format!("{:.1} KB/s", bps / KIB)
    } else if bps < GIB {
        format!("{:.1} MB/s", bps / MIB)
    } else {
        format!("{:.2} GB/s", bps / GIB)
    }
}

/// Bytes → "512 B", "12 KB", "640 MB", "3.4 GB".
pub fn bytes(b: f64) -> String {
    if b < KIB {
        format!("{b:.0} B")
    } else if b < MIB {
        format!("{:.0} KB", b / KIB)
    } else if b < GIB {
        format!("{:.0} MB", b / MIB)
    } else {
        format!("{:.1} GB", b / GIB)
    }
}

/// Megabytes → "640 MB", switching to "1.25 GB" from 1024 MB up.
pub fn mb(mb: f64) -> String {
    if mb >= 1024.0 {
        format!("{:.2} GB", mb / 1024.0)
    } else {
        format!("{mb:.0} MB")
    }
}

/// Bytes → GB as a number, for "18.4 / 31.1 GB" style labels.
pub fn gb(bytes: u64) -> f64 {
    bytes as f64 / GIB
}

/// MHz → "3.45 GHz".
pub fn ghz(mhz: f64) -> String {
    format!("{:.2} GHz", mhz / 1000.0)
}

/// Cache size in KB → "48 KB" or "24 MB".
pub fn kb(kb: u32) -> String {
    if kb >= 1024 {
        format!("{:.0} MB", f64::from(kb) / 1024.0)
    } else {
        format!("{kb} KB")
    }
}

/// Seconds → "2h 5m" or "45m".
pub fn secs(secs: i32) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

/// Truncate to `max` characters, ending in "…" when cut. Splits only on char boundaries.
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

/// Table percentage cell such as "12.3%". When `show_zero` is false, near-zero
/// values return `None` and the cell is left blank. Lite does this for GPU,
/// which reads 0 for almost every process.
pub fn pct_cell(value: f64, show_zero: bool) -> Option<String> {
    (show_zero || value >= 0.05).then(|| format!("{value:.1}%"))
}

/// Table bytes/s cell. Returns `None` (blank) when idle, which keeps the columns quiet.
pub fn rate_cell(bps: f64) -> Option<String> {
    (bps >= 1.0).then(|| rate(bps))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_and_sizes() {
        assert_eq!(rate(512.0), "512 B/s");
        assert_eq!(rate(1536.0), "1.5 KB/s");
        assert_eq!(rate(12.0 * MIB), "12.0 MB/s");
        assert_eq!(rate(1.2 * GIB), "1.20 GB/s");
        assert_eq!(bytes(640.0 * MIB), "640 MB");
        assert_eq!(bytes(3.4 * GIB), "3.4 GB");
        assert_eq!(mb(640.4), "640 MB");
        assert_eq!(mb(1280.0), "1.25 GB");
        assert_eq!(kb(48), "48 KB");
        assert_eq!(kb(24_576), "24 MB");
        assert_eq!(ghz(3450.0), "3.45 GHz");
    }

    #[test]
    fn durations() {
        assert_eq!(secs(45 * 60), "45m");
        assert_eq!(secs(2 * 3600 + 5 * 60 + 9), "2h 5m");
    }

    #[test]
    fn truncate_counts_chars_not_bytes() {
        assert_eq!(truncate("héllo", 5), "héllo");
        assert_eq!(truncate("héllo world", 5), "héll…");
    }

    #[test]
    fn blank_cells() {
        assert_eq!(pct_cell(0.0, true).as_deref(), Some("0.0%"));
        assert_eq!(pct_cell(0.01, false), None);
        assert_eq!(pct_cell(2.34, false).as_deref(), Some("2.3%"));
        assert_eq!(rate_cell(0.5), None);
        assert_eq!(rate_cell(2048.0).as_deref(), Some("2.0 KB/s"));
    }
}
