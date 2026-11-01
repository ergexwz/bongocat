# ADR-0100: Windows background permissions and resource lifetime

Status: implementation under verification (2026-11-01).

## Product boundary

Windows starts with ordinary permissions. Administrator status is diagnostic
information, not a prerequisite for the pet or proof of anti-cheat compatibility.
No game memory access, injection, game rendering interception, input simulation,
input driver, anti-cheat evasion or security exclusion is introduced. Raw Input,
pressed-state reconciliation and the existing physical gamepad backends remain.

The user explicitly replaced the eight-hour soak requirement for this task with
a maximum ten-minute resource/stability observation. This does not establish
long-term leak freedom or compatibility with every game.

## Window and input ownership

Creating a window or explicitly changing its topmost preference may set normal
user32 topmost order without activation. Showing or ticking a window does not
restore its topmost band or compete with a foreground window. Being covered does
not overwrite the saved preference or stop host input, plugin ticks or persistence.
This supersedes the repeated topmost guard in ADR-0096. Existing layered-window
composition remains; exclusive fullscreen and game-specific restrictions remain.

`overlay.visible` persists only an explicit user show/hide choice and defaults to
true in older configurations. Startup restores it, including a hidden first run
of the renderer; the shutdown frame check permits that deliberately hidden run.
Failed runtime commands roll back the saved choice through the revisioned store.
This supersedes the session-only visibility policy in ADR-0015.

The renderer crops key textures to nonzero alpha plus a two-pixel filtering
border, translating the quad in original canvas coordinates. Original assets,
legacy framing, pre-multiplication, filtering, mirroring and manifest lookup names
are preserved. Fully transparent keys allocate one transparent pixel. Equal-size
plugin HUD updates reuse their GPU texture and vertex buffer. After DXGI reports
occlusion, visibility is probed without redrawing the scene; input stays active.

Companion windows are created from their existing Tauri configuration on demand.
Creation runs on one bounded serial worker per companion, never in a synchronous
UI event handler: Windows WebView creation can deadlock on the UI event loop.
Protocol responses and background business work do not wait for window creation.
Closing destroys the view; an implicit last-window exit is prevented, while an
explicit host Stop still exits. Mail persistence/reconnection and todo notifications
run in Rust, independently of a hidden runtime WebView. Views rehydrate committed
business state and UI scale. UI and pending notification queues are bounded.
WebView2 may retain environment-level resources after a view closes; measure the
complete process group rather than claiming every byte is immediately released.

## Optional elevation

`system.request_elevation_on_startup` defaults to false when absent in old v1
configurations. Settings offer a one-time UAC restart and a future-start opt-in.
Login tasks always use LeastPrivilege, including path reconciliation and repair
of a previously highest-privilege task. Legacy compatibility flags are inspected
through the executable's Windows property page, by explicit user action.

A UAC child checks its parent's executable, user SID and session, acknowledges
an unpredictable local event, and waits for the parent's normal shutdown to
release state and single-instance locks. Cancellation leaves the original running;
another-account elevation is rejected. Automatic elevation is disabled in harnesses.

In an elevated host, each plugin runs behind an ordinary-permission copy of the
host. The linked token must have the same user/session, medium integrity and no
elevation. Only explicit anonymous stdio handles are inherited. The broker and
descendants belong to a kill-on-close job; failure to acquire or validate the token
fails closed. The ordinary host adds no broker. The existing plugin protocol is
unchanged. Actual UAC and process-token acceptance still requires an interactive
Windows check; compilation alone does not satisfy this gate.

## Installer and verification

Windows packaging verifies the three offline plugin packages against the existing
release-key-signed catalog, checks package identity/digests, and verifies Microsoft's
Authenticode signatures on the offline WebView2 installer and app-local VC runtimes.
For this locally patched distribution, `--bundle-local-plugins` explicitly accepts
a catalog prepared from the companion checkout and compiles its exact SHA-256 into
the host. Runtime authenticates those catalog bytes before checking every ZIP's
digest, identity and safe paths. The pin comes only from the build, never an adjacent
file or runtime environment variable; changed catalog bytes fail closed. Online
catalogs keep the existing release key and signatures. No release keys are generated
or changed. Missing trusted inputs stop installer packaging before building;
`--build-only` creates only a development/test executable.

PowerShell signature verification clears an inherited PowerShell 7 module path
before invoking Windows PowerShell 5.1. Production resource lookup stays beside the
executable even if a bundle is incomplete, so developer source paths cannot mask a
missing installer resource. Artifact acceptance uses the payload extracted from the
generated installer, with isolated plugin data and an unrelated working directory.

Bundled packages are seeded disabled once. Existing packages and user enable
choices are preserved; a persistent record prevents reinstalling a removed plugin
on every start. The uninstaller removes the current user's startup registration;
the existing NSIS optional data removal includes only production user data, never
shared runtimes. Native builds reuse `target/release`.

`--performance-smoke-root` exists only with the storage-test-injection feature,
requires Development and a bounded 1..600-second run, and uses isolated data with
all three local companion packages enabled. `tools/measure-resources.ps1` captures
per-process and process-group counters for at most 600 seconds. Missing counters
remain unavailable rather than zero. Private commit, working sets and GPU memory
are reported separately; shared working sets are not summed as unique RAM.

First game checks: Identity V PC, PEAK and Steam The Last of Us Part I. Record game,
protection component, OS and display mode versions. No protected game is profiled
under a debugger. A result is verified coexistence, restricted, incompatible or
untested; no absent warning is treated as a universal safety guarantee.
