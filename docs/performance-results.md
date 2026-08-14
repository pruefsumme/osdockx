# Dock optimization results

Functional baseline: `6b2eea9`

Optimized implementation: `cd2e7ef` plus this report's benchmark-output update.

The deterministic Cairo benchmark uses 15 synthetic icons at 64 CSS px. Each row below is the median of five release-mode runs, with 240 timed frames per run. The regular sweep starts after 12 warm-up frames so its timings include first-traversal cache construction. The warm sweep preloads one complete traversal before timing the next traversal.

## Renderer comparison

| Workload | Scale | Baseline mean | Final mean | Reduction | Final p50 | Final p95 | Final p99 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Rest, reflections | 1x | 14.965 ms | 0.838 ms | 94.4% | 0.804 ms | 1.032 ms | 1.315 ms |
| Center hover, reflections | 1x | 12.382 ms | 1.042 ms | 91.6% | 1.006 ms | 1.239 ms | 1.617 ms |
| First sweep, reflections | 1x | 15.600 ms | 3.271 ms | 79.0% | 3.302 ms | 4.065 ms | 4.423 ms |
| Rest, no reflections | 1x | 4.083 ms | 0.782 ms | 80.8% | 0.752 ms | 0.954 ms | 1.177 ms |
| Center hover, no reflections | 1x | 4.280 ms | 0.983 ms | 77.0% | 0.951 ms | 1.213 ms | 1.375 ms |
| Rest, reflections | 2x | 22.096 ms | 2.175 ms | 90.2% | 2.076 ms | 2.835 ms | 3.625 ms |
| Center hover, reflections | 2x | 24.300 ms | 2.639 ms | 89.1% | 2.491 ms | 3.521 ms | 4.136 ms |
| First sweep, reflections | 2x | 24.268 ms | 8.608 ms | 64.5% | 8.776 ms | 10.481 ms | 11.944 ms |
| Rest, no reflections | 2x | 13.868 ms | 2.217 ms | 84.0% | 2.090 ms | 2.870 ms | 3.745 ms |
| Center hover, no reflections | 2x | 17.556 ms | 2.650 ms | 84.9% | 2.507 ms | 3.470 ms | 4.370 ms |

The comparable first-sweep result retains composite-construction cost. Once the full 1x traversal is warm, the five-run median improves to 1.045 ms mean, 0.994 ms p50, 1.276 ms p95, and 1.669 ms p99. Reflection and shelf hit rates are both 100%. At 2x, the combined raster/reflection cache reaches its required 32 MiB bound and evicts entries during the sweep: the median is 8.916 ms mean, 10.525 ms p95, and 11.604 ms p99, with a 72.39% reflection hit rate and 100% shelf hit rate.

The deterministic renderer stages showed the following representative 1x sweep p95 progression:

| Commit/stage | Sweep p95 | What changed |
| --- | ---: | --- |
| `6b2eea9` baseline | 49.712 ms | Immediate icon reflections and procedural shelf work every frame |
| `55ce64b` reflection cache | 7.569 ms | Reused icon rasters and completed six-pass reflection composites |
| `687733a` shelf cache | 3.968 ms | Reused static back/front shelf layers |
| Final, comparable first sweep | 4.065 ms | Median of five runs; later commits do not alter the direct renderer path |
| Final, fully warm sweep | 1.276 ms | All 1x sweep reflection composites resident |

## Runtime work removed

| Area | Final steady-state behavior |
| --- | --- |
| Hover scheduling | One shared GTK frame tick coalesces pointer and icon-animation updates; unchanged device-pixel signatures do not redraw. |
| Reflections | Warm fixed layouts create no temporary reflection surfaces. Raster, raw-window-icon, and reflection storage share a 32 MiB LRU bound. |
| Procedural shelf | The active back/front layers are reused; warm benchmark frames report a 100% shelf-cache hit rate. |
| Configuration and themes | GIO directory monitors trigger a 100 ms debounced reload. Parsing is event-driven, with a five-second recovery poll only after monitor failure. |
| X11 metadata | A 50 ms nonblocking event drain updates only the changed property. Full reconciliation uses the normalized 5–60 second recovery interval. |
| Draw state | Ordinary Cairo and GL draws borrow `DockModel` and `Theme`; custom-icon mappings are synchronized only when configuration changes. |

## Acceptance status

| Gate | Result |
| --- | --- |
| At most one requested dock draw per display frame | Covered by the coalescing implementation and regression test. |
| No redraw for unchanged device-pixel output | Covered by device-pixel signature tests. |
| 1x draw p95 below 8 ms and p99 below 16.7 ms | Pass for every deterministic workload. |
| 2x draw p99 below 16.7 ms | Pass; the fully warm sweep median p99 is 11.604 ms. |
| 2x draw p95 below 8 ms | Not met for a continuous sweep; median p95 is 10.525 ms. Rest and fixed-hover workloads pass. |
| Warm reflection hit rate above 95% | Pass at 1x (100%). At 2x the 32 MiB bound limits the sweep to 72.39%; fixed rest/hover layouts remain at 100%. |
| No steady-state config/theme parsing | Satisfied by the monitor path; recovery parsing runs only while monitors are unavailable. |
| Near-zero steady-state X11 property requests | Satisfied between property events and periodic reconciliation by construction and cache-field mapping tests. |
| Visual fixtures | The four deterministic rest/hover golden hashes remain exact. Cached grouping differs from the immediate six-pass path by at most four channel values in the strict byte comparison; this exceeds the one-channel stretch target but is not visibly distinguishable in the fixtures. |
| Cache memory | Pass; unit tests enforce the combined 32 MiB limit under eviction. |

CPU, Xorg CPU, RSS, PSS, private anonymous memory, ten-minute soak growth, compositor fallback screenshots, and live create/focus/minimize/restore/close timings were not measured in this repository-only run. Those gates require the same audited X11 session, compositor, windows, display scaling, and CPU governor as the supplied baseline. Running the dock here would alter the active desktop and user configuration, so these values are deliberately left unclaimed.

## Reproduction

Run:

```text
cargo bench --bench cairo_renderer
cargo test renderer_paints_non_empty_surface --lib
cargo test renderer::tests::leopard --lib
cargo test --lib
```

For audited desktop CPU and memory measurements, warm a release build for two minutes and run `cargo run --release --bin osdockx-perf -- <PID> 60`. The helper writes samples under `/tmp` and reports OSDockX and Xorg CPU separately.
