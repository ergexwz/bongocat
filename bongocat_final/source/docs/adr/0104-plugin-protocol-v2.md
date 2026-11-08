# ADR-0104: Negotiated plugin protocol v2 and requested wakeups

## Status

Implemented; performance and long-soak evidence is recorded separately. This
supersedes the fixed-cadence and protocol-v2 limitations listed in ADR-0103.

## Wire contract

The host accepts package protocol versions 1 and 2 at catalog selection, package
validation, discovery and handshake. Version 1 retains its original request and
full-output response format. Version 2 uses separate typed Hello/Welcome,
Command/Reply, Snapshot, Event/Ack, CheckpointAck, Subscriptions and Wake frames.
Frames retain the 256 KiB limit. Required extensions missing from the host
refuse startup; optional extensions are intersected with the host's offer and
reported to plugin code. Unknown message shapes still fail closed.

The SDK publisher compares typed output regions and emits only changed model,
settings, configuration, checkpoint, errors or menus, with increasing revisions.
Replies contain typed error codes independently of region publications. The
host normalizes v1 and v2 replies into its existing runtime vocabulary without
turning protocol-v2 error codes into string-dispatched business logic.

Subscriptions atomically replace requested feeds, bounded by the manifest's
existing anonymous-input and pet-context capabilities. Subscription updates
cannot grant operating-system access. Native plugins remain a trust boundary,
not an operating-system sandbox.

## Scheduling and event delivery

V2 receives no global periodic Tick. A plugin asks for a single monotonic delay
between 34 ms and 24 hours, or no scheduled work. Its actor wakes at the deadline,
not on a polling timer. Identical schedule declarations accompanying ordinary
input do not postpone an already scheduled timer. Handling a tick or timer-
changing operation allows the next deadline to be declared again.

The service owner is woken by commands, process projections/completions, input,
HUD interactions, viewport changes, pet geometry/state, background IO and stop.
It parks indefinitely when no work has a deadline. Compatibility v1 plugins
retain 200 ms ticks. The legacy counters-only source without an event waker
retains its 50 ms compatibility poll. Font eviction and bounded retry use their
own deadlines.

V2 bubbles travel outside the replaceable state slot. Transport, actor and UI
handoffs are bounded; a full transport queue refuses further delivery rather
than silently acknowledging loss. Accepted event IDs are acknowledged, and
duplicates are suppressed for the entire session using a contiguous admitted
sequence frontier. Wire event IDs start at 1 and increase by one (u64); they are
independent of the bubble's application business ID. Admission failure rolls the
frontier back. Later IDs receive EventRejected until the gap is retried; without
the retry extension, a gap is a protocol error. Replays below the frontier are
acknowledged without delivery, using constant memory. The SDK admits
256 unacknowledged events. Application business IDs must be persisted if retries
must survive process restart; no cross-restart exactly-once guarantee is made.

## Durable state and upgrade safety

Legacy sessions omit the checkpoint schema field and read as version 0. V1
sessions continue to omit that field when written. V2 Restore includes the
schema version; plugin code owns migration or refusal. The bundled bridge
accepts 0/1 and refuses unknown versions before changing business data. One
bounded pre-migration checkpoint is retained. Checkpoint schema changes are
allowed only during startup.

New checkpoints are atomically saved before CheckpointAck, using the revision
of the checkpoint actually saved rather than a newer concurrently received
snapshot. SDK callbacks distinguish a committed checkpoint from a published
one. Failed live package replacement restores the previous package pointer and
the saved host session before restarting the old plugin. Business-file formats
for Todo, Mail and Cyber Work are unchanged and remain plugin-owned.

The official source packages move to protocol 2/version 0.2.0. Frozen v1 bundled
packages remain unchanged as compatibility fixtures. A local production candidate
must explicitly select the exact Windows build directory when preparing its v2
catalog and use the existing `--bundle-local-plugins` digest pin. A real-process
v1-to-v2 test verifies that enabled intent, old package directories, Todo data,
Cyber Work counters and disabled Mail data survive an update. No signature,
remote catalog or release is published by this work.

## Verification

The SDK has capability, revision, event duplicate/backpressure and unchanged-
deadline contracts. Real SDK example processes cover checkpoint persistence,
failed upgrades, acknowledged events and exactly three requested timer wakes.
The three companion packages are exercised through the asynchronous service,
including automatic resume. Resource measurements use an isolated HEAD host/SDK
baseline and the same harness; those figures apply to the plugin service, not
the whole application renderer or game compatibility.
