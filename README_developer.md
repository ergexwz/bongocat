# BongoCat workspace

This repository contains two things:

## 1. `version/BongoCat_2.5.0/` — minimal portable release

Runtime-only release: `BongoCat.exe`, trimmed `resources/models`, and the
app-local VC++ runtime (`vcruntime140.dll`, `vcruntime140_1.dll`). No source,
build scripts, docs, licenses, or original release artifacts.

Copy the whole `BongoCat_2.5.0` folder to a local Windows directory, then run:

```text
version\BongoCat_2.5.0\BongoCat.exe
```

Requirements: Windows 10 1903+ x64.

## 2. `bongocat_final/` — development build and full source

Locally built BongoCat 2.5.0 (based on upstream 2.4.0) with the Xbox/XInput
background-gamepad fix, the zero-scan-code input-reset fix, and the companion
plugin system (todo / mail plugins are installed from local ZIPs).

```text
bongocat_final\run.bat
```

The full modified source is in `bongocat_final/source/`; the local patch is
`bongocat_final/bongocat-xinput.patch`; build steps are in
`bongocat_final/BUILD.md`.

Do not run multiple BongoCat builds at the same time: they share the
single-instance identity and `%APPDATA%\com.ayangweb.bongo-cat` configuration.
