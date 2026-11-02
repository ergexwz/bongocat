# BongoCat 2.5.1 local delivery and resource verification

The local, unsigned x64 installer is delivered at
`version/BongoCat_2.5.1/BongoCat_2.5.1_x64.exe` in the outer checkout.
Size: 239,392,473 bytes. SHA-256:
`8422055e331557076ef420f271b1d2a9d0957c9d62677dfc47b4f7496ad1e393`.
No commit, push, tag or online release was made.

## Plugin startup and deployment

The executable the user ran in `bongocat_final` had SHA-256
`bcc700c789fbe60cae1d3688fdedaeafecc7d94b8de67a343bec16f44c3f6e25`.
It did not match the newer source builds and lacked the independently named
plugin broker. The previous broker fixes were present in source/build output,
but had not reached that runtime directory. This delivery synchronizes the
host, broker, hash-pinned bundled catalog, three companion packages, VC DLLs and
offline WebView2 installer with the actual 2.5.1 installer payload.

The existing independent broker avoids a main-image RUNASADMIN compatibility
flag affecting plugin launch. Its binary pipe bridge flushes replies, preventing
length-prefixed frames waiting forever in line-buffered stdout. Those fixes
were verified again through the final extracted broker; no timeout increase or
protocol change was needed here. Existing companion sources were rebuilt with
custom-protocol and their binary hashes match all three bundled archives.

The installer was extracted into `target/v251-验收 目录`. Archive integrity passed
(189 files). Host and broker match the Production release output byte for byte;
catalog/archive/dependency/runtime hashes match the synchronized runtime.
Each model has 14 valid declared file references. Model pixels and binary assets
match the user's runtime, while packaged JSON differs only in formatting. Existing
runtime models and upstream provenance were preserved. Packaged `--help` exits
successfully from an unrelated working directory.

Actual-payload tests passed: plugins install disabled, then all three enable,
tick for 40 seconds and stop successfully; all three also handshake/start/tick/
stop through the ordinary broker. Test data was isolated from personal data.
This is not an interactive elevated-host linked-token acceptance test.

## Performance

Windows clipping masks now store their eight-bit coverage in R8_UNORM instead
of BGRA8. Unaligned mask texel storage falls by 75%, with unchanged dimensions,
coverage precision and source-alpha blending. WARP executes the actual shader
and blend state and confirms overlapping faint/partial/opaque coverage matches
the old BGRA alpha bytes. Driver alignment and total process memory may differ.

This delivery also makes the existing transparent-border key cropping and lazy
companion-window lifecycle available in the user's runtime. Texture tests report
level-zero RGBA bytes before/after cropping:

| Preset | Full canvas | Cropped |
| --- | ---: | ---: |
| standard | 45,929,376 | 3,363,424 |
| keyboard | 49,395,744 | 3,583,996 |
| gamepad | 10,399,104 | 885,852 |

For companion measurements, the same manager test ran the three original dist
archives and then the final bundled archives with empty isolated data, all three
enabled and no requested panel opening. Sampling was bounded to 40 seconds plus
startup/shutdown; medians below use samples after second 15. Private commit is
not physical RAM. WebView2 is attributed separately from native plugin processes.

| Component | Original packages | Final bundled packages |
| --- | ---: | ---: |
| Three plugin processes, sum private commit median | 34.14 MiB | 8.48 MiB |
| WebView2 process group, sum private commit median | 995.08 MiB | 0 MiB |
| WebView2 process IDs observed after warmup | 26 | 0 |

This verifies idle enabled-plugin savings from avoiding eagerly constructed
hidden WebViews. Opening panels still creates WebView2 processes. These numbers
are short scenario measurements, not long-run bounds or a general whole-app RAM
reduction percentage. Raw data: `target/v251-old-plugins.csv`,
`target/v251-new-plugins.csv`, `target/v251-plugin-memory-summary.json`.

The initial updated-host comparison encountered another running `BongoCat.exe`
and returned `WakeFailed`. After the user exited that instance, the delivered
2.5.1 host ran 45 seconds and exited normally. The preserved original executable
was then rerun for 45 seconds from the same runtime directory with the current
configuration. Configuration SHA-256 before/after the controlled old run and
after the subsequent settings smoke remained
`bbc190516aba38d5dc034d06e107cf61f7d23733c717ea10bcffc428431eb1e3`.
Both hosts exited with code 0; samples after second 15 give these main-process
medians, excluding companion and WebView2 processes:

| Main process metric | Original executable | 2.5.1 |
| --- | ---: | ---: |
| Private commit | 392.41 MiB | 326.13 MiB |
| Private working set | 209.12 MiB | 206.73 MiB |
| Total working set | 273.06 MiB | 268.85 MiB |
| Dedicated GPU memory | 159.85 MiB | 69.14 MiB |
| Shared GPU memory | 5.77 MiB | 4.77 MiB |

Private commit fell approximately 66.28 MiB (16.9%) and dedicated GPU memory
approximately 90.71 MiB in this short scenario. Resident working-set savings are
much smaller; these metrics must not be described as interchangeable RAM savings.
This is a sequential observation rather than a randomized benchmark or soak.
Cyber Work was enabled in the existing profile; its activity/hover and child
lifetimes differed between runs. Main-process counters are kept separate.
Raw data: `target/v251-controlled-old.csv`, `target/v251-updated.csv`,
`target/v251-controlled-config.json`, `target/v251-host-memory-summary.json`.
The earlier 338.21 MiB host baseline predates this configuration and is not used
for the controlled comparison.

The delivered Production host also exited with code 0 from the existing
`--run-seconds 4 --settings-window-smoke` test, which checks settings rendering,
destruction on close, fresh-entity reopening and runtime continuity. Native
application logs confirm startup and completed shutdown for these runs. This is
automated lifecycle acceptance, not a manual visual-layout review.

## Checks and limits

- Windows overlay: 113 passed, including the new WARP coverage regression.
- Platform: 70 passed, 3 ignored; packaging: 37 passed; plugin manager: 32 passed.
- Packaging/version/target/Swift tool contracts: 47 passed.
- Actual extracted-payload acceptance: 2 passed.
- Formatting, affected-crate clippy and Production application clippy passed.
- Native release and NSIS packaging passed. `just` was unavailable, so its exact
  cargo-packaging recipe was used. The first sandbox package attempt could not
  resolve tool downloads; reusing cached NSIS outside the sandbox succeeded.
- Initial manager tests failed on sandbox TEMP access; rerunning unchanged tests
  with repository-local TEMP passed. The Python target test initially lacked
  PyYAML; a target-local test dependency resolved it. No system Python change.
- Full workspace checks were not rerun; prior checkpoint records unrelated
  locale/source-contract failures. No macOS validation was performed.
- Clean-machine install, upgrade/uninstall, missing-runtime installation, full
  elevated-host UAC/token flow, game compatibility and long-duration soak remain
  untested. No production mail credentials were used.

Changelog: rewrote the existing optimization entry in both languages to include
mask storage and independent plugin-launcher delivery; no duplicate entry added.
No personal data needs deletion. User's pre-existing AGENTS.md deletion and
AGENTS1102.md remain untouched. Build caches remain in their original locations:
host target approximately 55.869 GiB and companion target 10.100 GiB at delivery
verification, before temporary extracted-payload cleanup.
