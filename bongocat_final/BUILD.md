# 从源码构建

## 前置条件

- Windows 10 1903+ / Windows 11 x64
- Rust **1.97.1**（`rust-toolchain.toml` 已固定；rustup 会自动安装）
- Visual Studio Build Tools，含 MSVC v143/v145 x64 工具链与 Windows SDK
  - 本机验证环境：Visual Studio Build Tools 18.10、Windows SDK 10.0.26100
- 网络可访问 crates.io 与 `github.com/ayangweb`（Cargo 需要拉取依赖）

源码已经包含：

- `source/crates/`
- `source/vendor/cubism/`（Live2D Cubism Core 静态库，Windows/macOS）
- `source/resources/icons/`（构建时嵌入的产品图标）
- `source/shared/`（测试 fixture）

源码不包含 `target/` 和 `.git/`，首次构建会重新编译全部依赖。

## 构建

```powershell
cd bongocat_final\source

# 可选：缩短首次构建时间；不设置则使用仓库的 fat LTO 配置
$env:CARGO_PROFILE_RELEASE_LTO = "thin"
$env:CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "4"

cargo build --release -p bongocat-app --features production
```

产物：

```text
source\target\release\bongocat-app.exe
```

把它复制到 `bongocat_final\`，覆盖旧的 `bongocat-app.exe`。必须保留同目录的：

```text
bongocat_final\resources\models\
```

程序只从可执行文件同目录的 `resources/models` 加载内置模型。

## 测试与检查

```powershell
cd bongocat_final\source

cargo test -p bongocat-platform -p bongocat-runtime
cargo clippy -p bongocat-platform -p bongocat-runtime --all-targets -- -D warnings
cargo fmt --all -- --check
```

本机结果：

- platform: 66 passed，3 ignored
- runtime: 115 passed
- shared fixtures: 2 passed
- clippy: 无警告
- release 构建成功

## 补丁

源码修改已经包含在 `source/` 中，workspace 版本为 **2.4.1**；相对
upstream v2.4.0 的完整 diff 在：

```text
bongocat-xinput.patch
```

应用到新版本源码时：

```powershell
git apply bongocat-xinput.patch
```

补丁主要修改：

```text
crates/bongocat-platform/Cargo.toml
crates/bongocat-platform/src/gilrs_gamepad.rs
crates/bongocat-platform/src/windows.rs
crates/bongocat-runtime/src/worker/mod.rs
Cargo.lock
```

升级 BongoCat 后如果这些文件变化较大，不要直接强推补丁；按 `BUILD.md` 中的行为说明重新适配：

- Xbox 走 XInput，其他手柄保留 WGI
- `make_code == 0` 的 HID 键盘报告只计数，不触发全量输入重置
- 重复 `GamepadConnected` 不清空已激活连接的轴状态
- XInput 断开宽限 5 秒，并结合 WGI 连接存在性判断

## 运行库

`bongocat_final` 已带应用本地 VC++ 运行库：

```text
vcruntime140.dll
vcruntime140_1.dll
```

它们是 Microsoft Visual C++ Redistributable 的一部分，从本机 VS Build Tools 的 `VC\Redist\MSVC\...\x64\Microsoft.VC145.CRT` 复制。若在其他机器上重新构建，构建机需要相同/兼容的 MSVC 运行库；分发包保留这两个 DLL 即可不依赖系统预装 VC++。

Windows 10 1903+ 自带的 `icuuc.dll`、`d3dcompiler_47.dll` 等系统 DLL 不随包分发。
