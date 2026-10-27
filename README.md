# BongoCat workspace

This repository contains three things:

## 1. `version/BongoCat_2.4.1/` — minimal portable release (recommended)

Only the files users need: `BongoCat.exe`, trimmed `resources/models`, the
app-local VC++ runtime, a launcher, and licenses. No source, build scripts, or
original release artifacts.

Copy the whole `BongoCat_2.4.1` folder to a local Windows directory, then run:

```text
version\BongoCat_2.4.1\run.bat
```

or:

```text
version\BongoCat_2.4.1\BongoCat.exe
```

Requirements: Windows 10 1903+ x64. See
[version/BongoCat_2.4.1/README.md](version/BongoCat_2.4.1/README.md).

## 2. `bongocat_final/` — development build and full source

Locally built BongoCat 2.4.1 (based on upstream 2.4.0) with the Xbox/XInput
background-gamepad fix and the zero-scan-code input-reset fix.

```text
bongocat_final\run.bat
```

The full modified source is in `bongocat_final/source/`; the local patch is
`bongocat_final/bongocat-xinput.patch`; build steps are in
`bongocat_final/BUILD.md`.

## 3. Root — original packaged BongoCat 2.4.0 release

The root `bongocat-app.exe`, `uninstall.exe`, and `resources/` are the untouched
original release. They do **not** contain the Xbox/XInput fix. Keep them for
reference or rollback; use `version/BongoCat_2.4.1` for the fixed build.

Do not run multiple BongoCat builds at the same time: they share the
single-instance identity and `%APPDATA%\com.ayangweb.bongo-cat` configuration.
