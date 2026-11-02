# BongoCat 2.5.2 local verification

Unsigned Windows x64 installer: `version/BongoCat_2.5.2/BongoCat_2.5.2_x64.exe` in the outer checkout, 239399876 bytes. SHA-256 `71988020ed5502bdbacf09199593c407a194c41496b7e99bebfe0e719739f9c0`.

The installer archive passed integrity and extraction (189 files). Extracted production host SHA-256 is `62d33db72cf1851497cf5a85802ba62cbdf2d21b98f410d007c240d463d42c7d`; the direct-run host was synchronized to these exact bytes and the extracted broker. The subsequent profiling build is Development, not the delivered production executable. No online release, signing change, commit or push was performed.

## Functional verification

- Config: 85 passed, 1 ignored. Includes a pre-quality v1 document with 120 FPS: original backup is retained, upgrade selects low/30, later high/60 choice survives reload.
- Windows overlay: 113 passed, including WARP shader/blend verification.
- Plugin manager: 33 passed, including offline pinned discovery and exact retired IDs retaining their files without registration or reinstallation.
- Plugin service: 5 passed. Application: 176 passed. Runtime: 115 passed. Settings protocol: 46 passed.
- Actual extracted-payload tests: 2 passed. Bundled plugins discover offline, install disabled, enable/tick/shutdown, and handshake through the extracted ordinary broker.
- Production application clippy and formatting checks passed. Eight locales passed validation (349 keys each). Existing hard-link cache warnings are environment warnings.
- Native isolated Development settings smoke exited 0 and confirmed exactly Todo, Mail and Cyber Work cards, settings close/destruction/reopening and continued runtime. Its data was under `target/profile252-visible`. A separate native settings-open run exited 0.
- Manual screenshot inspection did not complete because the computer-use app approval timed out. Production clean-machine install/upgrade/uninstall, game compatibility, macOS and a full workspace check were not performed. The subsequent interactive elevated-host test failed, as recorded below.

## Memory experiment

Same-source Release Development builds permit repository-local profile injection; production rejects that switch. Three companion plugins are enabled for each run. High/60 is sampled for 45 seconds, low/30 for 600 seconds; settings are closed. Source model assets and original profiles are untouched. Raw samples are `target/measure252-high.csv` and `target/measure252-low.csv`; process groups include only descendants of the test host. Main private commit, private working set, total working set, plugin private commit and WebView2 private commit are recorded separately. Shared resident pages must not be added as physical RAM usage.

The preserved production 2.5.1 baseline refused to write its log in the sandbox. No active-profile workaround was used. Consequently this run does not establish a controlled 2.5.1-to-2.5.2 RAM percentage or prove the 20 percent goal. High-versus-low comparisons establish the quality setting effect within 2.5.2 only. The ten-minute run is bounded observation, not an eight-hour soak.

The completed summary is below. Controlled 2.5.1 comparison remains unverified.

## Local state and delivery

Previous user changes (including the root AGENTS.md deletion and AGENTS1102.md) were retained. New ADR-0102, this benchmark, and the 2.5.2 installer are intended additions. Existing untracked broker, bundled packages, dependencies, ADR-0101 and 2.5.1 artifacts predate this work. No personal data needs deletion. Caches remain at the original host and companion target directories, approximately 66.820 GiB and 10.100 GiB at the pre-observation scan. Test profiles, extracted payloads, raw data and the preserved pre-delivery executables remain under the ignored host target directory.

Changelog: rewrote the existing optimization entry in both languages; no duplicate entry added.

## Completed resource samples

| Metric (MiB, median after second 15) | High / 60 FPS | Low / 30 FPS |
| --- | ---: | ---: |
| Main private commit | 104.91 | 91.51 |
| Main total working set | 74.89 | 70.16 |
| Main private working set | 26.24 | 26.58 |
| Three native plugins private commit | 12.27 | 11.97 |
| WebView2 descendants private commit | 165.18 | 166.36 |

High has 5 post-warmup samples; low has 102. Both native processes exited 0. Low ran for a nominal 600 seconds. Main private commit fell 12.8% and total working set fell 6.3% between these settings; private working set did not meaningfully decrease. These are different metrics and must not be described as an overall physical RAM reduction.

Low main private commit range: [90.895, 92.43] MiB; total working set range: [69.27, 71.168] MiB. Handles: [917.0, 927.0]; threads: [133.0, 138.0]. Small memory drift during this bounded run is not a long-run leak verdict. WebView2 descendants appeared and exited during sampling; their commit is not attributed to the host.

Additional Python distribution/packaging contracts: 35 passed, 1 failed. The existing local path SDK dependency fails the upstream-Git-revision contract; the local dependency was present before this task and was retained for companion development. See `target/contracts252.log`.

## Final native model matrix

The isolated Release Development settings lifecycle smoke passed all nine combinations: standard, keyboard and gamepad at low, medium and high quality. Low/medium used 30 FPS and high used 60 FPS. Every process exited 0 and recorded application startup and completed shutdown. Exact three companion cards were required by the smoke. Results: `target/matrix252-results.json`. Windows overlay/config/service tests and formatting were rerun after the final source changes and passed. The final core total is 573 passed and one explicitly ignored configuration test. These automated checks do not replace manual image-quality review.

## Subsequent interactive administrator diagnosis

An isolated copy of the user Cyber Work 0.1.0 package and data started in an ordinary process. The installed executable matched the bundled ZIP SHA-256. An interactive administrator test against the unchanged 2.5.2 broker reproduced Windows error 1346 (invalid impersonation level). Experimental shell-token approaches subsequently failed with error 1314 (privilege not held) or error 5 (access denied); those experiments were reverted and are not a fix. The original user data was never modified. The bundled icon lookup repair is present in source and passed manager/service and extracted-package tests, but has not been packaged or installed.
