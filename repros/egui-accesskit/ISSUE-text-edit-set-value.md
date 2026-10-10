<!-- Draft for https://github.com/emilk/egui/issues/new?template=bug_report.md
     Title: TextEdit ignores AccessKit `SetValue`, so UI Automation can't set its text
     Before posting: re-run `./probe.ps1 text_edit -SetValue hello` against egui main if a release has landed since. -->

**Describe the bug**

`TextEdit` doesn't handle `accesskit::Action::SetValue` (or `ReplaceSelectedText`). On Windows, UI Automation exposes the field's `ValuePattern` as writable (`IsReadOnly = false`), and `ValuePattern.SetValue("hello")` returns success, but the text never changes. Speech input, automation tools and some screen reader commands fill fields this way.

`Slider`, `DragValue` and panel resize handles already handle `SetValue`. `TextEdit` only handles `SetTextSelection` (`text_selection/cursor_range.rs`). This is still the case on `main` (8f6d3d6).

**To Reproduce**

```rust
fn main() -> eframe::Result {
    let mut text = String::new();
    eframe::run_ui_native("repro", Default::default(), move |ui, _frame| {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.text_edit_singleline(&mut text);
            ui.label(format!("Current value: {text:?}"));
        });
    })
}
```

With `eframe = { version = "0.36.2", features = ["accesskit"] }`, call `ValuePattern.SetValue` on the `Edit` element, for example with Accessibility Insights for Windows, or with this PowerShell probe: <link to probe.ps1>.

```
ValuePattern.IsReadOnly = False
Calling ValuePattern.SetValue('hello')
Edit value after:  ''
App label after:   Current value: ""
```

**Expected behavior**

The field contains "hello", as if the user had typed it. Alternatively, if setting the value isn't supported, the node shouldn't advertise a writable value.

**Desktop:**
 - OS: Windows 11 (26200)
 - egui / eframe 0.36.2, glow backend, `accesskit` feature

**Additional context**

A fix would probably handle `SetValue` (replace the whole text) and `ReplaceSelectedText` in `TextEdit`, add both actions to its node, and respect `char_limit` and the `TextBuffer` being read-only. `egui_kittest`'s `set_value_accesskit` only sends numeric values today, so a test would need a string variant. Happy to send a PR if that approach sounds right.
