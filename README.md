# RustUI lab

The TaskManager+ Lite UI, ported to several Rust UI libraries side by side. It
serves two purposes: a test app for Windows-focused contributions to those
libraries, and a fair comparison between them.

| Path | What's there |
|---|---|
| `crates/tmp-model` | The shared model: data types, the poll thread, Processes grouping and sorting, formatting, derived values, series colors, settings, and a deterministic synthetic data source. No UI dependencies. |
| `frontends/<library>/` | One port per library. Each folder is its own Cargo workspace with its own `target/`. |
| `repros/<library>-<issue>/` | Minimal standalone reproductions to link from upstream issues and PRs |
| `bench/` | `measure.ps1` and `report.ps1`, plus the recorded results |

```bash
cargo test -p tmp-model
```

```bash
cargo run -p tmp-model --example dump -- 30
```

## Writing a port

1. Create `frontends/<library>/Cargo.toml` with an empty `[workspace]` table and `tmp-model = { path = "../../crates/tmp-model" }`.
2. Create a `SharedModel`, then call `spawn_poller(Synthetic::default(), model.clone(), settings.refresh_ms, wake)`. In `wake`, ask your toolkit to redraw. Keep the returned `Poller` alive for the life of the app.
3. Redraw only when woken. Don't run a continuous frame loop; `bench/measure.ps1` will show it as idle CPU.
4. Build the pages from `groups`, `fmt`, `derived` and `palette`, so that only the UI code differs between ports.
5. Testing a fix in a library fork: add `[patch.crates-io]` to the port's `Cargo.toml`, pointing at your local checkout.
