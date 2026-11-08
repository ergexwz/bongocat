# ADR-0103: Isolate plugin IO from the service owner

## Status

Implemented. On 2026-11-07 the user chose to retain the current isolation design
after reviewing the Release precheck's approximately 1.9 MiB extra host private
commit with ten plugins and improved request latency. This accepts the measured
tradeoff for 2.5.4; the original 10% memory comparison was exceeded, not passed.
ADR-0104 adds protocol v2 and event/deadline-driven service scheduling. The
measurements and short-validation limits are in the plugin runtime benchmark.

## Decision

Each plugin has a persistent worker owning its process, configuration transaction,
and lifecycle. The service retains a projection and never waits for that plugin's
start, request, or stop. Workers publish projections before reliable command
completions. A projection is applied again after draining completions, so a UI
acknowledgement cannot expose the state from before its operation.

The mailbox admits at most 64 queued operations and 1 MiB of control payload.
Package bytes remain subject to the existing archive limit and installation gate.
Ticks, geometry, activity totals, and pending previews may be replaced by newer
values within a segment; actions, configuration, and lifecycle operations form
ordering barriers. Input edges are ordered; rejected batches accumulate an
overflow count delivered to that plugin, rather than silently disappearing.
Shutdown is an independent signal. Admission closes before workers stop, and
every worker is asked to stop before the service joins any of them.

The process reader's wake target follows ownership when a running process moves
to a worker. A crashed worker fails pending replies and is removed before retry;
its private channels cannot deliver results into its replacement.

Each actor exposes metadata-only diagnostic counters for completed/failed
commands, rejected/coalesced work and input overflow, plus the most recent 1024
request durations. Queue pressure cannot cause an unbounded latency history.

HUD and companion caches are invalidated by their affected output regions.
Unchanged views do not rebuild menus or copy every plugin HUD on each service
iteration. ADR-0104 replaces the original 50 ms integration cadence with event
and deadline wakeups. V1 plugins retain their existing 200 ms ticks.

## Evidence and limits

- A real two-process fixture makes serial shutdown fail and parallel shutdown pass.
- A nonresponsive process reaches its 2 s timeout while another actor completes
  a request in under 100 ms; the overloaded actor rejects further queued work.
- The current Todo, Mail, and Cyber Work packages enable, stop, resume and disable
  through the real service in an isolated store.
- Configuration-only changes do not invalidate the HUD or companion menu.
- The retry fixture uses a missing executable, avoiding an interactive Windows
  launch of malformed executable bytes.

The requested shortened 25-minute validation and resource comparisons are
recorded in the benchmark; they do not replace UI/game acceptance,
administrator-broker acceptance, or establish long-term memory behavior. Protocol v2, capability
negotiation, event/checkpoint acknowledgements, requested wakeups, SDK examples
and companion migration are described in ADR-0104. The existing full-workspace
localization coverage failure is reported separately.
