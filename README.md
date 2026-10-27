# BongoCat v2.4.0 workspace

This repository contains two things:

## 1. `bongocat_final/` — run this on Windows

Locally built BongoCat 2.4.0 with the Xbox/XInput background-gamepad fix and the
zero-scan-code input-reset fix.

```text
bongocat_final\run.bat
```

or:

```text
bongocat_final\bongocat-app.exe
```

Requirements: Windows 10 1903+ x64. Copy the whole `bongocat_final` folder to a
local Windows directory before running; do not launch it directly from the WSL
`\\wsl.localhost\...` UNC path. The folder is self-contained: keep
`bongocat-app.exe`, `resources/`, and the app-local `vcruntime140*.dll` together.
See [bongocat_final/README.md](bongocat_final/README.md) for details.

The full modified source is in `bongocat_final/source/`; the local patch is
`bongocat_final/bongocat-xinput.patch`; build steps are in
`bongocat_final/BUILD.md`.

## 2. Root — original packaged BongoCat 2.4.0 release

The root `bongocat-app.exe`, `uninstall.exe`, and `resources/` are the untouched
original release. They do **not** contain the Xbox/XInput fix. Keep them for
reference or rollback; run `bongocat_final/bongocat-app.exe` for the fixed build.

Do not run the root release and `bongocat_final` at the same time: they share the
single-instance identity and `%APPDATA%\com.ayangweb.bongo-cat` configuration.
