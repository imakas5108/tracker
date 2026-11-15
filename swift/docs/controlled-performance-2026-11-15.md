# Paired performance check — 15 November 2026

The previous apparent twofold slowdown was not reproduced under identical
workloads. This comparison does not establish the cause of the earlier result.

## Method

Apple M4, 16 GiB, macOS 27.2.1, Swift 6.4.2, release compilation in Swift 5
language mode. Compared audit baseline `4921173` with motion baseline `c827d7c`.
Each snapshot received the same harness-only private-socket support and
`--benchmark-background` activation guard. No numerical or tracking code was
backported. Each trial owned a fresh isolated app, empty sessions and private
endpoint. No builds ran during the paired timings.

`scripts/controlled-benchmark.py` ran five repetitions at 1, 2, 4 and 8 open
videos/jobs, alternating variant order. All used the same 72-frame, 1920 × 1080
synthetic clean ball-toss fixture, seed (130, 850), template radius 12, search
radius 64, score .9 and separation .015. The single active tracking slot is
unchanged: these are submitted-workload comparisons, not 1/2/4/8 simultaneous
decoders. The harness reports aggregate automatic frames per wall second,
including submission and polling overhead.

## Results

| Submitted videos | Audit median fps | Motion median fps | Change | Audit / motion sampled peak RSS |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 73.12 | 71.57 | -2.12% | 219.5 / 234.3 MiB |
| 2 | 78.02 | 76.40 | -2.07% | 226.4 / 233.9 MiB |
| 4 | 77.40 | 78.08 | +0.88% | 224.2 / 233.3 MiB |
| 8 | 78.98 | 78.46 | -0.66% | 230.0 / 239.4 MiB |

Per-trial status-call p95 medians ranged from 12.8 to 16.9 ms. This is a protocol
latency proxy, not measured rendered input feedback. One-video trials varied:
68.9–76.6 fps for audit and 60.7–76.4 fps for motion. Short runs and system
variation limit precision. Observations, provenance and exact timestamps agreed
across every trial, with coordinate/time comparisons at tolerance 1e-6 and exact
rational-stamp equality. There is no evidence here for adding a second backend
or widening admission to repair the historical comparison.

Raw observations and fixture hash: `build/controlled/results/raw.json`; compact
results: `build/controlled/results/summary.json`. Prepared snapshot bundles live
under `build/controlled/{4921173,c827d7c}/Tracker.app`. After recreating these
snapshots with identical harness support, reproduce with:

```sh
python3 scripts/controlled-benchmark.py \
  --variant audit="$PWD/build/controlled/4921173/Tracker.app" \
  --variant motion="$PWD/build/controlled/c827d7c/Tracker.app" \
  --counts 1,2,4,8 --repetitions 5
```

## Remaining performance work

Large-track overlay/lookup profiling, sustained playback/input latency, memory
pressure, thermal behavior, cold launch, several large real videos and broader
challenging labeled clips remain open. Synthetic 1k/10k/100k-point UI projects
were prepared but not yet profiled. No fixed product session cap was added and
the scheduler remains single-active. The [DJI stress test](dji-stress-2026-11-15.md)
prioritizes tracking robustness and clip-range support as concrete next gaps;
it does not substitute for these performance measurements.
