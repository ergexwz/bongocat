## BongoCat 项目优化

程序原先有点小问题，不能正确的反应xbox手柄，现在做了个人修改来优化体验。

目前有三个插件：(1) todo清单、(2) 邮件整理、(3) 点击计数

原项目链接：https://github.com/ayangweb/BongoCat

**2.5.2 安装版**：运行 `version/BongoCat_2.5.2/BongoCat_2.5.2_x64.exe`。安装包包含主程序、三个插件、独立插件启动组件和离线运行库。先从托盘退出旧版再安装；首次安装的插件默认关闭，在设置 → 插件中心按需启用。旧版目录保留。

**直接运行**：`bongocat_final/run.bat` 的运行目录在本次交付后同步到 2.5.2。请保留整个目录，不能只复制主 exe；管理员模式启动插件时还需要同目录的 `bongocat-plugin-broker.exe`。

**伴随插件**：`bongocat_plugins/dist/` 已提供待办、邮件与“赛博打工”三个独立插件 ZIP；在
插件中心“从文件安装”选择即可。源码与重新打包方法详见 `bongocat_plugins/README.md`。
邮件插件 0.1.1 起，邮件列表、归档和邮件设置窗口支持标题栏缩放、右下角拖动缩放
（固定宽高比，每个窗口的比例会保存）。赛博打工在桌宠头顶显示键鼠敲击计数器，并在
gamepad 模式下统计游戏时长；右键桌宠 → 插件 → 计数器缩放 / 使用历史。

**开机自启动**：登录任务会记录当前 `BongoCat.exe` 的实际位置；移动程序文件夹后，
正常启动会尝试把任务更新到新位置。若设置页提示“应用位置已变化”，
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

Locally built BongoCat 2.5.1 (based on upstream 2.4.0) with the Xbox/XInput
background-gamepad fix, the zero-scan-code input-reset fix, and the companion
plugin system (todo / mail / Cyber Work plugins are bundled and initially disabled).

```text
bongocat_final\run.bat
```

The full modified source is in `bongocat_final/source/`; the local patch is
`bongocat_final/bongocat-xinput.patch`; build steps are in
`bongocat_final/BUILD.md`.

Do not run multiple BongoCat builds at the same time: they share the
single-instance identity and `%APPDATA%\com.ayangweb.bongo-cat` configuration.

2.5.2 将插件中心切换为随包的待办、邮件和赛博打工，旧番茄钟、输入法状态、按键音效的文件与数据保留但不再运行。设置中的画质支持低、中、高，帧率支持30/60；首次升级默认低画质30帧，并备份原配置。
