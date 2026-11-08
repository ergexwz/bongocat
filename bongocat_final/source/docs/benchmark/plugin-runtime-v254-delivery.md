# v2.5.4 Windows delivery validation

## Scope and acceptance

Windows x64 local installer, using the existing host, SDK and companion workspaces.
No commit, push, public release, signing-configuration change or installation over
the user's copy is included. Configuration, business files and enable choices
remain in place. The user confirmed delivery through isolated validation on the
current machine; clean-machine installation/upgrade/uninstallation is excluded
from this delivery's completed acceptance. macOS and game compatibility are not
claimed.

On 2026-11-08 the user first shortened sustained testing to four hours, then
accepted the observed run and requested proceeding early. Actual duration is
70 minutes 26 seconds. The user also requested two comparison rounds rather
than three. This delivery does not claim the four-hour or original eight-hour
duration gates passed.

**Delivery decision:** the user explicitly requested delivery with **CPU
acceptance still open**. Functional/compatibility/package checks are complete;
this report does not mark the full performance plan or idle CPU gate accepted.

The incremental baseline and the previously accepted isolation-memory tradeoff
remain documented in `plugin-runtime-v2-2026-11-06.md` and
`plugin-runtime-followup-2026-11-07.md`. This validation does not relabel that
tradeoff as passing the original 10% memory gate.

| Plan phase | Current evidence |
| --- | --- |
| Baseline and test entry points | Mandatory Todo/Mail/Cyber Work, SDK examples and fault fixtures; archived binary inputs and counters. Original-duration comparisons queued below. |
| Independent communication | Persistent per-plugin actors/readers/writers, bounded admission, independent stop/deadline supervision, generation isolation and parallel shutdown; real-process fault tests pass. |
| Idle work and ownership | Event/deadline service wakeups, coalesced v1 ticks, shared output snapshots, per-plugin HUD revisions/cache and unchanged-frame suppression; unit/process contracts pass. |
| Protocol v2 | Negotiation, typed requests/replies, region snapshots, acknowledged events, subscriptions, one-shot wakes and atomic checkpoints; v1 binary compatibility retained. |
| SDK and official plugins | Three packaged SDK examples and official v2 companions 0.2.1; IDs/data/enable intent retained, asynchronous native UI work and rollback tests pass. |
| Final resource acceptance | User accepted the 70-minute observation and requested two rounds. Measurements are complete; idle CPU acceptance remains open and the user expressly approved delivery with that limitation. |

## Corrections completed for this delivery

- Removed seven unreferenced legacy plugin translations in all eight catalogs.
  The complete i18n suite now passes.
- Establish the requested topmost band on hidden-window creation and restore it
  on show. The complete Windows overlay suite passes, including both previously
  failing topmost assertions.
- Replacement windows reuse the healthy session D3D11 device and immediate
  context. New-device shader initialization reproduced two retained handles per
  replacement on this machine; device creation, composition initialization and
  shader compilation alone did not reproduce that growth. Window buffers,
  composition targets and candidate model resources stay independent. Device
  removal still selects a new device.
- Forty replacements, alternating accepted and discarded candidates, retain
  identical captured pixels after the other renderer clears the shared context.
  The final isolated graphics test records 505 handles after warmup and 507 after
  forty iterations, rather than linear growth.
- The resource sampler copies identity-checked process properties and disposes
  its observer handle. Children that exit during property reads are counted as
  unavailable samples. Seven deterministic regressions cover null/throwing start
  time, PID reuse, exit during sampling, descendant creation times and real
  process/window ownership guards.
- V2 transport event IDs are contiguous session-local u64 sequences. A constant
  size admitted frontier suppresses replays for the entire session, including
  after more than 1024 events; rejected admission cannot permit later events to
  overtake. SDK acknowledgement callbacks still receive the business bubble ID.
  This replaces the unreleased v2 draft, without changing published v1 frames.
- Windows development/test entry points also isolate WebView2 profiles. Earlier
  runs isolated business files but used the framework's default browser cache.
  The test-only `WEBVIEW2_USER_DATA_FOLDER` override resolves beside each
  temporary plugin executable; all three directories were observed inside the
  isolated store. Production behavior is unchanged. This uses the documented
  [WebView2 environment override and relative-path contract](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/webview2-idl?view=webview2-1.0.4258.31).

## Installer and payload

- Installer: `target/package/BongoCat_2.5.4_x64.exe`, **239,659,544 bytes**, local
  unsigned build.
- SHA-256: `7f1df24ed7aaac1649cfc1cddbc21a662cff768d8182c5b4ba3c6e04f5e28d53`.
- Host and broker SHA-256:
  `7e6a2142a9394946a5f455f41d4e9e3cd118afd86bcdcb2f6cd836748264bfab`.
- Bundled catalog SHA-256:
  `036226fdadee5f3cb5542949257f6dfd62ef2a5e622daf2005aa78eeabe7ed1e`.
- Three official packages are protocol 2, version 0.2.1, minimum host 2.5.4.
  Their exact ZIPs are inputs to the long run. Test fixtures and scaling/example
  executables are absent from the installer.
- `just build --bundle-local-plugins --formats nsis` succeeds. 7-Zip integrity
  and extraction succeed (189 entries). Host/broker match the build outputs;
  all 170 model files, three plugin archives, catalog, VC runtime files and
  offline WebView2 installer match their inputs.
- The extracted host reports product version 2.5.4 and exits successfully for
  `--help` from an unrelated working directory.

Receipts and logs are under `target/optimization-followup`: `delivery-artifact.json`,
`installer-sequence-local-build.log`, `installer-sequence-integrity.log`,
`installer-sequence-extraction.log` and `installer-sequence-verification.log`.
The two prior candidate installers and receipts are archived in
`previous-installer` and `pre-sequence-installer`; earlier formal installers
remain unchanged. The sandbox packaging attempt could not access the NSIS cache;
the ordinary Windows packaging entry point completed successfully.

## Functional verification

`just check` passes on the final Rust sources: format, all-target/feature Clippy,
complete workspace tests and Release check. The initial sandbox run could not
connect to the update tests' loopback server; the ordinary Windows rerun passes.

`just plugin-tests` passes SDK examples, current official plugins, the Release
nine-healthy/one-fault scenario and the workspace suite, using the isolated
browser-cache entry point. The current native/SDK workspace passes 55 tests and
all-target Clippy. SDK process tests include 600 events under queue backpressure;
21 SDK tests cover replay after 2048 events, admission rollback, gaps and sequence
exhaustion.

Both ordinary and actually elevated hosts pass against the extracted installer
payload: initial disabled state, real startup/run/stop, direct broker invocation
rejection, v1-to-v2 business-data/enable-intent preservation and service updates.
The elevated test asserts the real host token; broker startup enforces the
ordinary plugin token. The recorded admin flags are false/true, and both use the
exact packaged broker hash above. All **127 final engineering-tool tests** pass
in one run with the required symbolic-link privilege, including eight sampler
regressions, two CPU-summary regressions and the development-cache regression.
PowerShell hashing now uses the .NET SHA-256 API,
and the benchmark launcher clears inherited `PSModulePath` for Windows
PowerShell child processes.

Evidence: `delivery-broker-sequence/{ordinary,elevated}/status.json`, their
per-test stdout and stderr files, `sampler-sequence-tests-local.log`,
`graphics-handles-delivery.log`, `just-check-delivery-sequence-final.log`,
`just-plugin-tests-sequence.log`, `plugin-workspace-sequence.log` and
`plugin-clippy-sequence.log`.

## Compatibility matrix

| Input | 2.5.4 host result | Evidence boundary |
| --- | --- | --- |
| Published v1 Todo 0.1.0, Mail 0.1.1, Cyber Work 0.1.0 | Pass: original ZIP hashes pinned in the test; install/start/tick/stop without recompilation | Delivered v1 bytes and wire fixtures are retained |
| Official v2 companions 0.2.1 | Pass: initial disabled state, startup/shutdown, v1-data upgrade and service update | Both ordinary and actual administrator hosts; minimum host `>=2.5.4` |
| v2 event-counter, timer and companion SDK examples | Pass: process handshake, event delivery, checkpoint/rollback, requested timer wakes and pet context | Built from SDK and public package manifests, with no host special cases |
| Unknown required / optional v2 capability | Required refused / optional disabled | SDK negotiation and host-session tests |
| Unreleased v2 draft 0.2.0 | Outside compatibility contract | Frozen bytes remain paired with their frozen host for baseline measurements |

The v2 package minimum-host requirement prevents an older host from selecting
the package. This is a metadata/filter contract; the previous host executable
was not reinstalled on this machine. Capability declarations are protocol
negotiation, not operating-system permission isolation.

## Resource runs

The observed run is `target/performance-app/v254-four-hour`, using the frozen
host and exact installer plugin ZIPs in `delivery-profile-sequence`. It requested
14,400 seconds, but ended normally at **4,226.0605 seconds** on the user's request.
It switched models/hid/restored the pet and repeatedly opened and closed all
three companion panels. All three native plugin PIDs remained unchanged;
periodic restarts were disabled. CPU, private commit, private/total working set, GPU,
threads, handles and log bytes are sampled separately for host, native plugins
and WebView2. Timing samples are bounded to the first 4,096 timed draws; missed
draw deadlines continue to be counted afterward. The profiling executable uses
the Development environment and opt-in isolated-store/reporting features, not
the Production environment. Its SHA-256 is
`75ee981dd796216995430a45888e8b84b4d625d7cc8367a8584e2bbc6b74cdc1`;
installer-byte acceptance is recorded separately above.

The run completed **130 model switches**, with zero model-commit rejections,
**129 observed browser-process reclamation cycles**, and 2,402 post-warmup
resource snapshots (largest gap 7.281 seconds). Captured and consumed input edges
both total 224; queue overflow, callback panic and decode-error counts are zero.
The host and its owned children exited normally with no residual processes.
Host logs total 45,648 bytes, with zero warning/error lines. The bounded draw
sample has P50/P95/P99 0.216/0.319/0.486 ms and zero recorded draw-budget misses.
This is current-build diagnostic timing, not a paired full-product draw-latency
comparison; physical input latency was not measured.

Closed-panel medians, first 15 minutes versus last 15 minutes:

| Metric | First | Last |
| --- | ---: | ---: |
| Host private commit | 93.094 MiB | 95.584 MiB |
| Native plugins private commit, sum | 20.750 MiB | 28.359 MiB |
| Host handles / threads | 925 / 140 | 930 / 140 |
| Native plugins handles / threads, sum | 1,017 / 93 | 1,041 / 93 |
| Host dedicated GPU process memory | 29.855 MiB | 30.289 MiB |
| WebView2 processes after reclamation | 0 | 0 |

Native private commit rose in small steps by 7.609 MiB in aggregate; this short
observation does not establish a long-duration plateau. It is retained as a
measurement boundary rather than hidden in the total. During open-panel samples,
WebView2 private commit medians are 676.826/674.262 MiB and total dedicated GPU
process memory medians are 219.832/219.832 MiB. These browser/GPU values are not
host RAM. Details: `delivery-soak-summary.json`, `shortened-native-memory-bins.json`
and `shortened-soak-analysis.log`.

The original sampler deliberately rejects an early exit, so `four-hour-final.log`
contains that duration rejection. The separate user-authorized completion helper
records exit status, actual time, the frame report and child-identity cleanup in
`shortened-soak-completion.log` and `summary.json`. Its unavailable exact WM_CLOSE
counter is left null; the 129 browser reclamation cycles come from the CSV.

Earlier attempts are preliminary/incomplete: one was restarted after plugin PE
metadata changed during rebuilding; another stopped at about 18 minutes when a
WebView exited during a process-property read; the next was stopped normally to
finish browser-cache isolation. Later attempts exposed parent-PID reuse in the
sampler and an unexplained normal early exit; the last preliminary run was
stopped to apply the event-sequence correction. None counts as four-hour
evidence. The sampler
now rejects early exit and checks remaining child identities after shutdown.

`run-delivery-benchmarks.py` accepts the user-shortened run and performs an
isolated settings-window smoke, 12 interleaved service measurements (0, 10
fixtures, 3 official plugins; baseline/current; two repetitions) and four
whole-app idle measurements. Each comparison uses 30 seconds warmup and 120
seconds sampling. Benchmarks run after the soak and builds, not concurrently.
Raw results and input hashes will be retained in `delivery-matrix`,
`delivery-benchmark-inputs.json` and `delivery-app-*`.

The machine is Windows 11 Home x64 build 26200, Ryzen AI 9 H 465 (20 logical
processors), Radeon 880M and RTX 5060 Laptop GPU. Driver versions and exact input
hashes are retained in `delivery-machine.json` and `delivery-benchmark-inputs.json`.
The user-requested two rounds replace the planned three; their aggregate is the
median of two observations, accompanied by both raw observations.
CPU rates are integrated using actual sample intervals; CIM sampling is not
exactly periodic. The previous arithmetic mean over rates could overweight
shorter intervals. Both the warmup-boundary and irregular-interval summary
regressions pass. Previous summaries remain archived.

The original first whole-app baseline completed its sampling, normal-exit and descendant
cleanup checks, then failed at the summary's `Get-FileHash` lookup. Its raw CSV
was retained and only missing report metadata was reconstructed, explicitly
marked in `baseline-app-report-recovery.log` and the run's `summary.json`.
Elapsed time in that recovered summary is the last sampled time (a lower bound);
its unavailable sampler-skip count is null. No measurement values were replaced.

Both members of the first pair were then repeated under the fixed harness,
with the original pair preserved in
`delivery-first-pair-before-harness-alignment`. The repeated current run records
306 cursor samples and transient WebView2 work, so it is not a controlled idle
scene. The second current run records one initial cursor sample and no WebView2
CPU. Two recorded pairs are retained; they are **not** claimed as two accepted
idle CPU repetitions. All final app reports have normal exit and child cleanup.

## Final comparisons and CPU disposition

| Service scenario, two rounds | Baseline request P95 | Current request P95 | Baseline host private commit | Current host private commit |
| --- | ---: | ---: | ---: | ---: |
| No plugins | 0.024 ms | 0.0125 ms | 1.840 MiB | 1.836 MiB |
| Ten fixtures | 3.182 ms | 2.979 ms | 4.383 MiB | 4.375 MiB |
| Three official companions | 1.4405 ms | 1.372 ms | 2.313 MiB | 2.264 MiB |

The separate nine-healthy/one-fault Release test passes the per-healthy-plugin
P95 <=100 ms contract. Service idle CPU is below the sampled process-time
resolution in these trials; zero counters do not prove zero instructions.

The quiet second whole-app pair provides the following observations, rather
than a two-round CPU acceptance result:

| Metric | Baseline | Current |
| --- | ---: | ---: |
| Host CPU, percent of machine | 0.46438% | 0.47371% |
| Native-plugin CPU, sum | 0.00914% | 0.02159% |
| Process-group CPU | 0.47352% | 0.49530% |
| Process-group private commit | 99.023 MiB | 99.051 MiB |
| Dedicated GPU process memory | 29.465 MiB | 29.465 MiB |
| Host / native threads | 141 / 38 | 141 / 38 |

That pair's aggregate CPU change is +4.60%, and private commit +0.028%. The
earlier approximately +0.4% memory statement referred to the two-pair aggregate,
not this quiet second pair. Native CPU increases by 0.01245 percentage points;
no native CPU improvement is claimed. The mixed two-pair aggregate CPU is
0.41432%/0.52731%, including transient browser work in the current first pair,
and is unsuitable for passing the idle CPU gate. No measurement-error waiver is
inferred: the user explicitly chose delivery with CPU **not yet accepted**.

Final summaries: `delivery-matrix-summary.json`, `delivery-app-summary.json`,
`delivery-soak-summary.json`; input hashes and both observations are preserved.
The installer receipt is `target/package/BongoCat_2.5.4_x64.validation.json` and
records this CPU disposition, actual sustained duration and the excluded gates.

## Delivery bookkeeping

The existing paired Unreleased changelog summary already covers this delivery;
this continuation adds no duplicate entry. No manual deletion of user state is
required. The current machine's isolated acceptance scope is user-approved;
clean-machine, macOS and game results remain explicitly unclaimed.
