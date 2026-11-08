# ADR-0105 — Plugin operation completion and snapshot ownership

## Status

Implemented. Windows validation and remaining gates are recorded in
`docs/benchmark/plugin-runtime-followup-2026-11-07.md` and
`docs/benchmark/plugin-runtime-v254-delivery.md`.

## Decisions

- A catalog-install reply follows download, digest validation, package replacement,
  required startup and durable session writes. Download admission retains the reply
  until completion. Failed downloads and shutdown resolve the same pending reply.
- Runtime outputs use shared immutable snapshots from the v2 decoder through the
  registry and actor projection. An unchanged reply shares the existing snapshot.
  V1 frames retain their delivered wire representation. The legacy manager tick
  API still returns owned outputs for existing callers.
- HUD input is cached per plugin. Configuration and menu changes retain the HUD
  allocation. Failed, disabled and removed plugins lose their cached HUD entry.
- Disabled and removed actors retire after their admitted operations complete.
  They stop independently and are joined after completion. Their channels remain
  private, preventing late results from entering a replacement actor. A stop flag
  is checked again immediately before parking because blocking pipe operations
  can consume a thread's unpark token.
- Optional `event_retry.v2` adds `EventRejected { id }`, meaning Busy for that event.
  Event/Ack/EventRejected use session-local contiguous u64 transport IDs, separate
  from business bubble IDs. The host admits only the next ID, defers gaps, and
  suppresses all earlier admitted IDs for the entire session in constant memory.
  The SDK retains the event until acknowledgement and retries the same ID at most
  once per 34 ms. Transport admission precedes acknowledgement and deduplication.
  Each event queue admits at most 256 events and 1 MiB of serialized event payload.
  Peers without this extension retain their existing saturation behavior.
- Control admission stays at 64 operations and 1 MiB. The pipe writer additionally
  reserves bounded acknowledgement capacity for 256 events and 256 checkpoints.
  A publication burst can therefore finish before the peer consumes its replies.
  Expired writes fail before sending another request. Stop reply and process exit
  share one three-second deadline.
  Requests are boxed inside the writer message so reserving acknowledgement
  capacity does not reserve an inline maximum-size HostMessage in every slot.
- Input-source overflow totals are converted to deltas. The v2 runner invokes
  `reset_input` before delivering a gap to plugin code. Implementations clear
  temporary pressed state in that callback and preserve their business data.
- Windows compares evaluated render content before GPU upload and presentation.
  Runtime animation evaluation keeps the configured cadence. Changed model
  generation, content, HUD, surface dimensions, DPI, show, paint and display/DWM
  notifications invalidate the cached presentation. This uses the evaluated frame
  as the source of truth and requires no approximation of physics settling.
- Windows replacement windows reuse the current D3D11 device and immediate
  context when the device is healthy. Back buffers, composition targets and
  candidate model resources remain private to each renderer, preserving switch
  rollback. Each draw restores its complete state after either renderer clears
  the shared context during teardown. A removed device is recreated. This bounds
  the process-lifetime shader-initialization handles observed on the acceptance
  machine without changing shader algorithms, texture formats or image quality.
- The requested topmost band is established on hidden window creation and
  restored when showing a demoted window. Native window flags are verified by
  the full Windows overlay suite, including hidden-window creation.
- Companion UI events, window creation and synchronous cursor/monitor queries run
  on the window worker. Protocol handlers never wait for those native operations
  while holding the business-state lock. Cyber Work keeps its existing hover
  behavior with a one-shot 200 ms wake while the counter can be visible. The three
  companion shutdown watchers use a condition variable instead of 50 ms polling.
- Current official development packages are 0.2.1 with a minimum host of 2.5.4.
  Frozen 0.2.0 artifacts remain inputs to the incremental baseline, not a
  compatibility target. Published v1 packages retain their binary tests; the
  unreleased v2 draft does not acquire a compatibility adapter.

## Diagnostics and acceptance

Actor diagnostics contain wire request/byte/timeout counts, queue and accounted
payload peaks, coalescing/overflow counts and bounded latency samples. Contents of
requests, input sequences and plugin business files are excluded.

The host fixture is a Cargo test binary and is excluded from product packaging.
SDK and official-package acceptance tests are explicitly ignored in an ordinary
workspace run. `just plugin-tests` prepares their required artifacts and executes
them before the workspace suite. Missing artifacts fail that entry point.

The accepted 2.5.4 isolation cost remains the historical comparison baseline.
This follow-up uses a frozen source snapshot of the existing 2.5.4 workspace for
incremental resource comparisons. Earlier short Windows verification remains
archived separately from the final installer checks and long-duration validation.
The development-only profiling runner permits an isolated eight-hour run and
keeps companions alive across model/panel exercises unless restart testing is
explicitly selected. For the 2.5.4 delivery the user shortened sustained testing
to four hours, subsequently accepted the observed 70-minute run and requested
two comparison rounds, and accepted isolated validation on the current Windows
machine. The report must preserve those distinctions from the original
eight-hour, three-round and clean-machine gates. macOS acceptance remains a
separate gate.
The user subsequently approved delivering the installer with CPU acceptance
still open. This is a delivery-scope decision, not a passed CPU or full-plan
performance gate; the installer receipt and final report must say so explicitly.
