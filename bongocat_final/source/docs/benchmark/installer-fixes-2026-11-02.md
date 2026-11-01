# Installer feedback fixes - 2026-11-02

Status: fixed installer built; extracted payload acceptance passed. No commit or
publication requested. Interactive elevated-host and clean-machine acceptance
remain untested.

Artifact: `target/package/2026-11-02/BongoCat_2.5.0_x64.exe`, 239,396,086 bytes,
SHA-256 `b994cb617648d4d48198f75410f2a75ff8f74b3e6148a624031f99d98381a8d1`.
The local installer is unsigned. It supersedes the 2026-11-01 installer whose
SHA-256 begins `1e87fab3`.

## Plugin startup

The reported installation is `D:\Users\ergex\AppData\Local\BongoCat` and the UI
reports that the plugin cannot start. Read-only inspection found the main image's
existing AppCompat Layers value `~ RUNASADMIN`. Its ordinary CreateProcess call
fails with error 740. A second probe using CreateProcessAsUserW with the caller's
token also fails with 740 for that path, while an isolated byte-identical copy
starts successfully. Both probes use `--help`, never request UAC, and leave the
user's compatibility flags, business data and enable choices unchanged.

The elevated host previously used its own executable path for the medium-token
plugin broker. The installer now includes a dedicated broker component from the
same host image, and the loader supplies its independent path. The broker still
validates ordinary permissions, same-account/session token acquisition, explicit
stdio handle inheritance and kill-on-close job ownership. A fresh user environment
avoids inheriting the elevated parent's compatibility layers. Running the component
without the internal broker invocation exits without initializing product data.

The first real broker-transport acceptance test also exposed a second defect:
`io::copy` wrote replies into line-buffered stdout without flushing. A
length-prefixed JSON frame has no newline, so the host waited for a buffered
reply and the initial handshake timed out. Both bridge directions now flush
each copied chunk. A regression test uses a line-buffered writer and a peer
that requires reply delivery before sending the next request; timeouts are not
increased. Plugin messages remain opaque and the protocol version is unchanged.

## Model backgrounds

The installer staged `source/resources/models`, whose standard/gamepad backgrounds
differed from the user's `bongocat_final/resources/models` reference. The two source
PNGs were synchronized exactly, retaining their filenames and 612x354 dimensions:

- standard: SHA-256 `96a66c7c89201f42b7dabf126ea3c8a7bd8221144a3c9b18088637dfa1d54ad5`
- gamepad: SHA-256 `24d2d51bb9b10b2ef8002bafb6c5c3f2ed7717d9e604f3c7df4c2e1137be93e5`

The reference resources, minimal runtime release, 2.4.1 archive and generated
provenance in the development runtime are not edited. No plugin protocol change
or user-data deletion is part of this fix. The current unreleased CHANGELOG entry
already covers the installer and permission behavior, so no extra entry is added.

## Verification

- Affected tests passed: packaging 37; platform 70 (3 ignored), including the
  binary-frame flush regression; plugin manager 32. Final formatting and clippy
  passed for these modules; Production app clippy also passed during this fix.
- Production release rebuilt using Cargo 1.97.1, the existing target cache and
  `just build --bundle-local-plugins --formats nsis`. NSIS in the sandbox fails
  with DLL relocation error 623; its original generated script compiled outside
  the sandbox without template edits.
- 7-Zip integrity passed (189 entries). Extraction into a Chinese/space path has
  matching companion archives, catalog, offline runtime and VC DLL hashes. All
  three model manifests resolve their 14 declared file references. `file`
  confirms the two backgrounds are 612x354 RGBA PNGs; hashes match the reference.
- Extracted host and independent broker are byte-identical to the Production
  build, SHA-256 `b7c4331fbadd21ad68db4342f0660c3bddba6f626d513f731dbe122997256d19`.
- Both actual-payload ignored acceptance tests passed (5.47 seconds total):
  bundled installation defaults disabled, direct plugin enable/tick/shutdown,
  and each companion's handshake/start/tick/stop through the ordinary broker.
  Direct helper invocation is refused without initializing an ordinary host.
- The final helper also passes the native CreateProcessAsUserW `--help` creation
  probe, while the existing main path still returns 740. These probes run under
  the ordinary caller, not through an elevated-host linked-token launch.
- Existing installed plugin images differ from the bundled images. Copies of
  the user's Cyber Work 0.1.0, mail 0.1.1 and todo 0.1.0-dev.202610292121 also
  passed handshake/start/tick/stop through the final helper with isolated empty
  business data. Existing packages, data and selected versions are untouched;
  this fix does not silently replace installed plugins.
- No automatic UAC acceptance or game-process inspection is used.
- The full elevated host handoff and clean-machine installation remain interactive
  acceptance cases; the compatibility failure itself was reproduced on this machine.

## Delivery and workspace

The conventional `target/package/BongoCat_2.5.0_x64.exe` path also contains the
same fixed bytes, so an earlier local artifact link no longer selects the broken
installer. The dated directory includes its SHA-256 sidecar and `verification.json`.
Exit the old host from the tray before installing over the existing directory;
do not delete personal data. No installation over the user's active profile was
performed by these tests.

The 35 packaging/Swift dependency tool tests passed with repository-local TEMP.
Their first attempt used sandbox-managed TEMP and hit two cleanup access errors;
the rerun changed only the temporary directory. All `.dsh-scratch` test copies and
extracted payloads were removed. Existing build caches remain at their original
paths: host `bongocat_final/source/target` 51.484 GiB, companion
`bongocat_plugins/target` 10.100 GiB (logical file sizes). No second target directory
was created. Protected runtime/release directories and provenance have no diff.
