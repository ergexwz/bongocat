# Optimization checkpoint - 2026-11-01

Status: the portability follow-up produced a local Windows installer; clean-machine installation acceptance and the overall optimization plan remain incomplete. Do not publish or replace the protected release executables.

## Current implementation

- Windows ordinary startup no longer repeats the administrator recommendation. Optional UAC preferences, same-account/session/executable handoff, medium-token plugin brokers, low-privilege startup tasks and explicit legacy-permission/runtime-repair entries are implemented in source. Actual UAC/token acceptance is still unverified.
- Repeated topmost reassertion was removed. Key textures crop transparent pixels with original-canvas placement; equal-size HUD resources are reused; DXGI occlusion probes avoid redrawing an occluded scene.
- Companion views are lazy and close by destroying the window. Rust owns mail reconnection/persistence and todo notifications. CSP was restricted. The initial implementation deadlocked when constructing a WebView on the UI event loop; it now uses a bounded serial window worker, independent of protocol responses.
- Installer source adds authenticated bundled-plugin seeding (disabled once, respecting removals), Microsoft-runtime preflight, offline WebView2 setup/retry, startup cleanup and optional production-data removal. The portability follow-up below generated a complete local installer; clean-machine installation still needs acceptance.
- The immutable release directories, development executable outside target/, exported model assets and generated provenance were not replaced. No commit or push was made.
- Explicit overlay visibility now persists through the revisioned atomic configuration store and is restored on startup. Old documents default to visible. Hover/idle hiding and occlusion do not save a new preference. Failed runtime commands roll back the preference; deliberately hidden startup no longer fails the shutdown first-frame check.

## Resumed pass

### Portability follow-up

The initial installer (239,372,964 bytes), SHA-256 `1e87fab3f79d712e2f53597d23d2d27b0464ada2c43a84d2caa3f07f06e04d78`, is superseded by `target/package/2026-11-02/BongoCat_2.5.0_x64.exe` (239,396,086 bytes), SHA-256 `b994cb617648d4d48198f75410f2a75ff8f74b3e6148a624031f99d98381a8d1`. It is a local unsigned build, not a published release. See [Windows installation and verification](../windows-installation.md) and the [installer feedback fixes](installer-fixes-2026-11-02.md) for the confirmed permission/transport defects, matching backgrounds and final extracted-payload tests. Older verification below is historical; the follow-up does not establish clean-machine, elevated-host, game or performance acceptance.

- Companions rebuilt with `custom-protocol` so frontends are embedded. The installer includes all three models, all three companion ZIPs, app-local VC DLLs and the Microsoft-signed x64 WebView2 offline installer.
- The explicit local-bundle option compiles a catalog digest into the host; catalog/ZIP tampering is rejected. The online catalog key is unchanged. No production signing key was generated.
- Production model resolution no longer falls back to the developer checkout. PowerShell signature verification resets an incompatible inherited module path.
- Plugin manager: 32 tests; packaging: 37; packaged resource paths: 2; packaging/Swift tools: 35. App Production clippy passed. An additional acceptance test ran the three actual plugin packages extracted from the installer with isolated data; all installed disabled, then enabled, ticked and shut down successfully.
- NSIS compiled its generated script outside the sandbox because the 32-bit compiler fails inside it with DLL relocation error 623. The generated template was not edited. 7-Zip installer integrity passed; the extracted payload in a Chinese/space path has complete model references and matching dependency/package hashes. The extracted Production host returns help successfully from an unrelated working directory.
- This account already has Production data, so the installer was not executed over the existing environment. Fresh Windows 10/11 installation, missing-runtime setup, full GUI startup, upgrade and uninstall remain untested. Protected releases/provenance and the unrelated background-image edits remain preserved.

- Cargo host/config tests passed: 176 application library tests and 84 configuration tests (one ignored). UI tests passed: 259; protocol: 46; overlay: 112 on a serial pass. The new demotion test failed once during parallel native-window tests, at the test's own demotion step, then passed alone and in the serial pass. This is recorded rather than treated as proof against all window-order races.
- The three companion frontends and release binaries rebuilt successfully; companion clippy passed. Real-process panel tests: Cyber Work 240.13 s / 2368 requests / one reopen / exit 0; todo 240.19 s / 2382 requests / one reopen / exit 0; mail 240.09 s / 2381 requests / exit 0, followed by a separate close/reopen pass of 64.06 s / 639 requests / exit 0. Native window IDs changed after destruction/recreation. Cyber Work continued persisting elapsed game time while the pet and history view were hidden. These used synthetic SDK business messages in isolated profiles, not game input. No authenticated mail account was exercised.
- WebView2 still occasionally reports Chrome_WidgetWin_0 unregister error 1412 at shutdown. No hanging companion process was observed after the completed panel tests; do not claim the warning is resolved.
- New signed-bundle tests passed (2): release-key/digest/path validation, initial disabled installation, preservation of business data, and respecting user removal. Test keys are disposable fixtures, not production signing keys.
- Full just check passed its formatting/clippy stages and reached workspace tests, where the existing catalog coverage test fails on seven unused plugin text keys in every locale. HEAD has the same keys without call sites. The serial workspace run with only that test skipped passed through host, overlay, plugin manager/service and UI; update integration then hit sandbox loopback denial 10013. All 9 update integration tests passed when rerun outside the sandbox. Release workspace check passed. These checks predate the last Windows-only dependency adjustment.
- The tool suite with the correct Cargo PATH and repo-local TEMP ran 103 tests: two existing failures (the locally patched SDK differs from the upstream git-pinning contract; a path assertion assumes Unix slashes) and three environment errors (Windows symlink privilege and two modules requiring missing PyYAML). The relevant packaging and Swift dependency checks passed separately: 35 tests.
- Tool checks exposed an unintended macOS transitive dependency through the new process broker. The plugin-manager platform dependency is now Windows-only; other platforms retain std::process launching. The dependency detector now understands Windows-only Cargo tables. This final change still needs Cargo formatting/clippy/test/release-check confirmation; the last release executable was built before it.
- A new isolated host profiling build succeeded with Cargo 1.97.1. Its attempted five-minute resource observation was interrupted by Escape after 100.47 s (341 rows). Only the verified test process tree was terminated. Raw partial data is target/resource-profile-final.csv; this interrupted run is not acceptance evidence. No matching old-build baseline exists, so main-process RAM savings remain unestablished.

## Verification so far

- Cargo 1.97.1 is available on this Windows host.
- Overlay baseline: 111 tests passed. Separate built-in crop verification passed for all 122 images, preserving every visible pixel and original coordinates. Logical base RGBA bytes: standard 45,929,376 -> 3,363,424; keyboard 49,395,744 -> 3,583,996; gamepad 10,399,104 -> 885,852. These numbers are texture storage, not process RAM savings.
- Configuration tests after updating the default fixture: 83 passed, 1 ignored.
- Companion workspace after the window-worker fix: 41 tests passed. Cyber Work release build succeeded. Its real-process 25-second window-open/reopen test completed 254 protocol requests, maximum response 22.46 ms, exit 0. This checks continued response while creating a WebView, not a full close/reopen visual acceptance test. WebView2 printed a class-unregister warning at exit; investigate on the next GUI acceptance pass.
- Todo and mail frontend builds passed before the last small removal of unused notification code. Rebuild them before final delivery.
- The full just check reached workspace tests but failed on the shared default fixture missing the new elevation field. That fixture and the config tests are now fixed; the full check has not yet been rerun on the final tree. Clippy stages had passed before subsequent window-worker and fixture changes.
- Computer Use was stopped with the physical Escape key. Do not resume UI automation until the user asks to continue. Earlier GPUI window capture also reported no screenshot targets; do not call the settings visual checks passed.

## Resource observation (diagnostic, not acceptance)

The isolated development host ran with settings requested open and all three plugins initially enabled. Sampling lasted 500.05 seconds (928 per-process rows), within the requested ten-minute budget. The initial Cyber Work binary stopped responding after its first WebView creation and was terminated around second 60; that defect was fixed afterward. Therefore this is not a successful full-duration three-plugin stability result.

After 30 seconds, host private commit median was 317.96 MiB and private working set median 206.56 MiB. The observed process-group private-commit median was 324.57 MiB, peak 504.04 MiB. WebView creation produced a temporary browser-process group. No equivalent old-build controlled baseline was captured, so no main-process improvement or percentage reduction is established. Raw counters remain at target/resource-profile.csv. Profiled host SHA256: 32969ab9e5a8580d2e7df124cae4686ee83df4ca827cd256d033204f21473f79.

## Resume here

1. Continue final comprehensive regression with the pre-existing failures tracked. The Windows-only dependency adjustment is included in the current Production installer build and affected-module checks; do not repeat unchanged builds unnecessarily.
2. Resume GUI acceptance only after the user asks to continue. Companion create/destroy/reopen checks are complete; GPUI settings close/reopen, model switching and saved-hidden native startup still need acceptance. Investigate the WebView2 shutdown warning.
3. Complete the remaining checks after addressing or explicitly reporting the pre-existing catalog/tool failures. Verify native elevation/proxy token boundaries and the installer with interactive acceptance.
4. Investigate main-process memory, especially before/after opening and closing GPUI settings and model previews. Use a genuinely matching baseline and keep any new resource observation within ten minutes. Do not describe the invalid earlier sample as a performance win.
5. Test the generated installer on a clean Windows 10/11 account/machine, including missing runtime, upgrade and uninstall. Record Identity V PC, PEAK and The Last of Us Part I compatibility separately; all three games remain untested.
6. target/release now contains the Production installer host with the bundled-catalog pin, not the earlier profiling executable. All three companion binaries have been rebuilt with custom-protocol. Protected release executables remain unchanged.

## Workspace

The upstream fetch found origin/main one commit ahead, including source and protected release-binary changes. It was not merged automatically. Temporary .dsh-scratch data is removed at pause; build caches and diagnostic logs remain in their original target directories. Changelog: one new optimization entry per language, with the overall change still under verification.

Build-cache sizes after the portability follow-up: {"bongocat_final/source/target": 46.728, "bongocat_plugins/target": 10.100} GiB. They remain in their original directories.

The final status also first exposed modifications to development model backgrounds
`bongocat_final/resources/models/gamepad/resources/background.png` and
`bongocat_final/resources/models/standard/resources/background.png`. Their origin
has not been established; preserve them and inspect attribution before any asset
cleanup. No intentional image edit belongs to this optimization task.
