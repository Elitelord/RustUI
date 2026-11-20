# frontends/gpui-kit

TaskManager+ Lite's UI ported to [gpui-kit](https://github.com/longbridge/gpui-kit) 0.10.0 (from crates.io), running on `tmp-model`. All four lite pages are ported: Processes, Performance, Battery and Settings.

```bash
cargo run --release
```

## Which gpui-kit component each part uses

| Part | gpui-kit component |
|---|---|
| Page navigation | `Sidebar` |
| Process table | `DataTable` with a `TableDelegate` (virtualized, sortable, context menu; cells named through `cell_label`) |
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

Each one marks a gap in gpui-kit or GPUI.

| Workaround | Why | Status |
|---|---|---|
| Group rows get `aria_expanded` and Expand/Collapse handlers in `render_tr` | The table has no built-in notion of expandable rows | Delegate-side by design |
| The selection is moved back onto the selected process after every update, then the pending scroll is cancelled (`vertical_scroll_handle.0.borrow_mut().deferred_scroll_to_item = None`) | The table selects by row index, and `set_selected_row` scrolls an off-screen row into view. Without the cancel, a table you've scrolled away from snaps back whenever a refresh moves the selected process (checked headlessly 2026-11-04: offset −3840 px → 0). There's no way to select without scrolling except through GPUI's scroll state. | Reported as longbridge/gpui-kit#4044; fixed by #4046 (merged 2026-11-19, not yet released), which stops `set_selected_*` from scrolling. Drop this workaround once that ships. |
| `.with_assets(gpui_kit::assets::Assets)` on the application | Without it every icon renders as blank space, with no warning. The `hello_world` example and the crate docs omit it. | Not yet reported |
| Bar tracks and free memory use `muted_foreground.opacity(0.25)` | In the dark theme, `muted` and `border` are within a few shades of the card background | Cosmetic |
| Always on top sets `HWND_TOPMOST` on the native window | GPUI only has `WindowKind::PopUp`, which on Windows is a captionless tool window with no taskbar button | GPUI gap |

## Accessibility

What Windows UI Automation sees on each page (Processes via `bench/a11y.ps1`; Performance and Battery via a repeated-read UIA probe, see the note below; measured on 0.10.0, 2026-11-19):

| Page | Elements | Named | Focusable | What's exposed |
|---|---:|---:|---:|---|
| Processes | 153 | 109 | 2 | A data grid named "Processes", which takes keyboard focus: named column headers with their sort buttons, then rows of named cells ("Visual Studio Code, 9 processes", "2.8%", "1.77 GB"). Blank cells stay blank. Groups expand and collapse through UIA. Each row is named after its cells ("Visual Studio Code, 9 processes, 2.7%, 1.76 GB, 0.8%"). The filter box, and End task, which is disabled, and so not a Tab stop, until a row is selected. |
| Performance | 89 | 89 | 0 | The sidebar, plus every card's title, value, device name and `kv` detail as text ("CPU", "17.4%", "AMD Ryzen AI 9 HX 370 …", "Cores 12", "Threads 24", …). The `AreaChart` sparklines are still unexposed. |
| Battery | 39 | 39 | 0 | The sidebar, plus the charge headline, power-flow rows, health and every `Details` row as text. |
| Settings | 9 | 9 | 4 | Sidebar, search, and the three controls ("Refresh rate: 1 s", "Default page: Processes", and the "Always on top" switch with its on/off state) |

Up to 0.9.1, **gpui-kit's `Label`, and plain text in general, wasn't exposed**: GPUI only puts text in the tree when it is a `Text` with an id (e.g. from `text!`), and `Label` rendered a `StyledText` with no id, so card titles, values, descriptions and the header strip were invisible to screen readers. The lab now draws all of that text through `Label` (via the `text()` helper in `src/text.rs`) instead of plain `div().child(..)`, and **0.10.0 gives `Label` an accessibility node** (longbridge/gpui-kit#3937, proposed from this lab's findings), so the text is now exposed: Performance went from 2 named elements to 89, Battery from 4 to 39, with no on-screen change. Helpers that build several labels from one line (`kv`, `stat_row`, `flow_row`, the header `stat`) wrap each in an element with its own id, since labels sharing the call-site id would make GPUI's debug builds panic under a screen reader. `AreaChart` is still unexposed.

The single-shot `bench/a11y.ps1` is unreliable for the static Performance and Battery pages: AccessKit populates the tree only after the app redraws, and the lab redraws on poll ticks, so one snapshot often races the first paint (it reported 0–4 elements even on a build that exposes 85). A screen reader stays attached and sees the text; to measure it, read the UIA tree a few times with a short pause between (each read nudges a repaint) rather than taking one snapshot. The Performance and Battery figures above come from that repeated read.

### Upstream changes

- **0.9.0** (longbridge/gpui-kit#3727 and #3758): the grid holds keyboard focus, so Tab or a click into the table moves UI Automation's focus to it, and focus then follows the selected row as the arrows move. On 0.8.1 focus stayed on the window. The lab names the grid with `DataTable::accessibility_label("Processes")`. Disabled buttons are no longer Tab stops.
- **Keyboard check on 0.9.0** (2026-11-03, UI Automation focus logged after keys sent with SendKeys): Tab from the filter box reaches the grid, and the arrow keys then move UIA focus row by row. Two gaps remain, both reproduced headlessly in gpui-kit's own tests and drafted as upstream issues:
  - Tab does nothing at startup, when nothing is focused, because Root's Tab binding needs a focused element to dispatch from.
  - Once the grid has focus, Tab, Shift+Tab and Escape can't leave it: `DataTable` binds Tab to "select next column". Reported as longbridge/gpui-kit#3788 and fixed in 0.9.1 by #3800 (checked in the lab on the PR's branch, 2026-11-04).
- **0.9.1** (longbridge/gpui-kit#3773): rows are named after their cells, with no lab change, since the lab already implements `cell_label`. Named elements on Processes went from 88 to 109.
- **0.10.0** (longbridge/gpui-kit#3937): `Label` exposes its text to assistive technology. The lab draws all its plain text through `Label` (the `text()` helper), so Performance and Battery now expose their cards' text (2 → 89 and 4 → 39 named). The issue and fix came from this lab's audit.
- **Pending** (longbridge/gpui-kit#4046, merged, not yet released): `set_selected_*` no longer scrolls the selection into view. Once released, the lab can drop its `sync_selection` workaround that clears GPUI's `deferred_scroll_to_item` (see the workarounds table). Fixes the lab's #4044.

### Removed workarounds

- **0.8.1** made two lab workarounds unnecessary:
  - `DataTable` now applies `Column::text_right()` (longbridge/gpui-kit#3617), so the numeric columns no longer align themselves.
  - It now exposes grid, header and cell roles, and `TableDelegate::cell_label` names cells (longbridge/gpui-kit#3667). On 0.8.0 the lab put one sentence per row in an `aria_label`, which got 29 elements named. On 0.8.1 the table exposes 153 elements: 88 named, the rest blank cells.
