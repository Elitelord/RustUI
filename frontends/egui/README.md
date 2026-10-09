# frontends/egui

TaskManager+ Lite's own egui UI (`TaskManagerPlus/src-tauri/src/lite`), ported onto `tmp-model` and upgraded from egui 0.29 to 0.36.

```bash
cargo run --release
```

Build without screen reader support, for comparison:

```bash
cargo build --release --no-default-features --target-dir target/no-accesskit
```

## Differences from lite

- **Data** comes from `tmp_model::Synthetic` instead of the native DLL. End task only reports which PIDs it would end.
- **Shared logic:** grouping, sorting, selection, formatting and color thresholds use `tmp-model`, the same code every other port uses.
- **Upgraded to egui 0.36** (`App::ui`, `egui::Panel`, `Frame::NONE`, integer margins, `corner_radius`). It keeps the glow renderer, like lite. accesskit is on by default.
- **Fixed the Name column widening.** It's `Column::remainder()`, and egui_extras grows non-clipping columns to fit their widest visible cell. Any name too long for the column pushed every column right when it scrolled into view. The column now clips and long names end in "…". The installed lite has the same bug.
- **Settings** are saved to `%LOCALAPPDATA%\RustUI-lab\lite-settings.json`, not the installed app's file.
- `TMP_LAB_FILTER=<text>` pre-fills the filter box, for repros and screenshots.
