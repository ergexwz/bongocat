# Windows 安装包与可移植性验收

适用范围：Windows 10/11 x64。本地构建产物：`target/package/BongoCat_2.5.0_x64.exe`，239,372,964 字节（约 228.28 MiB）。此产物尚未做 Authenticode 代码签名。

SHA-256：`1e87fab3f79d712e2f53597d23d2d27b0464ada2c43a84d2caa3f07f06e04d78`。

## 给使用者

1. 运行安装器，选择语言和当前用户可写的安装目录；中文和空格路径可用。
2. 从安装完成页或快捷方式启动。三个内置模型、VC++ 运行库和三个插件随包提供，插件首次安装默认关闭，在插件中心开启。
3. 缺少 WebView2 时，安装器使用随包提供的微软官方离线程序补齐；失败时宿主仍可运行，可在设置中重试插件依赖。
4. 卸载界面提供单独的个人数据删除选项，默认保留数据；共享 WebView2 不随宿主卸载。

配置和插件业务数据存放在当前账号的 `%APPDATA%\com.ayangweb.bongo-cat\production`，与程序安装位置分开。升级时保留已有插件选择和数据。

## 构建与信任

沿用 `just build` 和 cargo-packager 的 NSIS 模板，按用户安装且提供目录选择页。资源从可执行文件所在目录解析；正式构建不会回退读取开发源码目录。

本地修改版插件使用显式 `--bundle-local-plugins` 构建方式：先生成 `plugins.json` 和 ZIP，宿主编译时固定清单 SHA-256，运行时先校验清单，再校验 ZIP 的摘要、身份和路径。在线插件目录仍按原有公钥验签，不信任可被替换的同目录哈希文件，也不更改上游签名公钥。

```powershell
# 先在 companion 工作区构建前端和 cargo release --features custom-protocol，复用原 target。
python tools/prepare-local-companions.py --checkout ../../bongocat_plugins --output target/package/dependencies/plugins --bundled
# 将微软官方 x64 Evergreen Standalone Installer 放入 target/package/dependencies/。
just build --bundle-local-plugins --formats nsis
```

WebView2 使用[微软官方离线部署方式](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution#offline-deployment)。本次下载来源为微软 x64 官方入口 `https://go.microsoft.com/fwlink/?linkid=2124701`，文件为 215,017,168 字节，SHA-256 `62863f0f0611a16ccf124b4f15650c3b66624504eba74d7cde1e2ae5d951249b`，微软 Authenticode 验证通过；VC 运行库也通过微软签名验证。

## 本次验证

- 插件管理器 32 项测试、打包 37 项测试、资源定位 2 项测试通过；受影响模块 clippy 和 35 项打包/平台工具检查通过。
- NSIS 生成安装器，7-Zip 完整性检查通过，共 188 个条目。此环境内 32 位 NSIS 编译器报系统 DLL 重定位错误 623，最终在沙箱外编译 cargo-packager 生成的原始脚本；没有修改或维护另一份 NSIS 模板。
- 解包到中文和空格目录，核对三个模型的全部声明资源，以及随包插件、清单、离线运行库的哈希。
- 从无关工作目录启动解包后的正式宿主，`--help` 返回 0；PE 依赖检查确认随包 VC DLL 和 Windows 系统依赖。
- 使用解包后的真实插件包安装到临时数据目录，确认首次默认关闭，再启动三个真实进程、持续 tick、正常关闭。验收使用 `bundled_distribution` 的显式忽略测试：设置 `BONGOCAT_TEST_BUNDLE` 和构建清单摘要后运行 `cargo test --locked -p bongocat-plugin-manager --test bundled_distribution -- --ignored`。
- 当前账号已有正式数据，没有在现有环境直接安装、升级或卸载，也没有改动其配置、凭证或开机项。

仍需在干净 Windows 10/11 x64 上验证安装向导、选择路径、缺少 WebView2、首次启动完整界面、升级和卸载。解包与进程测试不能替代这些验收，也不代表所有电脑或反作弊系统均允许运行。
