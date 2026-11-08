# Plugin performance and compatibility follow-up

## Scope and baseline

This work continues the existing uncommitted 2.5.4 implementation on Windows
x64. It preserves the local host/SDK/official-plugin workspace layout, released
v1 packages, plugin IDs, business data and enabled intent. No product default
quality or frame-rate setting changes. The user selected short Windows testing;
the eight-hour gate remains open.

Before editing, the tracked and nonignored untracked source tree was archived at
`target/optimization-followup/baseline-source.zip` with its input inventory in
`baseline-inputs.json`. Baseline and current Release service harnesses are frozen
as separate executables, with SHA-256 receipts in `baseline-executable.json` and
`current-executable.json`. This measures the incremental follow-up, not the
earlier synchronous-to-actor rewrite. The historical approximately 1.9 MiB
isolation overhead remains an explicitly accepted cost in
[the preceding report](plugin-runtime-v2-2026-11-06.md); it does not waive the
10% regression check for this follow-up.

## Implementation and plan adjustments

- Replaced silent retired-Pomodoro test exits with a mandatory host fixture.
  Kept real lifecycle, persistence, localization and HUD assertions. SDK examples
  and all three official companions are required by `just plugin-tests`.
- Catalog install replies now await download, validation, install/start and
  durable state. Disabled actors retire asynchronously. A stop check immediately
  before parking prevents a lost shutdown wake after a blocking exchange.
- Shared immutable output snapshots span the v2 decoder, registry and actors.
  Per-plugin HUD inputs retain allocations across unrelated changes. Wire
  statistics count actual IO without serializing a second time.
- Negotiated optional `event_retry.v2` retains event IDs and payloads across Busy
  responses. Event and byte limits, bounded acknowledgement capacity, remaining
  execution budgets and input-gap reset are exercised independently.
- Windows skips unchanged upload/presentation while continuing configured
  animation evaluation. Model/surface/DPI/show/paint/display changes invalidate
  the presentation. Frame timing is opt-in and bounded.
- Companion UI events, window creation and cursor/monitor queries run outside
  the protocol business lock. Shutdown watchers use condition variables. Cyber
  Work keeps a 200 ms one-shot wake for its existing hover counter behavior.
  Stop watchers call the thread-safe application exit entry directly.
- Official development packages use 0.2.1 and require host 2.5.4. This avoids
  selecting v2 packages on old hosts and makes the tested upgrade concrete.
- SDK examples include public package manifests and a standard-library-only
  Python packager. The HUD companion demonstrates context-triggered notifications
  as well as HUD output. Host process contracts read those same public manifests.

See [ADR-0105](../adr/0105-plugin-runtime-completion-and-caching.md) and the
[SDK author guide](../../../../bongocat_plugins/crates/bongocat-plugin-sdk/README.md).
Capability declarations describe the protocol contract; they provide no OS
permission isolation.

## Plan assessment

| Stage | Current state | Acceptance boundary |
| --- | --- | --- |
| Baseline and test entry | Implemented, including mandatory fixtures and frozen source/binaries | Short local measurements only |
| Communication isolation | Persistent per-plugin IO, bounded mailboxes and independent lifecycle owners | Blocked writer and faulty-peer tests passed |
| Idle work and copies | Event wakeups, shared snapshots, regional HUD/menu invalidation | Targeted cache and scheduling contracts passed |
| v2 protocol | Typed negotiation, publications, events, checkpoints and one-shot scheduling | Frozen v1/v2 and SDK contracts passed |
| SDK and official plugins | Three examples and three v2 companions; Windows render/window changes | Windows short verification; no macOS acceptance |
| Final acceptance | Partially verified | Existing full-suite failures and long/manual gates below remain open |

## Functional evidence

The isolated refused-read fixture completes its handshake but never consumes
Start. A 64 KiB restored state blocks the real pipe writer; the host terminates
that process within the existing five-second startup budget, records a timeout,
and a healthy peer continues. A separate Release test drives nine healthy
clients concurrently through 100 actions each while a tenth plugin is faulty;
the per-plugin P95 gate is 100 ms.

The SDK process test pauses event consumption, publishes 600 events, resumes and
checks delivery once per ID with no transport timeout. Frozen delivered v1 ZIPs
are verified against their recorded hashes before execution. A frozen v2 0.2.0
bundle from the existing local installer is also tested without recompilation.

| Boundary | Evidence | Result |
| --- | --- | --- |
| v1 wire and binaries | Saved JSON request fixture; three hash-checked frozen v1 ZIPs | Passed |
| Frozen v2 binaries | Existing installer-extracted 0.2.0 packages, no rebuild | Passed |
| SDK contracts | Event-only, three one-shot timer wakes, HUD/context/notification, 600-event backpressure | 4 passed |
| Official upgrade | v1 to 0.2.1, business files and enabled choices; async service resume | 3 passed |
| Ordinary Windows host | Three real companions, 30 seconds, clean shutdown; direct broker rejection | Passed, token not elevated |
| Elevated Windows host | Same scenario, asserted elevated host; broker verifies actual child medium token | Passed, actual host elevated |
| Settings window | Close/reopen, three required companion cards, isolated Development root | Exit 0 |
| Plugin workspace | Tests and all-target/all-feature Clippy | 51 tests passed; Clippy passed |
| Development tools | Catalog/root/package/summarizer contracts | 14 tests passed |
| Host static checks | Format; all-target Clippy; app injection and production feature checks; Release check | Passed |
| Host process faults | Refused pipe reads, out-of-order replies, continuous unsolicited pushes, persistence/start failures, v1 wire shape | 7 passed |
| Manager/service units | Writer framing and failure, operation completion, recovery, queue ownership | 37 + 8 passed |
| Service integration | Real fixture lifecycle, configuration, locale, data and HUD behavior | 14 passed; isolated performance test separately required |

Final broker receipts are in `target/optimization-followup/broker-acceptance-final`, including
actual host elevation, test exit codes and broker/package digests. Both paths use
the existing authenticated broker; the tests change no user enable settings.

The first graphical prechecks are excluded from performance comparisons. They
overlapped builds, and the initial window-closure helper also selected Tauri's
internal event target. The checked-in app sampler now selects only the
`Tauri Window` class. It retains the process handle so Windows PowerShell can
read the real exit code after sampling. The corrected 75-second functional run
closed six actual panels, changed the model twice and exited with code 0.

Validation records and raw outputs are retained under
`target/optimization-followup`. The final result tables below are completed only
from finished, uncontended runs.

## Resource method

Baseline/current Release runs are interleaved, using the same machine, harness
scenario and build mode. Each 0-plugin, 10-fixture and 3-official-plugin case warms
for 15 seconds and samples for 45 seconds, repeated three times. Official baseline
packages are frozen 0.2.0 artifacts; current packages are the development 0.2.1
artifacts. Synthetic plugins use the same fixture executable for both hosts.
No builds or other acceptance scenarios run during these measurements.
The machine is Windows 11 build 26200, Ryzen AI 9 H 465, 20 logical processors,
and 33,447,333,888 bytes of physical memory. GPU adapters are Radeon 880M and
GeForce RTX 5060 Laptop GPU. Each final run has 25–28 resource snapshots.

The first complete intermediate matrix is retained under
`matrix-before-compact`. It was not accepted: official-companion host private
commit increased from 2.320 to 2.559 MiB (10.27%), and ten-fixture host private
working set increased 35.66%. The writer's enlarged acknowledgement ring stored
the large Request variant inline in every slot. Requests now use a boxed payload,
retaining the same admission capacity, budgets and wire representation while
reducing reserved slot storage. The final matrix is rebuilt and repeated after
this adjustment. The app pilot interrupted to make this fix is explicitly marked
incomplete and does not count as the 25-minute run.

### Final service matrix

All 18 final runs completed. Values below are medians of three runs; memory is
MiB and latency is milliseconds. Both columns use v2. The baseline is the frozen
before-edit 2.5.4 workspace, not the original synchronous v1 host.

| Scenario | Version | Request P95 | Host private commit | Host private working set | Native plugin private commit | Host threads |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 0 plugins | Baseline | 0.037 | 1.832 | 0.957 | 0 | 3 |
| 0 plugins | Current | 0.002 | 1.832 | 0.949 | 0 | 3 |
| 10 fixtures | Baseline | 3.464 | 4.383 | 2.527 | 6.641 | 33 |
| 10 fixtures | Current | 3.460 | 4.371 | 2.594 | 6.648 | 33 |
| 3 official | Baseline | 1.302 | 2.297 | 1.625 | 8.227 | 12 |
| 3 official | Current | 1.375 | 2.289 | 1.621 | 8.184 | 12 |

Host private commit changes are 0.00%, -0.27% and -0.34%. The largest host private
working-set increase is 2.63% in the ten-fixture case, compared with the rejected
intermediate increase of 35.66%. Host working-set changes are -0.05%, +0.81% and
+0.16%. No measured process-memory metric exceeds the 10% regression gate in
this final service matrix.

Machine-normalized sampled CPU is below this sampler's observed resolution for
the synthetic cases and current official case. The baseline official native
plugin group averages 0.00897%; the current group is below resolution. Zero is
not a claim of zero CPU or zero wakeups. There is no renderer in this harness,
and companion windows remain closed: these figures establish neither whole-app
GPU savings nor network/mailbox behavior. Console helpers are accounted for
separately in the raw summary.

The separately required nine-healthy/one-faulty Release test completes 900 healthy
actions with per-plugin P95 from 32.040 to 41.060 ms, below the 100 ms target.
`fault-nine-plus-one-final.json` retains each plugin's P95 and maximum. Final
matrix CSV/JSON and `matrix-summary.json` are under
`target/optimization-followup/matrix`; executable, package and input hashes are
recorded in `target/optimization-followup/measurement-inputs.json`.

### App resource exercise

The app exercise runs separately using `tools/measure-app-performance.ps1`.
Every 30 seconds it hides the pet, selects the next built-in model, restores the
pet, restarts one companion and opens the three panels. The external helper
closes actual panels after a delay. All commands use a separate injected
Development root. The frame collector retains at most 4,096 successful draw
durations; its quantiles describe those retained draws, not scheduling/input
latency or every frame throughout the full run. Resource CSVs cover the run.

The completed run lasted 1,502.63 seconds, with 891 post-warmup resource
snapshots. It switched built-in models 46 times and closed 120 actual companion
panels. The application exited with code 0, and PID/creation-time checks found no
remaining test processes. The largest resource-sample gap was 7.00 seconds;
reported peaks are observed sampled peaks, not continuous maxima.

| Process group | Median private commit MiB | Observed private-commit peak MiB | Mean CPU, % of machine | Observed dedicated GPU peak MiB |
| --- | ---: | ---: | ---: | ---: |
| Host application | 93.23 | 126.65 | 0.3726 | 47.96 |
| Three native companions | 19.39 | 41.82 | 0.0503 | unavailable |
| WebView2 descendants | 0 | 855.42 | 0.1623 | 387.23 |

The WebView2 median is zero because panels are closed during more than half the
samples; it does not describe an open panel's footprint. GPU figures sum the
available per-PID counters. GPU allocation is separate from process commit and
must not be added to it as host RAM. Host median dedicated GPU allocation was
29.51 MiB. Comparing the first/last 10% of samples, host private commit rose
1.84 MiB and native-companion commit fell 0.86 MiB. Companions restart in rotation,
so this does not establish uninterrupted long-lived plugin heap stability.

The overlay presented 37,594 frames and skipped 4,135 unchanged presentations.
All 46 model commits succeeded. The retained 4,096 draw samples have P50/P95/P99
of 0.224/0.337/0.500 ms, with no retained sample exceeding its draw budget;
33,451 further timing samples were dropped by the diagnostic cap. These are CPU
draw-submission timings, not GPU execution or end-to-end frame/input latency.
No physical input edges were captured during this resource exercise.

One resource trend remains open: host handle count increased by 84 between the
first/last 10% sample windows. Type-only snapshots showed stable process/thread
handle counts, while Section and Mutex references grew with model switching.
Before exit, one named Section and one named Mutex each had 46 duplicate handles.
Their creating component has not been identified; this must not be labeled a
proved harmless cache or a passed long-duration resource-reclamation gate.
`handle-trend.jsonl` retains redacted type/name counts. The separate eight-hour
gate remains open even though the short run exited cleanly and left no processes.

Raw app CSV, frame report and completion receipt are in
`target/performance-app/app-short-soak-final`; the derived group summary is
`target/optimization-followup/app-summary.json`. Measured binaries and package
inputs are preserved with verified hashes in
`target/optimization-followup/final-measured-artifacts/receipts.json`.

### Whole-application idle comparison

The unmodified before-edit app was also built in Release from the frozen source
snapshot. Its executable SHA-256 is
`0d4eea9acda2f3f4f290e3e6dfabda9d3e0c942e3173ae49b5bb61302d3cb5f5`.
Baseline/current app runs alternate three times, each with 15 seconds warmup and
45 seconds sampling, a fresh injected Development root and the same default
model/configuration. Three official companions remain enabled with their panels
closed. The baseline uses frozen 0.2.0 ZIPs; current uses the exact archived 0.2.1
ZIPs from the completed service/app measurements. Frame tracing is disabled for
both. The old app has no frame-report export, so this is a resource-only
comparison, not a before/after draw-latency comparison.

| Process group | Version | Median private commit MiB | Median private working set MiB | Mean CPU, % of machine | Dedicated GPU MiB |
| --- | --- | ---: | ---: | ---: | ---: |
| Host | Baseline | 91.082 | 25.977 | 0.36851 | 29.465 |
| Host | Current | 90.900 | 25.879 | 0.31575 | 29.465 |
| Native companions | Baseline | 8.016 | 5.766 | 0.00896 | unavailable |
| Native companions | Current | 8.098 | 5.855 | 0.01065 | unavailable |
| Process group total | Baseline | 99.217 | 31.873 | 0.38053 | 29.465 |
| Process group total | Current | 99.000 | 31.740 | 0.32293 | 29.465 |

The observed aggregate CPU change is -15.14%; host CPU changes -14.32%. Host and
aggregate private commit change -0.20% and -0.22%. Host working set changes
+0.59%, aggregate working set +0.41%, and GPU dedicated/shared allocation is
unchanged. Host/native thread medians remain 143/38. WebView2 stays absent.

The native-only relative CPU delta is +18.81%, an absolute +0.00169 percentage
points. Its baseline run range is 0.00710–0.01202% and current range
0.00718–0.01071%; these overlap. Reconstructing CPU-time deltas from the CSV shows
15.625 ms accounting steps, equivalent to about 0.00174 percentage points over
45 seconds on 20 processors. The difference is within one observed accounting
step and the run-to-run spread. This short sample cannot establish a native-only
CPU improvement or regression. The 200 ms hover wake also preserves existing
visible counter behavior; it must not be removed to manufacture an idle result.

All six app comparisons exited successfully, with 28–29 resource samples each.
Raw cases are in `target/performance-app/app-compare-*`, input hashes in
`app-comparison-inputs.json`, and the derived comparison in
`app-comparison-summary.json` under `target/optimization-followup`.

The sampler follows only the test process and its descendants, checking PID and
creation time. CPU is normalized by logical processor count. Host, native
plugins, WebView2 and console helpers remain separate. Private commit, working
set and GPU allocation are separate metrics; unavailable GPU counters remain
unknown. Repetition results use medians. These short samples do not replace the
original 30-second warmup/120-second measurement or eight-hour soak gates.

## Known pre-existing test failures and remaining gates

The i18n and topmost failures and the model-switch handle growth described in
this historical section are resolved by the subsequent
[v2.5.4 delivery validation](plugin-runtime-v254-delivery.md). That report tracks
the final installer, ordinary/elevated payload checks and remaining long-run
measurements; the earlier measurements below are retained as their original record.

`just check` and the workspace portion of `just plugin-tests` encounter the
existing i18n `catalog_keys_are_referenced_by_source` failure. The frozen
before-edit source reproduces it. The full Windows overlay suite also reproduces
two topmost assertions on the before-edit snapshot; the affected single-window
test passes in isolation. These failures are not counted as passed checks.
The final overlay rerun has 111 passing tests and the same two failing topmost
assertions. The workspace excluding those known suites, targeted plugin tests, plugin
workspace tests, all-target checks and Clippy provide the narrower evidence.

The next resource investigation is to identify and bound the duplicated named
Section/Mutex references during model switching. Their creating component is
still unknown. Full draw-latency A/B and sustained physical-input latency also
remain unverified. SDK example packages and SHA-256 receipts are retained at
`target/optimization-followup/sdk-example-packages` and
`sdk-example-packages.json`; the four process contracts use the public manifests.
Native workspace tests use a workspace-local temporary directory: two sandbox
file-operation failures disappeared when using that supported temporary root,
and all 51 native/SDK tests then passed.

Windows clean-machine install/upgrade/uninstall, actual games, macOS, the
original-duration performance matrix, sustained physical-input latency and
eight-hour stability remain separate acceptance work. No installer was published
or installed over the user's copy.
The existing paired Unreleased changelog entry already describes this delivery
unit, so this continuation adds no duplicate summary. No manual deletion of user
state is required.
