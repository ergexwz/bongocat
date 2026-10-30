## BongoCat 项目优化

程序原先有点小问题，不能正确的反应xbox手柄，现在做了个人修改来优化体验。

原项目链接：https://github.com/ayangweb/BongoCat

**下载教程**：下载 `bongocat/version/BongoCat_2.5.0` 整个文件夹即可食用（2.5.0 不提供 ZIP；`BongoCat_2.4.1` 目录与 ZIP 保留为旧版归档）。

**伴随插件**：`bongocat_plugins/dist/` 已提供待办与邮件两个独立插件 ZIP；在
插件中心“从文件安装”选择即可。源码与重新打包方法详见 `bongocat_plugins/README.md`。
邮件插件 0.1.1 起，邮件列表、归档和邮件设置窗口支持标题栏缩放、右下角拖动缩放
（固定宽高比，每个窗口的比例会保存）。

**开机自启动**：登录任务会记录当前 `BongoCat.exe` 的实际位置；移动程序文件夹后，
第一次以管理员身份启动会自动把任务更新到新位置。若设置页提示“应用位置已变化”，
重新开关一次即可修复。

## 开发者说明

做了两个地方：

# 1. `version/BongoCat_2.5.0/` — minimal portable release

Runtime-only release: `BongoCat.exe`, trimmed `resources/models`, and the
app-local VC++ runtime (`vcruntime140.dll`, `vcruntime140_1.dll`). No source,
build scripts, docs, licenses, or original release artifacts.

Copy the whole `BongoCat_2.5.0` folder to a local Windows directory, then run:

```text
version\BongoCat_2.5.0\BongoCat.exe
```

Requirements: Windows 10 1903+ x64.

# 2. `bongocat_final/` — development build and full source

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
