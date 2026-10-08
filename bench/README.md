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

## Results so far

| Build | Startup (ms) | Memory, private WS (MB) | Private bytes (MB) | Idle CPU (% of one core) | Processes | Exe (MB) |
|---|---:|---:|---:|---:|---:|---:|
| egui lite 2.7.2 (installed, live data) | 33.4 | 108.4 | 173.8 | 15.7 | 1 | 7.94 |

At a 1 s tick, 15.7 % of one core is about 157 ms of CPU per poll. This build can't show how much of that is the live telemetry collection and how much is the UI.
