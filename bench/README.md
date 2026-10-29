# bench

Measures every build of the same app the same way.

```powershell
./bench/measure.ps1 -Exe <path to exe> -Label "<toolkit> (<data source>)"
./bench/report.ps1          # Markdown table of the latest run per label
```

`measure.ps1` launches the exe 3 times. Each time it waits for a visible window, lets the app idle for 5 s, samples for 30 s, then closes the app. It appends the median of the runs to `results.jsonl`. Commit that file: it's the data behind the comparison table.

## What the numbers mean

| Column | How it's measured |
|---|---|
| Startup | From process creation (kernel start time) to a visible main window. This is **not** first paint: a toolkit that shows its window before drawing looks faster than it is. egui's 33 ms is likely that case. Every run after the first is a warm start. |
| Memory, private WS | Private working set, which is Task Manager's "Memory" column. Read from perf counters at the end of the idle window. |
| Private bytes | Committed private memory, including pages that aren't resident |
| Idle CPU | CPU time over the 30 s window, as a share of one core and of the whole machine (24 logical processors) |
| Processes | Size of the process tree: the app plus everything it spawned. WebView2 builds show 6–7 here, and all of them count. |

All values are summed over the whole process tree.

## Comparing fairly

- **Use the same data source.** The installed lite build polls live telemetry through the native DLL, so its idle CPU includes the telemetry collection. The lab ports start on `Synthetic` data. Compare ports with ports, and compare against lite only once a port runs on live data, or after building an egui port on `tmp-model`.
- **Use the same refresh rate.** Every build here uses the 1 s default.
- **Use the same power state.** The script warns when the laptop is on battery and records `on_ac`. Plug in before measuring.
- **Close heavy apps first.** Background load changes CPU clocks and timer behavior.
- **Close screen readers and inspectors** (NVDA, Narrator, Accessibility Insights) before measuring. While one is connected, accesskit builds do extra work every tick.

## Results so far

Same window size and 1 s refresh throughout. The lab builds are TaskManager+ Lite's own egui UI ported onto `tmp-model` (`frontends/egui`), running on synthetic data.

| Build | Startup (ms) | Memory, private WS (MB) | Private bytes (MB) | Idle CPU (% of one core) | Exe (MB) |
|---|---:|---:|---:|---:|---:|
| egui lite 2.7.2 (installed, live data) | 33.4 | 108.4 | 173.8 | 15.7 | 7.94 |
| egui 0.29 lab (synthetic) | 29.3 | 77.1 | 137.6 | 0.67 | 4.94 |
| egui 0.36 lab (synthetic, no accesskit) | 32.4 | 74.5 | 133.1 | 0.77 | 7.28 |
| egui 0.36 lab (synthetic, accesskit) | 22.0 | 74.4 | 133.7 | 0.62 | 7.69 |
| gpui-kit 0.8.1 lab (synthetic) | 203 | 34.2 | 78.1 | 2.26 | 24.61 |

What this shows:

- **Lite's idle CPU is the telemetry, not egui.** The same UI on synthetic data uses 0.67 % of a core instead of 15.7 %, so about 96 % of the installed lite's idle cost (roughly 150 ms per 1 s tick) is collecting live data through the native DLL. About 31 MB of its memory is the backend too.
- **The 0.29 → 0.36 upgrade is free at idle.** CPU and memory are within run-to-run noise. The exe grows by 2.3 MB.
- **accesskit costs nothing until something listens.** With no screen reader or inspector running, the accesskit build idles like the others. An earlier run of the same build measured 2.53 % of a core, most likely because NVDA or Accessibility Insights was still open, and AccessKit only builds and updates its tree while a UI Automation client is connected. Close those tools before measuring, or measure that case on purpose as its own row.
- Startup differences of ±10 ms are noise. Every egui build shows its window before the first paint.
- **gpui-kit uses less than half the memory of egui** (34 MB vs 74 MB private working set), with a 3× larger exe. The first of its three runs measured 66.5 MB, probably a cold start, so the median of the other two decides.
- **gpui-kit idles at about 1.8 % of a core regardless of updates, because GPUI wakes on every vsync.** At a 5 s refresh it still measured 1.89 %, against 2.26 % at 1 s (diagnostic run, not recorded here), so the once-a-second redraw isn't the cost. Per-thread sampling shows the main thread running at the display rate. On Windows, GPUI's `VSyncProvider` thread invalidates every window after each vblank, and each wake runs a frame check even when nothing changed. GPUI does the same on macOS while a window is visible, so this baseline is by design.
- **A minimized gpui-kit window doesn't get cheaper.** Unlike macOS, which stops its display link for hidden windows, GPUI on Windows keeps invalidating minimized ones: the lab measured 3.91 % visible and 3.36 % minimized (egui: 1.98 % and 0.42 %). Patching GPUI to skip hidden and minimized windows brought minimized down to 0.70 % without changing visible CPU, and drawing resumes on restore. Diagnostic runs with `threads.ps1` (CPU per named thread, `-Minimize` to minimize first; 3 runs of 20 s each), not recorded in `results.jsonl`. The patch and an upstream issue draft are in `repros/gpui-idle-vsync/`.
- **gpui-kit's 203 ms startup is real.** Unlike egui, its window appears with its first frame drawn.

## Accessibility

```powershell
./bench/a11y.ps1 -Exe <path to exe> -Label "<same label as measure.ps1>"
```

Walks the UI Automation tree under the app's window, the same tree Narrator, NVDA and Accessibility Insights read. It records how many elements there are, how many have a name a screen reader can speak, and how many can be reached by keyboard. The summary goes to `a11y.jsonl` and the full tree to `a11y-trees/`. This is a scripted proxy: still run an Accessibility Insights FastPass and listen with NVDA before calling a port accessible.

| Build | Elements | Named | Keyboard-focusable |
|---|---:|---:|---:|
| egui lite 2.7.2 (installed, live data) | 0 | 0 | 0 |
| egui 0.29 lab (synthetic) | 0 | 0 | 0 |
| egui 0.36 lab (synthetic, no accesskit) | 0 | 0 | 0 |
| egui 0.36 lab (synthetic, accesskit) | 136 | 98 | 26 |
| gpui-kit 0.8.1 lab (synthetic) | 153 | 89 | 2 |

With accesskit, screen readers can finally see the window's contents, but the structure is weak (see `a11y-trees/egui-0.36-lab-synthetic-accesskit.txt`):

- The process table is a flat list of `Text` elements. It has no table, row or column structure, so a screen reader can't say which numbers belong to which process. (An earlier version of `a11y.ps1` printed the window's top-level elements in reverse, which made the table look bottom-up. Fixed on 2026-10-22; the counts were unaffected.)
- The painter-drawn expander arrows are focusable `Custom` elements with no name.
- 18 elements have no control type at all.
- The filter box is a real `Edit`, but it ignores UI Automation's `ValuePattern.SetValue`, so assistive tools can't type into it that way.

The first two are fixable in the app. The rest are candidates for upstream egui and egui_extras issues.

gpui-kit 0.8.1 exposes the process table as a real data grid (see `a11y-trees/gpui-kit-0.8.1-lab-synthetic.txt`):
- named column headers with their sort buttons,
- rows of cells named through `TableDelegate::cell_label`,
- expand and collapse through UIA.

That structure arrived upstream in longbridge/gpui-kit#3667. The unnamed elements are blank cells, which are blank on screen too. The weak spots:
- Almost nothing is keyboard-focusable (2 elements).
- On the other pages, gpui-kit's `Label` and plain text aren't exposed at all. See `frontends/gpui-kit/README.md`.

The installed lite exposes nothing below its window, because eframe is built without accesskit. To a screen reader, the window is a titled empty box.

Manual checks on the same build (2026-10-08):

| Check | Result |
|---|---|
| Accessibility Insights FastPass, automated checks | "No issues found". This is a false pass: with no elements below the window, there is nothing for the checks to fail. |
| FastPass tab stops | 1: the window itself |
| Keyboard only | Nothing works. You can't switch pages, move through rows, reach the filter box, or sort. |
| NVDA | Announces "TaskManagerPlus Lite" for everything. Silent when hovering over rows and buttons. |

A clean FastPass only counts when the scripted element count is nonzero, so always report the two together.

Accessibility Insights exports (`*.a11ytest`) go in `bench/a11y-manual/`. That folder is gitignored, because each export embeds a screenshot of whatever data was on screen.
