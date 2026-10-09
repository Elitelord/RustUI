//! Settings page.
//!
//! Lite stores its settings at
//! `%LOCALAPPDATA%\com.taskmanagerplus.app\lite-settings.json`. The lab uses the
//! same JSON through `tmp_model::settings`, but keeps the file under
//! `%LOCALAPPDATA%\RustUI-lab\` so the installed app's settings stay untouched.
//!
//! Full-width rows (label + description left, control right), matching the main
//! app's settings layout.

use eframe::egui;
use tmp_model::Poller;
use tmp_model::settings::{LiteSettings, Page, REFRESH_CHOICES};

use super::theme;

pub struct SettingsView {
    pub settings: LiteSettings,
}

impl SettingsView {
    pub fn new(settings: LiteSettings) -> Self {
        Self { settings }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, poller: &Poller) {
        ui.heading(egui::RichText::new("Settings").color(theme::TEXT_PRIMARY));
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Preferences for the lite window. Stored separately from the main app.")
                .color(theme::TEXT_SECONDARY),
        );
        ui.add_space(16.0);

        let mut changed = false;
        let cur_refresh = self.settings.refresh_ms;
        let cur_page = self.settings.default_page.clone();
        let cur_top = self.settings.always_on_top;
        let mut new_refresh = cur_refresh;
        let mut new_page = cur_page.clone();
        let mut new_top = cur_top;

        card(ui, |ui| {
            setting_row(
                ui,
                "Refresh rate",
                "How often telemetry is polled. Slower saves CPU.",
                |ui| {
                    for (label, ms) in REFRESH_CHOICES {
                        if pill(ui, label, cur_refresh == ms).clicked() {
                            new_refresh = ms;
                            changed = true;
                        }
                    }
                },
            );
            divider(ui);
            setting_row(
                ui,
                "Default page",
                "Which page opens when the lite window launches.",
                |ui| {
                    for page in [Page::Processes, Page::Performance, Page::Battery] {
                        let (label, key) = (page.label(), page.key());
                        if pill(ui, label, cur_page == key).clicked() {
                            new_page = key.to_string();
                            changed = true;
                        }
                    }
                },
            );
            divider(ui);
            setting_row(
                ui,
                "Always on top",
                "Keep the lite window above other windows (applies on next launch).",
                |ui| {
                    if toggle(ui, cur_top).clicked() {
                        new_top = !cur_top;
                        changed = true;
                    }
                },
            );
        });

        if changed {
            self.settings.refresh_ms = new_refresh;
            self.settings.default_page = new_page;
            self.settings.always_on_top = new_top;
            poller.set_interval_ms(self.settings.refresh_ms);
            // Same as lite: a failed save keeps the change for this session only.
            let _ = self.settings.save();
        }
    }
}

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .fill(theme::BG_ELEVATED)
        .stroke(egui::Stroke::new(1.0, theme::BORDER_SUBTLE))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::same(4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// A full-width setting row: title + description on the left, control(s) pushed
/// to the right edge.
fn setting_row(ui: &mut egui::Ui, title: &str, desc: &str, control: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(10, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(title).strong().color(theme::TEXT_PRIMARY));
                    ui.label(egui::RichText::new(desc).size(12.0).color(theme::TEXT_SECONDARY));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
            });
        });
}

fn divider(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    let y = ui.cursor().top();
    ui.painter().hline(
        (rect.left() + 10.0)..=(rect.right() - 10.0),
        y,
        egui::Stroke::new(1.0, theme::BORDER_SUBTLE),
    );
}

/// A selectable pill button.
fn pill(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let (bg, fg, border) = if selected {
        (theme::ROW_SELECTED, theme::TEXT_PRIMARY, theme::ACCENT)
    } else {
        (theme::BG_TERTIARY, theme::TEXT_SECONDARY, theme::BORDER_STRONG)
    };
    let btn = egui::Button::new(egui::RichText::new(label).color(fg))
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, border))
        .corner_radius(6.0);
    let r = ui.add(btn);
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

/// A painter-drawn on/off switch.
fn toggle(ui: &mut egui::Ui, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(36.0, 20.0), egui::Sense::click());
    let track = if on { theme::ACCENT } else { theme::BG_TERTIARY };
    let p = ui.painter();
    p.rect_filled(rect, 10.0, track);
    let knob_x = if on { rect.right() - 10.0 } else { rect.left() + 10.0 };
    p.circle_filled(egui::pos2(knob_x, rect.center().y), 7.0, egui::Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}
