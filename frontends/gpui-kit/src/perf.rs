//! Performance page: one card per subsystem (CPU, Memory, GPU, Disk, Network,
//! and NPU when present).
//!
//! Each card pairs a gpui-kit `AreaChart` sparkline over the rolling `History`
//! with the details lite's Performance page shows: per-core CPU, a
//! Windows-style memory composition bar, device identity, clocks, capacities,
//! and the read/write and send/receive breakdowns.

use gpui_kit::component::chart::AreaChart;
use gpui_kit::component::{ActiveTheme as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use tmp_model::derived::{self, MemoryComposition};
use tmp_model::fmt::{self, gb, ghz, kb};
use tmp_model::{CoreCpu, History, Series, Snapshot, palette};

fn color(rgb: palette::Rgb) -> Hsla {
    gpui_kit::rgb(rgb.hex()).into()
}

pub fn page(s: &Snapshot, hist: &History, per_core: &[CoreCpu], cx: &App) -> impl IntoElement {
    v_flex()
        .id("performance")
        .size_full()
        .gap_3()
        .overflow_y_scroll()
        .child(cpu_card(s, hist, per_core, cx))
        .child(memory_card(s, hist, cx))
        .child(gpu_card(s, hist, cx))
        .child(disk_card(s, hist, cx))
        .child(network_card(s, hist, cx))
        .when(s.npu_present, |this| this.child(npu_card(s, hist, cx)))
}

// -----------------------------------------------------------------------------
// Cards
// -----------------------------------------------------------------------------

fn cpu_card(s: &Snapshot, hist: &History, per_core: &[CoreCpu], cx: &App) -> Div {
    let c = color(palette::CPU);
    let l1 = s.l1d_cache_kb + s.l1i_cache_kb;
    card(cx)
        .child(head(
            "CPU",
            format!("{:.1}%", s.cpu_usage_percent),
            c,
            &s.cpu_name,
            cx,
        ))
        .child(sparkline("spark-cpu", &hist.cpu, c, true, cx))
        .child(
            details()
                .child(kv("Cores", s.core_count.to_string(), cx))
                .child(kv("Threads", s.thread_count.to_string(), cx))
                .when(s.cpu_base_frequency_mhz > 0.0, |d| {
                    d.child(kv("Base", ghz(s.cpu_base_frequency_mhz), cx))
                })
                .when(s.cpu_frequency_mhz > 0.0, |d| {
                    d.child(kv("Current", ghz(s.cpu_frequency_mhz), cx))
                })
                .when(s.cpu_max_frequency_mhz > 0.0, |d| {
                    d.child(kv("Max", ghz(s.cpu_max_frequency_mhz), cx))
                })
                .when(l1 > 0, |d| d.child(kv("L1", kb(l1), cx)))
                .when(s.l2_cache_kb > 0, |d| {
                    d.child(kv("L2", kb(s.l2_cache_kb), cx))
                })
                .when(s.l3_cache_kb > 0, |d| {
                    d.child(kv("L3", kb(s.l3_cache_kb), cx))
                }),
        )
        .when(!per_core.is_empty(), |card| {
            card.child(core_grid(per_core, cx))
        })
}

fn memory_card(s: &Snapshot, hist: &History, cx: &App) -> Div {
    let c = color(palette::MEMORY);
    let m = MemoryComposition::from_snapshot(s);
    let free_color = track(cx);
    let segments = [
        ("In use", m.in_use, color(palette::MEM_IN_USE)),
        ("Modified", m.modified, color(palette::MEM_MODIFIED)),
        ("Cached", m.cached, color(palette::MEM_CACHED)),
        ("Free", m.free, free_color),
    ];
    let value = format!(
        "{:.1} / {:.1} GB  ·  {:.0}%",
        gb(s.used_ram_bytes),
        gb(s.total_ram_bytes),
        s.mem_percent()
    );
    card(cx)
        .child(head("Memory", value, c, "", cx))
        .child(sparkline("spark-mem", &hist.mem, c, true, cx))
        .child(stacked_bar(&segments, m.total.max(1.0), cx))
        .child(
            details().children(segments.iter().map(|(label, bytes, swatch)| {
                h_flex()
                    .gap_1()
                    .child(div().size(px(9.)).rounded_sm().bg(*swatch))
                    .child(kv(label, fmt::bytes(*bytes), cx))
            })),
        )
        .child(
            details()
                .child(kv(
                    "Available",
                    fmt::bytes(s.available_ram_bytes as f64),
                    cx,
                ))
                .when(s.commit_limit_bytes > 0, |d| {
                    d.child(kv(
                        "Committed",
                        format!(
                            "{} / {}",
                            fmt::bytes(s.committed_bytes as f64),
                            fmt::bytes(s.commit_limit_bytes as f64)
                        ),
                        cx,
                    ))
                })
                .when(s.paged_pool_bytes > 0, |d| {
                    d.child(kv("Paged pool", fmt::bytes(s.paged_pool_bytes as f64), cx))
                })
                .when(s.non_paged_pool_bytes > 0, |d| {
                    d.child(kv(
                        "Non-paged pool",
                        fmt::bytes(s.non_paged_pool_bytes as f64),
                        cx,
                    ))
                }),
        )
}

fn gpu_card(s: &Snapshot, hist: &History, cx: &App) -> Div {
    let c = color(palette::GPU);
    card(cx)
        .child(head(
            "GPU",
            format!("{:.1}%", s.gpu_usage_percent),
            c,
            &s.gpu_name,
            cx,
        ))
        .child(sparkline("spark-gpu", &hist.gpu, c, true, cx))
        .child(
            details()
                .child(kv(
                    "Type",
                    if s.gpu_is_integrated {
                        "Integrated"
                    } else {
                        "Discrete"
                    }
                    .to_string(),
                    cx,
                ))
                .when(s.gpu_memory_total > 0, |d| {
                    d.child(kv(
                        "VRAM",
                        format!(
                            "{} / {}",
                            fmt::bytes(s.gpu_memory_used as f64),
                            fmt::bytes(s.gpu_memory_total as f64)
                        ),
                        cx,
                    ))
                })
                .when(s.gpu_shared_memory_total > 0, |d| {
                    d.child(kv(
                        "Shared",
                        format!(
                            "{} / {}",
                            fmt::bytes(s.gpu_shared_memory_used as f64),
                            fmt::bytes(s.gpu_shared_memory_total as f64)
                        ),
                        cx,
                    ))
                })
                .when(s.gpu_temperature > 0.0, |d| {
                    d.child(kv("Temp", format!("{:.0}°C", s.gpu_temperature), cx))
                })
                .when(s.gpu_usage_3d_percent > 0.0, |d| {
                    d.child(kv("3D", format!("{:.0}%", s.gpu_usage_3d_percent), cx))
                })
                .when(s.gpu_usage_compute_percent > 0.0, |d| {
                    d.child(kv(
                        "Compute",
                        format!("{:.0}%", s.gpu_usage_compute_percent),
                        cx,
                    ))
                }),
        )
}

fn disk_card(s: &Snapshot, hist: &History, cx: &App) -> Div {
    let c = color(palette::DISK);
    card(cx)
        .child(head(
            "Disk",
            fmt::rate(s.disk_read_per_sec + s.disk_write_per_sec),
            c,
            "",
            cx,
        ))
        .child(sparkline("spark-disk", &hist.disk, c, false, cx))
        .child(
            details()
                .child(kv("Read", fmt::rate(s.disk_read_per_sec), cx))
                .child(kv("Write", fmt::rate(s.disk_write_per_sec), cx))
                .child(kv("Active", format!("{:.0}%", s.disk_active_percent), cx))
                .child(kv(
                    "Queue",
                    format!("{:.2}", s.disk_queue_length as f64),
                    cx,
                )),
        )
}

fn network_card(s: &Snapshot, hist: &History, cx: &App) -> Div {
    let c = color(palette::NETWORK);
    card(cx)
        .child(head(
            "Network",
            fmt::rate(s.net_send_per_sec + s.net_recv_per_sec),
            c,
            "",
            cx,
        ))
        .child(sparkline("spark-net", &hist.net, c, false, cx))
        .child(
            details()
                .child(kv("Send", fmt::rate(s.net_send_per_sec), cx))
                .child(kv("Receive", fmt::rate(s.net_recv_per_sec), cx))
                .when(s.net_link_speed_bps > 0.0, |d| {
                    d.child(kv("Link speed", fmt::rate(s.net_link_speed_bps), cx))
                }),
        )
}

fn npu_card(s: &Snapshot, hist: &History, cx: &App) -> Div {
    let c = color(palette::NPU);
    card(cx)
        .child(head(
            "NPU",
            format!("{:.1}%", s.npu_usage_percent),
            c,
            &s.npu_name,
            cx,
        ))
        .child(sparkline("spark-npu", &hist.npu, c, true, cx))
        .child(
            details()
                .when(s.npu_dedicated_total_bytes > 0, |d| {
                    d.child(kv(
                        "Dedicated",
                        format!(
                            "{} / {}",
                            fmt::bytes(s.npu_dedicated_used_bytes as f64),
                            fmt::bytes(s.npu_dedicated_total_bytes as f64)
                        ),
                        cx,
                    ))
                })
                .when(s.npu_shared_total_bytes > 0, |d| {
                    d.child(kv(
                        "Shared",
                        format!(
                            "{} / {}",
                            fmt::bytes(s.npu_shared_used_bytes as f64),
                            fmt::bytes(s.npu_shared_total_bytes as f64)
                        ),
                        cx,
                    ))
                }),
        )
}

// -----------------------------------------------------------------------------
// Building blocks
// -----------------------------------------------------------------------------

/// The empty part of a bar, and free memory. The dark theme's `muted` and
/// `border` are both within a few shades of the card, so a translucent
/// foreground is used to stay visible on any theme.
fn track(cx: &App) -> Hsla {
    cx.theme().muted_foreground.opacity(0.25)
}

fn card(cx: &App) -> Div {
    v_flex()
        .w_full()
        .flex_shrink_0()
        .gap_2()
        .p_3()
        .rounded_lg()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().secondary)
}

/// Card header: title on the left; the current value and the device name on
/// the right.
fn head(title: &str, value: String, value_color: Hsla, subtitle: &str, cx: &App) -> Div {
    h_flex()
        .w_full()
        .gap_3()
        .child(div().font_semibold().child(title.to_string()))
        .child(div().flex_1())
        .when(!subtitle.is_empty(), |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .truncate()
                    .child(subtitle.to_string()),
            )
        })
        .child(
            div()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(value_color)
                .child(value),
        )
}

/// A two-minute sparkline over one `History` series. Percent series are
/// pinned to 0–100; rates scale to their own maximum.
fn sparkline(id: &'static str, series: &Series, stroke: Hsla, percent: bool, cx: &App) -> Div {
    let points: Vec<(SharedString, f64)> = series
        .points
        .iter()
        .enumerate()
        .map(|(i, v)| (SharedString::from(i.to_string()), *v))
        .collect();
    let chart = AreaChart::new(points)
        .id(id)
        .x(|(x, _)| x.clone())
        .y(|(_, y)| *y)
        .stroke(stroke)
        .fill(linear_gradient(
            0.,
            linear_color_stop(stroke.opacity(0.35), 1.),
            linear_color_stop(cx.theme().secondary.opacity(0.), 0.),
        ))
        .x_axis(false)
        .grid(false)
        .point_count(Series::CAP)
        .interactive(false)
        .appear(false);
    let chart = if percent {
        chart.y_domain(0.0, 100.0)
    } else {
        chart
    };
    div().w_full().h(px(56.)).child(chart)
}

/// A wrapping row of key/value details.
fn details() -> Div {
    h_flex().flex_wrap().gap_x_4().gap_y_1()
}

fn kv(key: &str, value: String, cx: &App) -> Div {
    h_flex()
        .gap_1()
        .text_xs()
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child(key.to_string()),
        )
        .child(
            div()
                .font_family(cx.theme().mono_font_family.clone())
                .child(value),
        )
}

/// One horizontal bar of proportional segments.
fn stacked_bar(segments: &[(&str, f64, Hsla)], total: f64, cx: &App) -> Div {
    h_flex()
        .w_full()
        .h(px(14.))
        .rounded_sm()
        .overflow_hidden()
        .bg(track(cx))
        .children(
            segments
                .iter()
                .filter(|(_, bytes, _)| *bytes > 0.0)
                .map(|(_, bytes, fill)| {
                    div()
                        .h_full()
                        .w(relative((*bytes / total) as f32))
                        .bg(*fill)
                }),
        )
}

/// Per-core usage as a wrapped row of vertical bars, colored by load.
fn core_grid(cores: &[CoreCpu], cx: &App) -> Div {
    h_flex()
        .flex_wrap()
        .gap_1()
        .children(cores.iter().map(|core| {
            let fill = (core.usage_percent / 100.0).clamp(0.0, 1.0) as f32;
            div()
                .relative()
                .w(px(12.))
                .h(px(32.))
                .rounded_sm()
                .overflow_hidden()
                .bg(track(cx))
                .child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .w_full()
                        .h(relative(fill))
                        .bg(color(palette::level(
                            derived::CORE.level(core.usage_percent),
                        ))),
                )
        }))
}
