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

What this shows:

- **Lite's idle CPU is the telemetry, not egui.** The same UI on synthetic data uses 0.67 % of a core instead of 15.7 %, so about 96 % of the installed lite's idle cost (roughly 150 ms per 1 s tick) is collecting live data through the native DLL. About 31 MB of its memory is the backend too.
- **The 0.29 → 0.36 upgrade is free at idle.** CPU and memory are within run-to-run noise. The exe grows by 2.3 MB.
- **accesskit costs nothing until something listens.** With no screen reader or inspector running, the accesskit build idles like the others. An earlier run of the same build measured 2.53 % of a core, most likely because NVDA or Accessibility Insights was still open, and AccessKit only builds and updates its tree while a UI Automation client is connected. Close those tools before measuring, or measure that case on purpose as its own row.
- Startup differences of ±10 ms are noise. Every egui build shows its window before the first paint.

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

With accesskit, screen readers can finally see the window's contents, but the structure is weak (see `a11y-trees/egui-0.36-lab-synthetic-accesskit.txt`):

- The process table is a flat list of `Text` elements, listed bottom-up. It has no table, row or column structure, so a screen reader can't say which numbers belong to which process.
- The painter-drawn expander arrows are focusable `Custom` elements with no name.
- 18 elements have no control type at all.
- The filter box is a real `Edit`, but it ignores UI Automation's `ValuePattern.SetValue`, so assistive tools can't type into it that way.

The first two are fixable in the app. The rest are candidates for upstream egui and egui_extras issues.

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
