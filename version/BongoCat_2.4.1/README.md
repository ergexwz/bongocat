# BongoCat 2.4.1（精简版）

这是 BongoCat 2.4.1 的本地修复版，基于 upstream 2.4.0 加 Xbox/XInput
后台手柄修复。只包含运行所需文件，不含源码、构建脚本和原版安装包。

## 运行

要求：**Windows 10 1903+ / Windows 11 x64**。

先把这个 `BongoCat_2.4.1` 文件夹复制到本地 Windows 目录（例如
`C:\BongoCat`），然后双击：

```text
run.bat
```

或直接运行：

```text
BongoCat.exe
```

不要只复制 exe；程序需要同目录的 `resources\models`。

## 修复内容

- Xbox 手柄通过 XInput 在后台读取，不再依赖 BongoCat 获得前台焦点。
- PS / Switch / 其他 HID 手柄继续走 WGI。
- Xbox 手柄 HID 键盘集合发送的零扫描码报告不再触发全量输入重置，
  修复 gamepad/standard 模型频繁自动切换。
- 保留 5 秒 XInput 断开宽限，避免空闲蓝牙手柄反复断连。

## 文件

```text
BongoCat.exe
run.bat
vcruntime140.dll
vcruntime140_1.dll
resources\models\standard
resources\models\keyboard
resources\models\gamepad
LICENSE
THIRD_PARTY_NOTICES.txt
licenses\
```

已包含应用本地 VC++ 运行库，不依赖目标机器预装 VC++ Redistributable。

## 注意

- 本地构建未签名，Windows SmartScreen 可能提示；选择“更多信息 → 仍要运行”。
- **按键图**：gamepad 模型缺失/错位的按键图请修改
  `resources\models\gamepad`。本次 release 没有改任何按键图。
- **Steam Input**：如果 Steam 接管了手柄，完全退出 Steam，或在 Steam
  设置 → 控制器中关闭 Xbox 控制器的 Steam 输入。
- **自动切换**：Xbox 手柄关闭后约 5 秒切换到 standard 模型。
- **PS / Switch**：继续使用 WGI 路径，代码未改；本机没有这些手柄，未实机验证。
- **第三方 Xbox 兼容手柄**：WGI 名称不含 `xbox` 且 vendor 不是 `0x045e`
  的手柄仍走 WGI；微软原装 Xbox 手柄走 XInput。
- 首次运行会在 `%APPDATA%\com.ayangweb.bongo-cat\production` 创建配置，
  与原版 BongoCat 共用。不要和原版同时运行，它们共用单实例身份。
- 独立 release 不跟随官方自动更新。
- 源码、补丁和重新构建说明在仓库的 `bongocat_final/` 中，不在本精简包内。
