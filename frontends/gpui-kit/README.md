# frontends/gpui-kit

TaskManager+ Lite's UI ported to [gpui-kit](https://github.com/longbridge/gpui-kit) 0.8 (from crates.io), running on `tmp-model`. All four lite pages are ported: Processes, Performance, Battery and Settings.

```bash
cargo run --release
```

## Which gpui-kit component each part uses

| Part | gpui-kit component |
|---|---|
| Page navigation | `Sidebar` |
| Process table | `DataTable` with a `TableDelegate` (virtualized, sortable, context menu) |
| Filter box | `Input` |
| Performance sparklines | `AreaChart` |
| Settings | `Settings` with `SettingField::dropdown` and `SettingField::switch` |

## Differences from lite

- **Data** comes from `tmp_model::Synthetic` instead of the native DLL. End task only reports which PIDs it would end.
- **Shared logic:** grouping, sorting, selection, formatting and color thresholds use `tmp-model`, the same code every other port uses.
- **Redraws only on new data.** The poll thread signals a `smol` channel, and a foreground task notifies the views. Nothing redraws in between.
- **Settings** are saved to `%LOCALAPPDATA%\RustUI-lab\lite-settings.json`, not the installed app's file.
- **Lab-only switches:**
  - `TMP_LAB_FILTER=<text>` pre-fills the filter box.
  - `TMP_LAB_PAGE=<processes|performance|battery|settings>` opens a page directly.

  Both are for repros, screenshots and audits.

## Workarounds in this port

Each one marks a gap in gpui-kit 0.8.0 or GPUI.

| Workaround | Why | Status |
|---|---|---|
| Numeric cells right-align themselves (`right()` in `processes.rs`) | `DataTable` ignored `Column::text_right()` | Fixed upstream in longbridge/gpui-kit#3617, after 0.8.0 |
| Rows get an `aria_label`, `aria_expanded` and Expand/Collapse handlers in `render_tr` | Rows had no accessible name and no table structure | Structure and the `TableDelegate::cell_label` API added upstream in longbridge/gpui-kit#3667, after 0.8.0; issue longbridge/gpui-kit#3647 |
| The selection is moved back onto the selected process after every update | The table selects by row index, and `set_selected_row` always scrolls, so the view snaps back to the selection every second | Not yet reported |
| `.with_assets(gpui_kit::assets::Assets)` on the application | Without it every icon renders as blank space, with no warning. The `hello_world` example and the crate docs omit it. | Not yet reported |
| Bar tracks and free memory use `muted_foreground.opacity(0.25)` | In the dark theme, `muted` and `border` are within a few shades of the card background | Cosmetic |
| Always on top sets `HWND_TOPMOST` on the native window | GPUI only has `WindowKind::PopUp`, which on Windows is a captionless tool window with no taskbar button | GPUI gap |

## Accessibility

What Windows UI Automation sees on each page (`bench/a11y.ps1`, debug build, 2026-10-24):

| Page | Elements | Named | Focusable | What's exposed |
|---|---:|---:|---:|---|
| Processes | 29 | 29 | 1 | Rows named through `render_tr`, sort buttons, End task, the filter box |
| Performance | 2 | 2 | 0 | Only the sidebar items |
| Battery | 4 | 4 | 0 | Only the sidebar items |
| Settings | 9 | 9 | 4 | Sidebar, search, and the three controls ("Refresh rate: 1 s", "Default page: Processes", and the "Always on top" switch with its on/off state) |

The main gap: **gpui-kit's `Label`, and plain text in general, isn't exposed**. GPUI only puts text in the tree when it is a `Text` with an id (e.g. from `text!`), and `Label` renders a `StyledText` with no id. So card titles, values, descriptions and the header strip are invisible to screen readers. `AreaChart` sets no role or label either, and sidebar items are named and report which one is active but can't be focused by keyboard.
