## BongoCat 项目优化

程序原先有点小问题，不能正确的反应xbox手柄，现在做了个人修改来优化体验。

原项目链接：https://github.com/ayangweb/BongoCat

**下载教程**：下载项目下面的 bongocat/version/BongoCat_2.4.1 或者 bongocat/version/BongoCat_2.4.1.zip 即可食用。

## 开发者说明

做了两个地方：

# 1. `version/BongoCat_2.4.1/` — minimal portable release

Runtime-only release: `BongoCat.exe`, trimmed `resources/models`, and the
app-local VC++ runtime (`vcruntime140.dll`, `vcruntime140_1.dll`). No source,
build scripts, docs, licenses, or original release artifacts.

Copy the whole `BongoCat_2.4.1` folder to a local Windows directory, then run:

```text
version\BongoCat_2.4.1\BongoCat.exe
```

Requirements: Windows 10 1903+ x64.

# 2. `bongocat_final/` — development build and full source

Locally built BongoCat 2.4.1 (based on upstream 2.4.0) with the Xbox/XInput
background-gamepad fix and the zero-scan-code input-reset fix.

```text
bongocat_final\run.bat
```

The full modified source is in `bongocat_final/source/`; the local patch is
`bongocat_final/bongocat-xinput.patch`; build steps are in
`bongocat_final/BUILD.md`.

Do not run multiple BongoCat builds at the same time: they share the
single-instance identity and `%APPDATA%\com.ayangweb.bongo-cat` configuration.
