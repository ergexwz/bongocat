# BongoCat 插件 SDK

[English](README.md)

插件通过 stdin/stdout 上的长度前缀 JSON 通信，stdout 专用于协议。插件自行持有业务逻辑和数据，宿主的 runtime、GPUI 对象、系统句柄和 Rust ABI 不跨越进程边界。

## v2 协议

实现 `Plugin::handle` 和 `v2::Plugin`，调用 `v2::run`；拥有窗口的随包插件使用 `run_host_v2`。包清单声明 `protocol_version: 2`，需要更新后的宿主。宿主通过独立适配路径继续接受 v1 包。

清单中的显示、输入能力与握手中的协议扩展是两层契约。后续能力优先作为可选的版本化扩展添加，保留已有帧的语义和默认必需能力集合；破坏帧结构时使用新的协商扩展或协议版本。持久状态属于必要功能时，应把 `checkpoint.v2` 设为必需；可选能力必须有可用的关闭行为。

- `capabilities` 声明必需和可选的版本化扩展；缺少必需能力时拒绝启动，`negotiated` 返回实际选中的能力，可据此关闭可选功能。当前扩展为 `regions.v2`、`events.v2`、`schedule.v2`、`checkpoint.v2`、`subscriptions.v2`。
- `handle` 返回完整的强类型 `PluginOutput`，SDK 只发送变化的区域，并附带递增 revision。操作回复、状态快照和事件相互独立，不提供任意 JSON 路径补丁。
  需要具体错误码时可覆写 `handle_v2`，例如返回 `StorageFailure`。未协商的可选扩展不会发送空帧；尝试发布依赖该扩展的数据会返回 `UnsupportedCapability`。
- `subscriptions` 原子替换输入及桌宠上下文订阅，只能缩小清单已经声明的能力范围。
- `next_wake_ms` 申请一次 34 毫秒到 24 小时之间的唤醒，返回 `None` 表示无定时工作。计时器处理本次唤醒后再申请下次；持续输入不会反复推迟未变化的计时截止时间。
- 气泡使用唯一业务事件 ID，最多保留 256 个未确认事件。`event_acknowledged` 表示宿主的有界队列已接收，返回气泡业务 ID，并不表示用户已经阅读。SDK 为传输分配独立的 u64 序号，从 1 连续递增。宿主以恒定内存在整个会话内抑制传输重放；入队失败不推进已接收序号，后续事件等待缺失序号重试。SDK 最近 1024 个业务 ID 的缓存用于避免重复发布未变化气泡，不限制传输重放保护。跨崩溃重试须自行持久化业务 ID，不承诺跨重启的恰好一次执行。
- 先处理 `Restore { schema_version, config, state }`。版本 0 表示旧的 opaque bytes；迁移到插件自己的检查点版本，或者返回错误且不改动数据。宿主在迁移前保留一个有界备份，原子保存新检查点后才调用 `checkpoint_committed`。检查点版本只允许在启动期间变化；失败包替换会一起恢复宿主会话。插件自己的业务文件仍需自行原子写入和迁移。

每插件控制队列最多 64 项、1 MiB，单帧最多 256 KiB；输出通过内容校验不意味着一定满足线上的大小上限。操作可返回 Busy、Timeout、InvalidConfiguration、StorageFailure、UnsupportedCapability 或 ProtocolViolation。除非业务 ID 保证重试安全，否则不要自动重放动作。能力声明属于协议契约，不是操作系统沙箱。

## 可运行示例

新的官方包要求宿主 2.5.4 或更高版本，请在 `app_versions` 填写实际最低版本。
可选扩展 `event_retry.v2` 通过 `EventRejected { id }` 表示宿主事件队列正忙。
运行器保留原载荷和 ID，直到收到确认，并以至少 34 毫秒的间隔重试。
未确认事件同时受 256 项和序列化载荷 1 MiB 的上限约束，收到确认后释放容量。
已有对端继续独立协商原有五项必需扩展。

订阅有序输入的插件通过 `v2::Plugin::reset_input` 在溢出后清除临时按下状态，
保留业务数据。请求预算包含运行器接收队列中的等待时间，超时动作由业务明确决定是否重试。

宿主适配层的 `HostSession` 回复和快照使用 `Arc<PluginOutput>`，可在下一次发布前共享同一份输出。

```text
cargo build --locked -p bongocat-plugin-sdk --examples --release
```

| 示例 | 插件 ID | 用途 |
| --- | --- | --- |
| `event_counter` | `com.example.event-counter` | 事件驱动输出、确认通知、检查点迁移 |
| `scheduled_timer` | `com.example.scheduled-timer` | 三次按需唤醒后进入空闲 |
| `companion_hud` | `com.example.companion-hud` | 宿主 HUD、通知与桌宠可见性订阅 |
| `scaling_fixture` | 使用清单 ID | 独立的双协议性能测试进程 |

将程序和 `plugin.json` 打成 ZIP；声明本地图片图标时再包含 PNG，随附示例使用 emoji 图标。清单 `id` 必须匹配示例，`entry` 是不含 `.exe` 的程序名。计数器声明 `model_ui`、`persistent_state`、`host_bubble_v1`，HUD 还需 `pet_state_v1`；宿主同时校验清单和实际握手。应用版本范围应按实际支持的宿主发行版填写，不能只依赖当前源码检出。

宿主 `just plugin-tests` 会构建这些示例，并以真实 v1/v2 进程验证升级回滚、定时唤醒、检查点和事件交付。

三个示例在 `examples/manifests` 中提供完整清单。在插件工作区运行以下命令，
即可生成可安装 ZIP，无需修改宿主代码：

```text
cargo build --locked -p bongocat-plugin-sdk --examples --release --target x86_64-pc-windows-msvc
python crates/bongocat-plugin-sdk/examples/package_examples.py --bin-dir target/x86_64-pc-windows-msvc/release/examples --output target/sdk-example-packages --target x86_64-pc-windows-msvc
```

输出目录请使用新目录，打包器保留已有包。工具仅使用 Python 标准库，缺少二进制时
直接报错，并输出每个 ZIP 的哈希。伴随示例在桌宠隐藏后恢复时发送一次通知，
通知 ID 在当前会话内唯一。
