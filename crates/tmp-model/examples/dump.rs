//! Print synthetic telemetry the way the header and Processes page show it.
//! This is a quick way to see the model without any UI.
//!
//!     cargo run -p tmp-model --example dump -- 30

use tmp_model::groups::{self, Sort};
use tmp_model::{Model, Source, Synthetic, derived, fmt};

fn main() {
    let ticks: u32 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(3);

    let mut source = Synthetic::default();
    let mut model = Model::default();
    for _ in 0..ticks {
        model.apply(source.sample());
    }
    let Some(s) = &model.snap else {
        eprintln!("no snapshot after {ticks} ticks");
        return;
    };

    println!(
        "tick {}  |  CPU {:.0}%  Memory {:.0}%  GPU {:.0}%  Disk {}  Net {}",
        model.tick,
        s.cpu_usage_percent,
        s.mem_percent(),
        s.gpu_usage_percent,
        fmt::rate(s.disk_bps()),
        fmt::rate(s.net_bps()),
    );
    if derived::has_battery(s) {
        println!(
            "battery {:.1}%  {}  {:.1} W",
            s.battery_percent,
            if s.is_charging {
                "charging"
            } else {
                "on battery"
            },
            s.power_draw_watts,
        );
    }

    let groups = groups::build_groups(&model.procs, "", Sort::default());
    println!(
        "\n{} apps · {} processes\n",
        groups.len(),
        model.procs.len()
    );
    println!(
        "{:<44} {:>7} {:>9} {:>6} {:>10} {:>10}",
        "Name", "CPU", "Memory", "GPU", "Disk", "Network"
    );
    for g in groups.iter().take(15) {
        let name = if g.is_multi() {
            format!("{} ({})", g.name, g.procs.len())
        } else {
            g.name.to_string()
        };
        println!(
            "{:<44} {:>7} {:>9} {:>6} {:>10} {:>10}",
            fmt::truncate(&name, 44),
            fmt::pct_cell(g.cpu, true).unwrap_or_default(),
            fmt::mb(g.mem),
            fmt::pct_cell(g.gpu, false).unwrap_or_default(),
            fmt::rate_cell(g.disk).unwrap_or_default(),
            fmt::rate_cell(g.net).unwrap_or_default(),
        );
    }
}
