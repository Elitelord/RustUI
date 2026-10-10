<!-- Draft for https://github.com/emilk/egui/issues/new?template=feature_request.md
     Title: egui_extras `Table`: expose table, row and cell roles to AccessKit -->

**Is your feature request related to a problem? Please describe.**

`egui_extras::Table` has no accessibility semantics. Screen readers see the header and cells as a flat list of text, with no table, rows, columns or header association, so they can't announce "Memory, 120 MB, row 2 of 3". With `TableBody::rows` (virtualization), only the visible rows exist in the tree, and nothing tells assistive tech how many rows there are in total.

This is what Windows UI Automation shows for a 3×3 table with a header (egui 0.36.2, `accesskit` on; also still the case on `main`, where `egui_extras` has no AccessKit code):

```
Window "egui_extras table repro"
  Text "Name"
  Text "CPU"
  Text "Memory"
  Text "alpha.exe"
  Text "1.0%"
  Text "50 MB"
  Text "beta.exe"
  ...
```

Repro: <link to examples/table.rs and examples/table_virtual.rs>

**Describe the solution you'd like**

Have `Table` emit AccessKit roles, as egui widgets now do with `accesskit::Role`:

- The table node: `Role::Table`, with `row_count` and `column_count` set to the *total* counts, so virtualized tables report their real size.
- Each row: `Role::Row`, with `row_index`.
- Header cells: `Role::ColumnHeader`. Body cells: `Role::Cell`, with `column_index`.
- Selected rows (`TableRow::set_selected`): mark the row as selected.

On Windows these map to UI Automation's Table and Grid patterns, which screen readers already use to read native list views.

**Describe alternatives you've considered**

Apps can name each row themselves (for example by labelling a row from its cells), but that doesn't give screen readers table navigation (next column, header announcements), and every app would have to repeat it.

**Additional context**

Found while porting a Windows task manager UI (a sortable process table, about 300 rows, virtualized) to egui 0.36. I'd like to work on this. Would a PR along these lines be welcome, and should virtualized tables report the total row count?
