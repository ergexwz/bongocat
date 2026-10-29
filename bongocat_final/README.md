# BongoCat 2.5.0 — Xbox/XInput 后台手柄修复 + 伴随插件版

这是基于 upstream BongoCat 2.4.0 的本地 2.5.0 构建，修复了 Windows 上 Xbox 手柄在桌宠窗口不获得前台焦点时没有输入的问题，并修复了 Xbox HID 键盘报告导致 gamepad/standard 模型频繁自动切换的问题；同时加入待办 / 邮件独立插件系统。

## 直接运行

系统要求：**Windows 10 1903+ x64**。

建议先把整个 `bongocat_final` 文件夹复制到本地 Windows 目录（例如
`C:\BongoCat`）再运行。不要只复制 exe，也不建议直接从 WSL 的
`\\wsl.localhost\...` UNC 路径启动。

双击：

```text
run.bat
```

或者直接运行：

```text
bongocat-app.exe
```

首次运行会在 `%APPDATA%\com.ayangweb.bongo-cat\production` 创建/读取配置，和原版 BongoCat 共用配置目录。

## 目录说明

```text
bongocat_final/
  run.bat                     启动脚本
  bongocat-app.exe            可执行文件（本地构建，未签名）
  vcruntime140.dll            VC++ 运行库（应用本地）
  vcruntime140_1.dll          VC++ 运行库（应用本地）
  resources/
    models/                   内置 standard / keyboard / gamepad 模型
    build-provenance.json     原发布元数据
  bongocat-xinput.patch       2.4.1 时期相对 v2.4.0 的 Xbox/XInput 补丁（历史）
  BUILD.md                    从源码重新构建的步骤
  source/                     修改后的完整 v2.5.0 源码（基于 upstream 2.4.0，无 target/.git）
  THIRD_PARTY_NOTICES.md      第三方组件说明
```

**不要只复制 `bongocat-app.exe`**：程序从可执行文件同目录的 `resources/models` 加载内置模型。分发时请复制整个 `bongocat_final` 文件夹。

## 伴随插件（待办 / 邮件）

完整源码在仓库根目录的 `bongocat_plugins/`，包含两个独立 Tauri 2 插件进程：

- `com.bongocat.todo`：手绘待办面板、快速新建、到期气泡。
- `com.bongocat.mail`：IMAP 邮件列表、归档、设置和桌宠气泡。

已构建的 ZIP 位于 `bongocat_plugins/dist/`。如需重新构建：

```powershell
cd bongocat_plugins
.\scripts\package.ps1
```

然后在 BongoCat 设置 → 插件中心点击“从文件安装”，选择 `dist/` 下的 ZIP。
右键桌宠（或托盘菜单）的“插件”子菜单会打开待办/邮件窗口。

本地安装的插件是可执行代码，宿主会要求确认信任来源；插件数据保存在
`%APPDATA%\com.ayangweb.bongo-cat\production\plugins\data\` 下。

## 修复内容

1. **Xbox 手柄改走 XInput**
   - PS / Switch / 其他 HID 手柄继续走 WGI。
   - Xbox 手柄通过 XInput 后台读取，不再依赖 BongoCat 获得前台焦点。
   - 支持 A/B/X/Y、十字键、LB/RB、LT/RT、左右摇杆、View/Start、L3/R3。

2. **零扫描码 HID 报告不再重置全部输入**
   - Xbox 手柄的 HID 键盘集合可能发送 `make_code = 0` 的报告。
   - 原版会把它当成未知按键并执行全量 `InputReset`，清空活动手柄，导致 gamepad/standard 自动切换。
   - 现在这类报告只计数并丢弃；非零未知扫描码仍按原逻辑处理。

3. **连接重放保护**
   - 已经激活的手柄连接被重复重放时，不再清空其摇杆/扳机轴状态。

## 已知说明

- **Steam Input**：如果 Steam 正在运行并且接管了手柄，BongoCat 可能读不到输入。完全退出 Steam，或在 Steam 设置 → 控制器中关闭 Xbox 控制器的 Steam 输入。
- **按键图**：本次没有修改任何模型贴图或按键图名称。gamepad 模型缺失/错位的按键图需要修改：
  ```text
  bongocat_final\resources\models\gamepad
  ```
  修改 `source/resources/models` 不会被这个可执行文件读取。
- **自动切换**：Xbox 手柄关闭后，切换到 standard 模型会有约 5 秒的断开宽限，用于避免空闲蓝牙手柄反复断连。
- **PS / Switch**：继续使用 WGI 路径，代码未改；本机没有这些手柄，未做实机验证。
- **第三方 Xbox 兼容手柄**：如果 WGI 名称不含 `xbox` 且 vendor 不是 `0x045e`，仍会走 WGI；微软原装 Xbox 手柄走 XInput。
- **未签名**：这是本地构建，Windows SmartScreen 可能提示。选择“更多信息 → 仍要运行”。
- **不要和根目录的原版同时运行**：两者共用单实例身份和配置。原版发布包保持原样，可随时回退。
- **自动更新**：独立构建不会跟随官方安装包自动更新；升级时请把 `bongocat-xinput.patch` 应用到新版本源码后重新构建。

## 从源码构建

见 [BUILD.md](BUILD.md)。简而言之：

```powershell
cd bongocat_final\source
cargo build --release -p bongocat-app --features production
```

然后把 `target\release\bongocat-app.exe` 放回 `bongocat_final`，并保留同目录的 `resources`。

## 验证

- `cargo test -p bongocat-platform -p bongocat-runtime`
  - platform: 66 passed
  - runtime: 115 passed
  - shared fixtures: 2 passed
- `cargo clippy -p bongocat-platform -p bongocat-runtime --all-targets -- -D warnings` 通过
- release 构建通过
- 实机：Steam 退出/开启时 Xbox Series X 蓝牙手柄均正常，gamepad/standard 不再频繁切换

## 回退

直接删除 `bongocat_final` 即可。根目录的原版 `bongocat-app.exe`、`uninstall.exe` 和 `resources` 没有被修改。
