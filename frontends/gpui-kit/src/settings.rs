//! Settings page, built on gpui-kit's `Settings` component.
//!
//! The lab keeps its settings in `%LOCALAPPDATA%\RustUI-lab\lite-settings.json`
//! through `tmp_model::settings`, the same JSON lite uses, so the installed
//! app's settings stay untouched.

use std::rc::Rc;

use gpui_kit::component::setting::{
    SettingField, SettingGroup, SettingItem, SettingPage, Settings,
};
use gpui_kit::*;
use tmp_model::Poller;
use tmp_model::settings::{LiteSettings, Page, REFRESH_CHOICES};

pub struct SettingsView {
    settings: LiteSettings,
    /// Shared with the window: a new refresh rate retimes the poll thread.
    poller: Rc<Poller>,
}

impl SettingsView {
    pub fn new(settings: LiteSettings, poller: Rc<Poller>) -> Self {
        Self { settings, poller }
    }

    /// Apply a change and save it. As in lite, a failed save keeps the change
    /// for this session only.
    fn update(&mut self, change: impl FnOnce(&mut LiteSettings), cx: &mut Context<Self>) {
        change(&mut self.settings);
        self.poller.set_interval_ms(self.settings.refresh_ms);
        let _ = self.settings.save();
        cx.notify();
    }
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();

        let refresh = SettingField::dropdown(
            REFRESH_CHOICES
                .iter()
                .map(|(label, ms)| (ms.to_string().into(), (*label).into()))
                .collect(),
            {
                let view = view.clone();
                move |cx: &App| view.read(cx).settings.refresh_ms.to_string().into()
            },
            {
                let view = view.clone();
                move |value: SharedString, cx: &mut App| {
                    if let Ok(ms) = value.parse() {
                        view.update(cx, |this, cx| this.update(|s| s.refresh_ms = ms, cx));
                    }
                }
            },
        );

        let default_page = SettingField::dropdown(
            [Page::Processes, Page::Performance, Page::Battery]
                .into_iter()
                .map(|page| (page.key().into(), page.label().into()))
                .collect(),
            {
                let view = view.clone();
                move |cx: &App| view.read(cx).settings.default_page.clone().into()
            },
            {
                let view = view.clone();
                move |value: SharedString, cx: &mut App| {
                    view.update(cx, |this, cx| {
                        this.update(|s| s.default_page = value.to_string(), cx)
                    });
                }
            },
        );

        let always_on_top = SettingField::switch(
            {
                let view = view.clone();
                move |cx: &App| view.read(cx).settings.always_on_top
            },
            {
                let view = view.clone();
                move |on: bool, cx: &mut App| {
                    view.update(cx, |this, cx| this.update(|s| s.always_on_top = on, cx));
                }
            },
        );

        let page = SettingPage::new("Lite window")
            .description("Preferences for the lite window. Stored separately from the main app.")
            .default_open(true)
            .group(SettingGroup::new().items(vec![
                    SettingItem::new("Refresh rate", refresh)
                        .description("How often telemetry is polled. Slower saves CPU."),
                    SettingItem::new("Default page", default_page)
                        .description("Which page opens when the lite window launches."),
                    SettingItem::new("Always on top", always_on_top).description(
                        "Keep the lite window above other windows (applies on next launch).",
                    ),
                ]));

        Settings::new("settings").page(page)
    }
}
