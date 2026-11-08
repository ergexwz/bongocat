# BongoCat plugin SDK

[简体中文](README.zh-CN.md)

Plugins communicate through length-prefixed JSON on stdin/stdout. Keep stdout
exclusively for this protocol. Plugins own their business logic and data; no
host runtime, GPUI object, operating-system handle or Rust ABI crosses the pipe.

## Protocol v2

Implement `Plugin::handle`, then `v2::Plugin`, and call `v2::run`. The companion
bridge provides `run_host_v2` for the bundled window-owning plugins. Declare
`protocol_version: 2` in the package manifest. This requires the updated host;
the host continues to accept protocol-1 packages through its separate adapter.

The manifest's display/input capabilities and the handshake's wire extensions
are separate contracts. Add future wire features as optional, versioned
extensions, and preserve existing frame meanings and the existing default
requirements. A breaking frame shape requires a new negotiated extension or
protocol version. Require `checkpoint.v2` when durable state is essential;
optional features must have a usable disabled behavior.

- `capabilities` declares required and optional versioned extensions. A missing
  required extension refuses startup; `negotiated` receives the selected set so
  optional behavior can be disabled. Current extensions are `regions.v2`,
  `events.v2`, `schedule.v2`, `checkpoint.v2` and `subscriptions.v2`.
- `handle` returns a complete typed `PluginOutput`. The publisher sends only
  changed regions with increasing revisions. A reply acknowledges an operation,
  independently of snapshots and events. There are no arbitrary JSON-path patches.
  Override `handle_v2` to report a specific typed failure such as `StorageFailure`.
  When an optional extension is unavailable, its frames are omitted; attempting
  to publish data that requires it returns `UnsupportedCapability`.
- `subscriptions` atomically replaces the requested input and pet-context feeds.
  It can only narrow the capabilities already declared in the manifest.
- `next_wake_ms` returns one delay between 34 ms and 24 hours, or `None` for no
  scheduled work. A timer requests its next wake after handling the previous one.
  Input activity does not continually postpone an unchanged timer deadline.
- Bubble IDs are unique application event IDs. At most 256 events may await
  acknowledgement. `event_acknowledged` reports acceptance into the bounded host
  queue and returns the application bubble ID. The SDK assigns independent,
  contiguous u64 wire IDs starting at 1. The host suppresses wire replays for the
  entire session with constant memory; rejected admission never advances its
  frontier, and later IDs wait for the missing ID to be retried. The SDK's recent
  1024 application IDs prevent accidental republication of unchanged bubbles;
  that convenience cache does not limit transport replay protection.
  Acknowledgement does not mean the user has read the notification. Persist
  business IDs if retries must survive a crash; cross-restart exactly-once
  execution is not promised.
- Handle `Restore { schema_version, config, state }` before publishing. Version 0
  denotes legacy opaque bytes. Migrate to the plugin's own checkpoint version,
  or return an error without changing its data. The host saves the previous
  checkpoint before migration and atomically saves a new checkpoint before
  `checkpoint_committed`. Checkpoint versions may change only during startup.
  The host keeps one bounded pre-migration backup and rolls back its session
  together with a failed package replacement. Plugin-owned business files need
  their own atomic writes and migrations.

The host admits 64 queued control operations and 1 MiB of control payload per
plugin. Frames are limited to 256 KiB; output validation does not waive the wire
size limit. Calls may return Busy, Timeout, InvalidConfiguration, StorageFailure,
UnsupportedCapability or ProtocolViolation. Do not replay actions automatically
unless their business IDs make retries safe. Optional capabilities are a protocol
contract, not an operating-system sandbox.

## Working examples

New official packages require host 2.5.4 or later. Declare the actual minimum in
`app_versions`. The optional `event_retry.v2` extension reports a full host event
queue with `EventRejected { id }`. The runner retains the original payload and ID
until acknowledgement and retries at most once per 34 ms. Unacknowledged events
are bounded by both 256 entries and 1 MiB of serialized payload. Receiving an
acknowledgement releases that capacity. Existing peers negotiate the five
original required extensions independently of this optional extension.

Plugins subscribing to ordered input implement `v2::Plugin::reset_input` to clear
temporary pressed state after an overflow. Keep business data intact. Request
budgets include time spent in the runner's receive queue. A timed-out action
requires an explicit business retry decision.

`HostSession` replies and snapshots hold `Arc<PluginOutput>`. Host adapters can
share a revision's output until the next publication.

Build without a UI framework:

```text
cargo build --locked -p bongocat-plugin-sdk --examples --release
```

| Example | Plugin ID | Demonstrates |
| --- | --- | --- |
| `event_counter` | `com.example.event-counter` | Event-driven output, acknowledged bubbles and checkpoint migration |
| `scheduled_timer` | `com.example.scheduled-timer` | Three requested wakes followed by idle |
| `companion_hud` | `com.example.companion-hud` | HUD, notifications and pet-visibility subscription |
| `scaling_fixture` | Manifest ID | Protocol-neutral isolated benchmark fixture |

Package the selected executable and `plugin.json` in a ZIP. Include the PNG
when the manifest declares a local-image icon; the supplied examples use emoji icons.
The manifest's `id` must match the example ID, `entry` names the executable
without `.exe`, and `capabilities` must include the data/display features used:
`model_ui`, `persistent_state`, `host_bubble_v1` for the counter and `pet_state_v1`
for the HUD. The host validates both the manifest and the executable handshake.
New application integrations should use an application-version range supported
by their actual host release, rather than relying only on this source checkout.

The host's `just plugin-tests` builds these examples and exercises real v1/v2
processes, failed upgrades, timer wake counts, checkpoints and event delivery.

The three examples include complete manifests in `examples/manifests`. From the
plugin workspace, build and package them without changing host code:

```text
cargo build --locked -p bongocat-plugin-sdk --examples --release --target x86_64-pc-windows-msvc
python crates/bongocat-plugin-sdk/examples/package_examples.py --bin-dir target/x86_64-pc-windows-msvc/release/examples --output target/sdk-example-packages --target x86_64-pc-windows-msvc
```

Use a fresh output directory; existing packages are preserved. The packager uses
only Python's standard library, fails for missing binaries and prints package
hashes. The HUD companion publishes a notification when a hidden pet becomes
visible again. Notification IDs are unique within the current session.
