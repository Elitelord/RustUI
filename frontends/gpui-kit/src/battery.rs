//! Battery page: two columns that fill the page width.
//!
//! The left column has the charge headline, a charge bar, the power-flow bars
//! (input / output / net) and a battery-health bar; the right column lists the
//! numeric details. Everything comes from the performance snapshot.

use gpui_kit::component::{ActiveTheme as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use tmp_model::Snapshot;
use tmp_model::derived::{self, PowerFlow};
use tmp_model::fmt::secs as fmt_secs;
use tmp_model::palette;

use crate::text::text;

fn color(rgb: palette::Rgb) -> Hsla {
    gpui_kit::rgb(rgb.hex()).into()
}

pub fn page(s: &Snapshot, cx: &App) -> impl IntoElement {
    let title = text("Battery").text_xl().font_semibold();
    if !derived::has_battery(s) {
        return v_flex()
            .gap_3()
            .child(title)
            .child(text("No battery detected.").text_color(cx.theme().muted_foreground));
    }
    v_flex().size_full().gap_3().child(title).child(
        h_flex()
            .w_full()
            .items_start()
            .gap_3()
            .child(visuals(s, cx))
            .child(details(s, cx)),
    )
}

fn visuals(s: &Snapshot, cx: &App) -> Stateful<Div> {
    let pct = s.battery_percent;
    let charge = color(palette::level(derived::charge_level(pct)));
    let flow = PowerFlow::from_snapshot(s);
    let net = if flow.net >= 0.0 {
        palette::NORMAL
    } else {
        palette::CRITICAL
    };
    card("charge", cx)
        .child(
            h_flex()
                .gap_3()
                .items_baseline()
                .child(
                    text(format!("{pct:.0}%"))
                        .text_3xl()
                        .font_semibold()
                        .text_color(charge),
                )
                .child(
                    text(if s.is_charging {
                        "Charging"
                    } else {
                        "On battery"
                    })
                    .text_color(cx.theme().muted_foreground),
                ),
        )
        .child(bar((pct / 100.0) as f32, charge, 10., cx))
        .child(text("Power flow").pt_3().font_semibold())
        // System draw is the output. While charging the adapter also supplies
        // the charge power, so input = draw + charge rate. Net is the battery's
        // flow: positive into it while charging, negative while discharging.
        .child(flow_row(
            "Input",
            flow.input,
            flow.scale,
            color(palette::NORMAL),
            cx,
        ))
        .child(flow_row(
            "Output",
            flow.output,
            flow.scale,
            color(palette::WARN),
            cx,
        ))
        .child(flow_row("Net", flow.net, flow.scale, color(net), cx))
        .when_some(derived::battery_health_percent(s), |card, health| {
            let level = color(palette::level(derived::health_level(health)));
            card.child(
                h_flex()
                    .pt_3()
                    .child(text("Battery health").flex_1().font_semibold())
                    .child(
                        text(format!("{health:.0}%"))
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_color(level),
                    ),
            )
            .child(bar((health / 100.0) as f32, level, 8., cx))
        })
}

fn details(s: &Snapshot, cx: &App) -> Stateful<Div> {
    let wh = |mwh: u32| format!("{:.1} Wh", mwh as f64 / 1000.0);
    card("details", cx)
        .child(text("Details").font_semibold())
        .when(!s.is_charging && s.battery_time_remaining > 0, |c| {
            c.child(stat_row(
                "Estimated time left",
                fmt_secs(s.battery_time_remaining),
                cx,
            ))
        })
        .when(s.is_charging && s.charge_rate_watts > 0.5, |c| {
            c.child(stat_row(
                "Charge rate",
                format!("{:.1} W", s.charge_rate_watts),
                cx,
            ))
        })
        .when(s.power_draw_watts > 0.0, |c| {
            c.child(stat_row(
                "System power draw",
                format!("{:.1} W", s.power_draw_watts),
                cx,
            ))
        })
        .when(s.battery_full_charge_capacity_mwh > 0, |c| {
            c.child(stat_row(
                "Full charge capacity",
                wh(s.battery_full_charge_capacity_mwh),
                cx,
            ))
        })
        .when(s.battery_design_capacity_mwh > 0, |c| {
            c.child(stat_row(
                "Design capacity",
                wh(s.battery_design_capacity_mwh),
                cx,
            ))
        })
        .when(s.battery_cycle_count > 0, |c| {
            c.child(stat_row(
                "Cycle count",
                s.battery_cycle_count.to_string(),
                cx,
            ))
        })
        .when(s.battery_voltage > 0.0, |c| {
            c.child(stat_row(
                "Voltage",
                format!("{:.2} V", s.battery_voltage),
                cx,
            ))
        })
}

// -----------------------------------------------------------------------------

/// The empty part of a bar. The dark theme's `muted` and `border` are both
/// within a few shades of the card, so a translucent foreground is used.
fn track(cx: &App) -> Hsla {
    cx.theme().muted_foreground.opacity(0.25)
}

/// A card. Its id keeps one card's labels apart from the other's.
fn card(id: &'static str, cx: &App) -> Stateful<Div> {
    v_flex()
        .id(id)
        .flex_1()
        .gap_2()
        .p_3()
        .rounded_lg()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().secondary)
}

/// One detail. Labels are unique within the card, so the label is the row's id.
fn stat_row(label: &str, value: String, cx: &App) -> Stateful<Div> {
    h_flex()
        .id(SharedString::from(label.to_string()))
        .child(
            text(label.to_string())
                .flex_1()
                .text_color(cx.theme().muted_foreground),
        )
        .child(text(value).font_family(cx.theme().mono_font_family.clone()))
}

/// One power-flow row: label, bar, watts.
fn flow_row(label: &str, watts: f64, scale: f64, fill: Hsla, cx: &App) -> Stateful<Div> {
    let frac = (watts.abs() / scale).clamp(0.0, 1.0) as f32;
    h_flex()
        .id(SharedString::from(label.to_string()))
        .gap_2()
        .child(
            text(label.to_string())
                .w(px(52.))
                .text_color(cx.theme().muted_foreground),
        )
        .child(div().flex_1().child(bar(frac, fill, 10., cx)))
        .child(
            text(format!("{watts:.1} W"))
                .w(px(64.))
                .text_right()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(fill),
        )
}

fn bar(frac: f32, fill: Hsla, height: f32, cx: &App) -> Div {
    div()
        .w_full()
        .h(px(height))
        .rounded_full()
        .overflow_hidden()
        .bg(track(cx))
        .child(
            div()
                .h_full()
                .w(relative(frac.clamp(0.0, 1.0)))
                .rounded_full()
                .bg(fill),
        )
}
