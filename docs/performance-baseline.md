# Dock performance baseline

Baseline commit: `6b2eea9`

The audited 15-item, 64 px Leopard dock produced the following initial measurements:

| Metric | Baseline |
| --- | ---: |
| RSS | 318,120 KiB |
| PSS | 178,345 KiB |
| Private anonymous memory | 138,492 KiB |
| Passive OSDockX CPU | ~13.4% of one logical core |
| Hover-sweep OSDockX CPU | ~16.9% (21–22% peaks) |
| Reflection temporary surfaces/frame | 15 |
| Reflection masks/frame | 90 |

These numbers were captured before optimization. They are the comparison point for the staged changes; machine-level latency percentiles must be rerun on the audited X11 session because the repository test environment does not reproduce its compositor, windows, or CPU governor.

The instrumented baseline Cairo benchmark in the repository test environment reported:

| Workload | Scale | Mean | p50 | p95 | p99 |
| --- | ---: | ---: | ---: | ---: | ---: |
| Rest, reflections | 1× | 14.965 ms | 11.581 ms | 47.576 ms | 52.698 ms |
| Center hover, reflections | 1× | 12.382 ms | 12.314 ms | 13.177 ms | 15.056 ms |
| Sweep, reflections | 1× | 15.600 ms | 12.352 ms | 49.712 ms | 51.948 ms |
| Rest, no reflections | 1× | 4.083 ms | 4.013 ms | 4.593 ms | 4.892 ms |
| Center hover, no reflections | 1× | 4.280 ms | 4.220 ms | 4.836 ms | 5.116 ms |
| Rest, reflections | 2× | 22.096 ms | 18.601 ms | 67.681 ms | 78.790 ms |
| Center hover, reflections | 2× | 24.300 ms | 20.727 ms | 43.031 ms | 90.818 ms |
| Sweep, reflections | 2× | 24.268 ms | 20.439 ms | 51.157 ms | 92.022 ms |
| Rest, no reflections | 2× | 13.868 ms | 13.792 ms | 14.666 ms | 17.593 ms |
| Center hover, no reflections | 2× | 17.556 ms | 14.269 ms | 56.894 ms | 58.856 ms |

These timings are a code-path baseline, not a substitute for the audited-machine gates.

## Reproduction

- Build and run the dock with `cargo run --release` and a 15-item, 64 px configuration.
- Warm the process for two minutes.
- Run `cargo run --release --bin osdockx-perf -- <PID> 60` for a deterministic core-X11 pointer sweep. The helper records dock/Xorg CPU and `/proc/<pid>/smaps_rollup` memory values under `/tmp`.
- Run `cargo bench --bench cairo_renderer` for deterministic Cairo rest, center-hover, sweep, reflection-off, 1×, and 2× renderer timings.
- Enable `osdockx::perf=debug` to receive one aggregate counter summary per ten seconds. Per-frame logs are not emitted at that level.

Keep compositor, display resolution, scale, CPU governor, pinned applications, and open windows fixed. Use a fresh process, warm for two minutes, repeat each workload five times, and report medians plus p95/p99 draw times.

## Deterministic visual fixtures

`renderer::tests::deterministic_rest_and_hover_renders_match_golden_hashes` renders 15 synthetic ARGB icons at 1× and 2×. Its checked-in pixel digests cover rest and center-hover frames; the font-dependent hover-label rectangle is excluded. The four reference digests, in that order, are:

```text
16098008435556604288
2165020988576021856
9880531414050164059
10299436036222587131
```
