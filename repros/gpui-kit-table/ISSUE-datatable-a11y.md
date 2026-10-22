<!-- Draft for longbridge/gpui-kit. Title: -->
<!-- DataTable: rows are unnamed to screen readers, with no table structure, header cells or keyboard focus -->

## Description

On Windows, through AccessKit and UI Automation, `DataTable` exposes each visible row as an unnamed `DataItem` with nothing inside it. A screen reader announces "data item" for every row and can't read any cell, column name, or the row's position in the table.

What is exposed today:
- The row role and selected state, set in [`state.rs:2199`](https://github.com/longbridge/gpui-kit/blob/8b3631f0/crates/component/src/table/state.rs#L2199): `.role(Role::Row).aria_selected(..)`.
- A "Sort by …" button for each sortable column, set in [`state.rs:1798`](https://github.com/longbridge/gpui-kit/blob/8b3631f0/crates/component/src/table/state.rs#L1798).

Nothing else from the table reaches the accessibility tree.

## Environment

- GPUI: gpui-pre 0.3.9
- GPUI Component: 0.8.0 (also `main` at 8b3631f0)
- Platform: Windows 11, UI Automation. I read the tree with UIA's control view (`TreeWalker.ControlViewWalker`) and checked it with Accessibility Insights. I haven't tested VoiceOver on macOS.

## Steps to Reproduce

1. `cargo run -p gpui-component-story -- datatable`
2. Inspect the window with Accessibility Insights for Windows or Narrator.

The DataTable story's tree (excerpt; the toolbar and menu bar above it are fine):

```text
Button "Sort by Symbol"
Button "Sort by Price"
Button "Sort by Chg"
Button "Sort by Chg%"
DataItem ""
DataItem ""
DataItem ""
… 14 unnamed DataItems in total, one per visible row, none with children
```

## Expected

A screen reader can identify the table and its column headers, read each row's cells, know which row is selected, and move through the rows by keyboard. That's what a native list view or data grid provides.

## Actual

1. **No table container.** No element has `Role::Table` or `Role::Grid`, so there's no row or column count and assistive tools don't know the rows form a table. The rows' `aria_selected` isn't reported either: they expose no `SelectionItemPattern`. Selection state normally needs a selectable container (grid, list) to be reported.
2. **Rows have no name and no content.** GPUI only puts text in the tree when it is a `Text` with an id (e.g. via the `text!` macro) or an `InteractiveText`. Delegates usually return plain strings from `render_td`, and the cell containers have no role, so nothing inside a row reaches the tree.
3. **Column headers aren't exposed.** There are no `Role::ColumnHeader` nodes and no column names. Only sortable columns appear, as "Sort by …" buttons, and those buttons aren't keyboard-focusable.
4. **No keyboard focus in the tree.** Tab moves focus into the table (the focus outline draws), but no node in the table is focusable or focused; UIA's focused element stays on the window. This looks like the same pattern as #3183 for `Input`.
5. **Expandable rows** (grouped or tree-like data) have no way to report expanded/collapsed state unless the delegate adds it.

## What a delegate can do today

`render_tr` returns the row element before the table adds `Role::Row`, so a delegate can already attach a label, expanded state and actions to rows. I verified this in an app with grouped rows (a process list):

```rust
fn render_tr(&mut self, row_ix: usize, _: &mut Window, cx: &mut Context<TableState<Self>>) -> Stateful<Div> {
    let row = &self.rows[row_ix];
    let table = cx.entity().downgrade();
    let name = row.name.clone();
    div()
        .id(("row", row_ix))
        .aria_label(row.accessible_label()) // e.g. "Microsoft Edge, 8 processes, CPU 0.5%, Memory 1021 MB"
        .aria_expanded(row.expanded)
        .on_a11y_action(Action::Expand, move |_, _, cx| {
            let _ = table.update(cx, |table, cx| {
                table.delegate_mut().toggle(&name);
                cx.notify();
            });
        })
}
```

- **Names:** named elements went from 8 of 29 to 29 of 29.
- **Expand/collapse:** group rows expose `ExpandCollapsePattern`, and calling `Expand()` / `Collapse()` through UI Automation opens and closes the group (2 rows → 10 → 2).

A delegate can't add the table container, header cells, cell roles or focus, because the table renders those.

## Suggested direction

I'm happy to implement this, in whatever order and shape you prefer:

1. **Container:** `Role::Table` on the table root (or `Role::Grid` when rows or cells are selectable), with `aria_row_count(rows_count)` and `aria_column_count(columns_count)`. The table is virtualized, so absolute counts and indices tell a screen reader where it is in the full list.
2. **Header cells:** `Role::ColumnHeader` with `aria_label(column.name)` and `aria_column_index`. The sort button stays inside.
3. **Body cells:** `Role::Cell` with `aria_row_index` / `aria_column_index`, plus a way to name them. Either an optional delegate method such as `fn cell_label(&self, row_ix, col_ix, cx) -> Option<SharedString>` (default `None`), or documenting that `render_td` should use `text!` so cell text becomes a `Label`.
4. **Focus:** expose the focused or selected row, e.g. `aria_active_descendant` on the container, or make rows focusable.

Items 1–3 are self-contained and could be a first PR. Item 4 probably interacts with #3183.
