# Plugin runtime v2 validation

## Scope

Windows x64 plugin-service changes on top of
`cbe8364e31ce4ece5872335af871e49f406a3fe7`. The baseline is archived from that
commit together with its SDK. Both builds use the same resource-test source,
Rust toolchain, process sampler and isolated temporary-store layout. No user
configuration, business data, installer, signing key or published package is
replaced.

Implementation and protocol decisions are in [ADR-0103](../adr/0103-plugin-worker-isolation.md)
and [ADR-0104](../adr/0104-plugin-protocol-v2.md). SDK examples and their contract
are documented in the [author guide](../../../../bongocat_plugins/crates/bongocat-plugin-sdk/README.md).

## Environment and method

- Windows 11 build 26200, AMD Ryzen AI 9 H 465, 20 logical processors,
  33,447,333,888 bytes of physical memory reported by Windows.
- Debug service harness; optimized native plugin executables. These are service
  measurements, not whole-application release-renderer measurements.
- Synthetic 0, 3 and 10 plugins use the same executable with protocol 1 or 2.
  The three-companion comparison uses frozen bundled v1 ZIPs and newly built
  Todo/Mail/Cyber Work v2 development ZIPs.
- Each case warms for 30 seconds, measures 100 sequential no-op operations, then
  samples idle resources for 120 seconds. Baseline/current runs are interleaved,
  with three repetitions per case. No build runs during resource sampling.
- The initial matrix was interrupted. Some subsequent samples ran alongside a
  separate soak before both were stopped at the user's request; those process
  counters exclude the other test, but scheduling can still be affected. The
  incomplete matrix must not be presented as a full three-run comparison.
- Only the test host and its descendants are sampled. PID plus creation time
  guards against attributing a reused PID to an unrelated process. The external
  sampler is excluded. Console helpers, native plugins and WebView2 are separate
  groups.
- CPU is the mean of sampled per-process CPU percentages, normalized by 20
  logical processors. Memory, handles and threads are snapshot medians; repeated
  runs are combined by their medians. Private commit, private working set,
  total working set and GPU allocation are separate counters.
- The first CPU delta in each measured phase is excluded: it crosses the phase
  boundary and may include the preceding warmup or latency-request burst.
- GPU counters unavailable for a group remain unknown. The harness does not run
  the application's renderer. Companion panels stay closed, so this does not
  characterize an open WebView2 window, account connections or a real mailbox.

Raw CSV/JSON, executable hashes and environment metadata are retained under
`target/performance-results/2026-11-06-final`. Summaries are generated with
`tools/summarize-plugin-performance.py`.

The completed zero-plugin comparison (three runs each) has a median request
P95 of 63,865 µs on the baseline and 12 µs on the current service. These
zero-plugin requests read the plugin center snapshot; the other scenarios send
plugin actions. Median host private commit was 1.820 MiB and 1.832 MiB,
respectively. Mean process-tree CPU was 0.00797% and 0.00000% of the machine,
respectively; the latter is below this sampler's observed resolution. Each run
contained 69–73 resource samples. This result does not establish the effect when
plugins are enabled. The interrupted three-plugin run is retained separately.
Only two baseline and one current three-plugin runs completed. Their host
private-commit medians were 2.549 and 2.949 MiB; the current run had six more
threads, corresponding to three persistent plugin actors and three writers.
This was an incomplete preliminary sample. A separate short matrix was then
run as described below.

## Short 3/10-plugin matrix

To stay within the requested short validation window, each case below used
15 seconds of warmup, 100 sequential operations and 45 seconds of sampling;
baseline and current runs were interleaved and repeated three times. Each run
contained 21–25 resource snapshots. The longest sample gap was 8.0 seconds in
one current three-fixture run; every other run's largest gap was at most 2.1
seconds. These short measurements do not replace the originally planned
120-second samples. CPU percentages are normalized to 20 logical processors;
zero means below the observed sampling resolution.

| Scenario | Version | Request P95 | Host private commit | Plugin private commit | Host CPU | Host threads |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 3 fixtures | v1 baseline | 65.631 ms | 2.543 MiB | 1.875 MiB | 0.06228% | 6 |
| 3 fixtures | v2 current | 2.477 ms | 2.949 MiB | 1.883 MiB | below resolution | 12 |
| 10 fixtures | v1 baseline | 66.463 ms | 3.000 MiB | 6.277 MiB | 0.10535% | 13 |
| 10 fixtures | v2 current | 6.425 ms | 5.082 MiB | 6.266 MiB | below resolution | 33 |
| 3 official plugins | v1 baseline | 65.734 ms | 1.984 MiB | 8.223 MiB | 0.06425% | 6 |
| 3 official plugins | v2 current | 2.712 ms | 2.516 MiB | 8.203 MiB | below resolution | 12 |

Private commit is not resident RAM. For example, host private working set for
the three official plugins was 1.473 MiB baseline and 1.828 MiB current;
native plugin private working set was 5.977 MiB and 5.969 MiB. Console helpers
and any WebView2 processes remain separate in the raw summary. The host commit
increase is approximately 27% with three real plugins and 69% with ten fixtures;
it is a real tradeoff above 10%, not a passing memory-regression gate. The extra
host threads correspond to the per-plugin actor and writer, but these counters
do not prove how much of the committed memory each allocation accounts for.
Normal plugin compatibility and the improved latency were verified. The
diagnostic allocation and heap observations below do not account for this
entire steady-state difference. These figures exceed the original memory gate;
the subsequent user decision below accepts that tradeoff for 2.5.4.

Raw data are under `target/performance-results/2026-11-06-short-matrix`, with
summaries in `target/performance-short-matrix-summary.json`. The baseline is the
archived host and SDK at the commit above. The three-plugin fixture executable
is shared across both versions; the official plugins use frozen v1 packages
versus the locally built v2 packages, so those measurements include plugin
implementation changes as well as host changes.

A small standalone Windows probe compared 20 parked Rust threads at the default
stack reservation against 256 KiB and 128 KiB reservations (three processes
each). Median process private commit was 1,794,048, 1,724,416 and 1,703,936
bytes respectively. This idle-thread-only probe does not prove a smaller stack
is safe for real plugin operations; the observed saving of roughly 68–88 KiB
for 20 threads also does not account for the approximately 2.08 MiB ten-plugin
host difference. Therefore no production stack limit was reduced. Probe source,
binary and individual results are kept under `target/stack-size-probe*`.

## Optimized-build follow-up (2026-11-07)

A Release-profile precheck used the same archived baseline, harness and scaling
fixture, with ten plugins, ten seconds of warmup and thirty seconds of samples.
Each side completed one run and clean shutdown. Baseline/current host private
commit medians were 2,662,400/4,648,960 bytes, request P95 was 65.076/4.866 ms,
and thread medians were 13/36. This is a preliminary single-run result, but
confirms that the service memory increase also exists in optimized builds.
It does not pass the 10% service-memory comparison gate. Metadata and raw data
are identified by `target/performance-release-precheck.json`.

Separate read-only `VirtualQueryEx` metadata observations of ten-plugin Release
test processes found 1,671,168/3,526,656 bytes of committed private regions.
Regions containing guard pages accounted for 643,072/1,556,480 bytes across
16/36 allocations; the other private regions accounted for 1,028,096/1,970,176
bytes. Guarded regions are consistent with thread stacks, but this does not
identify every stack or attribute all heap commitments. These separate short
observations are not a repeated resource comparison. Their metadata is kept in
`target/performance-results/regions-baseline-*` and `regions-current-*`.
On 2026-11-07 the user explicitly chose to retain the current isolation design
after reviewing the roughly 1.9 MiB Release overhead and latency improvement.
The measured tradeoff is accepted for 2.5.4. The original 10% service-memory gate
was exceeded and waived by that decision, not passed; short tests still cannot
establish a long-term memory bound.

The workspace version and its 23 local lockfile entries have been prepared for
2.5.4 without dependency upgrades. Host Clippy feature groups and the workspace
Release check pass. The SDK/companion workspace passes 49 unit tests and Clippy;
33 focused version, changelog and packaging-tool tests pass. The current host
passes HUD/manager/service unit tests, real parallel-shutdown and v2-example
contracts, and the three real companions' asynchronous stop/resume lifecycle.
Fresh 0.2.0 packages were additionally checked for first-install disabled state,
v1 data-preserving upgrades and service-level catalog/local update intent.
Legacy Pomodoro-dependent tests still lack that retired fixture and their early
returns are not counted as real-process evidence. The bundle inputs are pinned
in `target/v254-bundle.json`. The accepted isolation design is the input to the
requested 2.5.4 Windows installer. The local installer preparation
does not publish a release or change signing configuration. The existing paired
Unreleased plugin-reliability entry was rewritten rather than duplicated.

## Automated behavior evidence

- Real plugin processes prove concurrent shutdown and mailbox backpressure.
  A healthy peer completes 100 requests while another process is blocked for
  its 2-second deadline. P95 was 199 microseconds, below the 100 ms gate; the
  metadata-only latency report is `target/fault-isolation-latency.json`.
- SDK contracts cover required/optional negotiation, disabled optional frames,
  monotonically increasing region revisions, event deduplication/backpressure,
  checkpoint acknowledgements and unchanged timer deadlines.
- Real counter/timer examples prove checkpoint persistence, failed-update
  rollback, independent event delivery, zero idle timer wakes for the counter
  and exactly three requested wakes for the timer. The HUD example verifies
  subscribed pet-visibility updates through a real plugin process.
- Current companion packages enable, stop, resume and disable through the real
  asynchronous service. The frozen three v1 packages also install disabled,
  start, tick and stop under the updated host.
- Windows settings close/reopen smoke passes with all three development
  companion cards and an isolated storage root. Shutdown prints the existing
  tray-cleanup warning while returning success.
- A 70-second exercise precheck completed one disable/enable cycle and retained
  all three plugins in a healthy state. The requested 25-minute replacement uses
  five minutes idle followed by twenty minutes with ten-second restart intervals.

The 25-minute run completed **102** restart cycles, confirmed all three plugins
enabled and healthy, and exited successfully. It recorded 639 process-tree
snapshots. The sampler had one 172.4-second gap spanning the end of the idle
period and the beginning of active work, so it is not continuous evidence for
those intervals. Raw data and the phase summary are under
`target/performance-results/2026-11-06-short-soak` and
`target/performance-short-soak-summary.json`.

During active work the host's private commit median grew from approximately
2.54 MiB in the first two-minute bucket to 3.22 MiB in the final bucket. Growth
slowed in later buckets but did not clearly stop; allocator warm-up versus
unbounded growth has not been established. Host handles remained at a median of
171 and host threads at a median of 12 at the beginning and end. Native plugin
thread counts differed by a median of nine between the first and last portions;
the processes were repeatedly replaced, so this is not a per-process leak
measurement. Whole-tree mean CPU was approximately 0.019% during available
idle samples and 0.040% during active samples, normalized to all 20 logical
processors. Store metadata changed from 24 files/26,127,374 bytes to 27 files/
26,127,718 bytes. WebView2 did not run with the panels closed; GPU data was
unavailable, so neither is counted as host memory.

Three additional isolated 180-second controls used the same current service
and three companion packages, with 30 seconds of warmup. Starting/ending values
below are medians of the first/last 10% of resource snapshots; no heap contents
or user data were collected. Each control also performed 100 initial latency
requests before the measured interval; the action counts below cover only the
measured interval.

| Active workload | Actions | Restarts | Host private-commit change | Host handle/thread change |
| --- | ---: | ---: | ---: | ---: |
| Actions only | 108 | 0 | 0 KiB | 0 / 0 |
| Restarts only | 0 | 17 | +384 KiB | 0 / 0 |
| Actions and restarts | 108 | 17 | +368 KiB | 0 / 0 |

These controls localize the observed growth to the restart path rather than
ordinary no-op requests. They do not distinguish retained allocator pages from
unreleased live allocations, or prove that memory eventually plateaus. All three
test processes completed their checks and shut down; their separate data are in
`target/performance-results/2026-11-06-diagnostics`.

An additional six-minute diagnostic used a separate test executable with a
counting wrapper around Rust's `System` allocator. It measured allocation sizes
and counts only, not contents. After 35 restarts, the median live Rust allocation
size for the first five observations was 145,195 bytes; the last five had a
median of 143,530 bytes, while the host's private commit rose by 282,624 bytes
between its first and last sample windows. After graceful service shutdown the
test process reported 15,959 live Rust-allocated bytes, including test/runtime
objects that were still in scope. There were 161 resource snapshots with a
maximum gap of 9.3 seconds. This rules out a corresponding rise in *tracked live
Rust requested bytes* in this short scenario; it does not count native/Windows
allocations, allocator metadata, or pages retained after a free.

Two additional `VirtualQueryEx` metadata snapshots of that same isolated test
process, at roughly restart 14 and 22, found private committed regions totaling
1,802,240 and 1,855,488 bytes. An existing 1 MiB reserved allocation accounted
for 49,152 bytes of the increase, another existing allocation for 8,192 bytes,
and a different allocation decreased by 4,096 bytes. These are region metadata,
not memory contents or proof of a particular heap or allocator. The generated
probe sources, raw allocation counts and region metadata are retained under
`target/plugin-allocation-probe` and the directory named by its `latest.json`.

A second six-minute test additionally enumerated the test process's four
Windows heaps with [`HeapWalk`](https://learn.microsoft.com/en-us/windows/win32/api/heapapi/nf-heapapi-heapwalk),
using heap locks and counting occupied block sizes only. All enumerations
completed without error. Across 35 restarts, the first/last five-observation
medians were 145,914/143,195 bytes for live Rust requests and 360,495/367,115
bytes for occupied Windows heap blocks. Process private commit increased by
602,112 bytes between its first and last sample windows; host handles were
unchanged. The 166 resource snapshots had a maximum gap of 2.4 seconds. After
service shutdown, still-in-scope test/runtime objects accounted for 15,965 live
Rust-requested bytes and 197,109 occupied Windows heap bytes. This supports
retained or otherwise untracked committed pages as a possibility, but does not
prove eventual reclamation or identify all native allocation sources. The probe
temporarily locks heaps and is diagnostic only, never part of the shipped host.
Its output is identified by `target/plugin-allocation-probe/native-latest.json`.

A separate five-minute diagnostic performed 29 restarts and called the Windows
[`HeapSetInformation`](https://learn.microsoft.com/en-us/windows/win32/api/heapapi/nf-heapapi-heapsetinformation)
low-fragmentation-heap resource optimization once after restart 15. The call
succeeded in 66 microseconds, and the isolated process's private commit fell
from 3,022,848 to 2,666,496 bytes immediately. Samples 1–7 seconds later had a
median of 2,691,072 bytes, but the first sample after the next restart was
3,088,384 bytes: the saving was short-lived. Median restart durations before
and after the call were 73.8 and 75.9 ms, respectively; one run cannot establish
a latency effect. This supports releasable heap cache as one contributor to the
observed committed pages, not a complete explanation or proof of a long-term
bound. No whole-process heap optimization is added to the product; its effect on
the real UI was not measured. The diagnostic output is identified by
`target/plugin-allocation-probe/trim-latest.json`.

To replace the incomplete idle observation, a fresh isolated five-minute idle
run completed successfully with 154 sample groups over 291.2 observed seconds;
its largest interval between samples was 6.4 seconds. Median host private commit
was 2,637,824 bytes and host private working set 1,921,024 bytes. The three native
plugins together had median private commit 8,683,520 bytes and private working
set 6,332,416 bytes. Host handles/threads were 171/12; the plugin totals were
548/38. Mean CPU was 0.00028% for the host and 0.02369% for the native plugins,
normalized to the machine's 20 logical processors. The extra console helper is
reported independently in `target/performance-idle-summary.json`. The idle
run also shut down successfully. It uses a new isolated store, so it is a
separate observation rather than a continuation of the pressure run.

## Quality gates and existing failures

Host all-target/all-feature Clippy (including both application feature groups),
plugin-workspace Clippy/tests and the host release check pass. The host workspace
excluding the overlay crate passes when the existing unused-localization-key
coverage test is skipped. This does not claim a clean full-workspace test run.

The unused-key failure reproduces on the unchanged baseline. In the current
overlay suite, topmost-window style assertions fail intermittently; the baseline
whole overlay suite reproduces the demoted-window assertion, while isolated
baseline/current runs of that assertion pass. The archived overlay suite also
lacks an external model fixture; that missing-fixture failure is not attributed
to the implementation. These results remain separate from plugin verification.

Python tool discovery reports 110/113 passing tests. The other three concern
symbolic-link privilege, a pre-existing generated `target/explorer-probe`
workspace and Windows path separators in the platform-detector test. The
changed development/distribution tool contracts pass all 10 focused tests.

No macOS smoke, elevated-host acceptance, clean-machine installer lifecycle,
game compatibility or full-renderer/GPU soak is claimed. No public release is published.
The checked-in bundled catalog still refers to the frozen v1 packages. Official
v2 packages were built and tested in the Development catalog; a future production
package must deliberately select and verify the new payload before it can ship
those plugins. The existing formal installers and bundled fixtures are unchanged.

An unsigned local production **candidate**, built from an isolated source copy
with `--bundle-local-plugins`, was subsequently generated without replacing an
existing installer. Archive integrity testing passed for 189 entries. The
extracted catalog digest matched the candidate pin compiled into the extracted
host; all three extracted version 0.2.0/protocol-2 ZIPs matched their catalog
digests and candidate inputs. The extracted host and broker matched the locally
built host binary. On the extracted payload, real-process tests passed for
first-install disabled state, start/stop, v1-to-v2 update with preserved Todo,
Cyber Work and disabled Mail data/enable intent, retention of old package
directories, and rejection of direct broker invocation. The extracted host's
`--help` also exited successfully from an unrelated working directory. Existing
installations still retain their selected plugin versions until the user chooses
an update. This is not a clean-machine install, upgrade or uninstall test, and
the candidate still identifies as 2.5.3; it is not a signed/public release.
The candidate, extraction, logs and payload hashes are under the isolated
directory identified by `target/v2-isolated-installer.json`.
After that candidate was built, a service-level test exposed that updating a
disabled plugin from a local file enabled it; the service now preserves the
enable choice for local updates as it already did for catalog updates. A real
v1-to-v2 service test verifies enabled Todo through the catalog, disabled Mail
through a local package, preserved data and old packages, restart, and automatic
enable on a fresh explicit local install. A new candidate was then built from an
isolated source snapshot matching the fixed service. Archive integrity, catalog
pin, three package digests, the extracted host and broker hashes, `--help`, and
all four real-package lifecycle and broker tests were checked again against the
new extraction. Its SHA-256 is
`11292c401325f39d091aa9ebd1aa26820da0428ac6145e8b3a3a9ca5a7eedd77`.
The service test links the current service code and uses the extracted packages;
it does not drive the extracted GUI. The candidate remains unsigned and does not
replace the separate clean-machine, elevated-host or game checks.
CHANGELOG is skipped: the existing unreleased plugin-reliability summary covers
this continuation, so no second entry is added. No local user state needs to be
deleted for this implementation.

## 2.5.4 local Windows installer

After the user accepted the isolation tradeoff, an isolated Production/Release
build generated `target/package/BongoCat_2.5.4_x64.exe` (239,598,806 bytes).
SHA-256: `7f821ed7ce16ce2ff7e125ba85c12fe90a24950789092b10146b32bb682c1f27`.
The local environment has no `just` executable; its exact `build` recipe was
invoked directly as `cargo run --locked -p bongocat-packaging --
--bundle-local-plugins --formats nsis`. The existing packaging crate and NSIS
template remain the sole packaging implementation. No dependency upgrade,
signing configuration, Git commit, tag, push or public release was made.

Archive integrity passed for all 189 entries. Extracted installer/host product
versions are both 2.5.4. The extracted host and broker match the Release binary;
the compiled catalog pin and all three 0.2.0/protocol-2 ZIP digests match their
selected inputs. All 170 model files, the bundled VC DLLs, the offline WebView2
installer, the provenance lockfile digest, and 736 source/manifest/lock files
were compared with the selected source snapshot. The extracted host's `--help`
exited with code 0 from an unrelated working directory.

Using the extracted plugin packages, all four real-process acceptance tests
passed: disabled-by-default installation and start/stop, v1-to-v2 upgrades
preserving business data/enable choices/old packages, direct broker invocation
rejection, and asynchronous-service catalog/local upgrade plus resume and fresh
local installation. The service test links the current 2.5.4 service; it does
not drive the extracted GUI. The final delivery copy's hash was checked again.
The adjacent `.exe.sha256` and `.validation.json` files contain the checksum,
payload receipt, accepted tradeoff and explicit untested boundaries. Build,
extraction and test logs are under the directory identified by
`target/v254-isolated-installer.json`.

This installer is unsigned (`NotSigned`). Clean-machine installation/upgrade/
uninstall, a new 2.5.4 elevated-host run, macOS, games and long-term memory bounds
remain separate unverified checks. The short validation authorized by the user
is not presented as an eight-hour soak. Existing formal installers and live
production data were preserved. No manual local-state deletion is required.
The paired Unreleased plugin-reliability entries were rewritten, not duplicated.

## Reproduction

From the host source directory with Cargo, Python, Node and frontend dependencies
available:

```powershell
python tools/dev_plugins.py -- cargo test --locked -p bongocat-plugin-manager -p bongocat-plugin-service -p bongocat-plugin-hud
python tools/prepare_performance_baseline.py > target/performance-baseline.json
python tools/prepare_performance_baseline.py --current > target/performance-current.json
$current = Get-Content target/performance-current.json -Raw | ConvertFrom-Json
./tools/measure-plugin-performance.ps1 -TestExecutable $current.executable -OutputDirectory target/performance-results/repeated -PluginCount 3 -Protocol 2 -PackageDirectory target/plugin-dev/plugins
./tools/measure-plugin-performance.ps1 -TestExecutable $current.executable -OutputDirectory target/performance-results/soak -PluginCount 3 -Protocol 2 -PackageDirectory target/plugin-dev/plugins -WarmupSeconds 0 -SampleSeconds 1500 -IdleSeconds 300 -RestartSeconds 10 -Iterations 1 -Exercise
python tools/summarize-plugin-performance.py target/performance-results/repeated --output target/performance-summary.json
```

`-Exercise` sends no-op operations every five seconds and restarts one companion
at the configured interval. It checks all enabled states after each cycle and before graceful
shutdown. Aggregate store file counts/bytes are recorded before/after; CSVs are
flushed during the run. GPU and open-panel lifetimes require their own application
tests and are not implied by this service-only command. For the three 180-second
controls, use `-ExerciseMode actions`, `-ExerciseMode restarts`, and
`-ExerciseMode mixed`, with `-WarmupSeconds 30 -SampleSeconds 180
-RestartSeconds 10 -Iterations 1 -Exercise`.
