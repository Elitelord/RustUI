# egui accesskit repros

Minimal repros for accessibility gaps found while porting TaskManager+ Lite to egui 0.36. Windows only (UI Automation).

```bash
cargo build --release --examples
```

```powershell
./probe.ps1 table
./probe.ps1 table_virtual
./probe.ps1 text_edit -SetValue hello
./probe.ps1 order
```

| Example | Finding | Status |
|---|---|---|
| `text_edit` | `TextEdit` ignores `Action::SetValue`, even though UIA reports the field as writable | Filed: [emilk/egui#8699](https://github.com/emilk/egui/issues/8699). **Fixed** by [emilk/egui#8701](https://github.com/emilk/egui/pull/8701), merged 2026-10-12 (next egui release after 0.36.2) |
| `table`, `table_virtual` | `egui_extras::Table` has no table, row or cell roles, and virtualized tables hide the total row count | Filed: [emilk/egui#8700](https://github.com/emilk/egui/issues/8700) |
| `order` | Children of a `right_to_left` layout are read in creation order, which is the reverse of their visual order | Minor, arguably by design. Not filed. |

Checked against egui `main` at 8f6d3d6 (2026-10-09) by reading the source. No existing issue or PR covers them (searched 2026-10-10).

Not upstream bugs (fix in the app):
- Painter-drawn widgets built from `allocate_exact_size(…, Sense::click())` show up as `Custom` with no name. Give them a role and label.
- Lite's process table lists rows bottom-up in the UIA tree. None of these repros does that, so the cause is something in lite's own table code.
