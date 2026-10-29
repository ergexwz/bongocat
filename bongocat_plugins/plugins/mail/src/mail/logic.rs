//! 邮件连接管理的纯逻辑（TDD 重点，不依赖网络/keyring/Tauri）。
//!
//! 这些函数决定「何时重置 IDLE / 何时重连 / 重连等多久」，
//! 抽成纯函数后可在 `#[cfg(test)]` 里覆盖边界，实际 IMAP 连接靠 tracer bullet 手动验证。

use std::time::Duration;

/// 不支持 IDLE 的服务器（如 Coremail/论客 老版本）轮询周期。
///
/// IDLE 不可用时降级为定期 FETCH 检查新邮件，5 秒周期与 IDLE 路径的
/// `wait_with_timeout` 兜底周期一致，保证体感延迟可控。
pub const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// IDLE 自动重置阈值。RFC 2177 建议 30 分钟内重置一次，防服务端超时；
/// 取 29 分钟留 1 分钟余量。
///
/// 当前 IDLE 保活由 async-imap `Handle::wait()` 内置 29min 超时实现（见 manager.rs），
/// 本常量 + [`should_reset_idle`] 作为纯逻辑 seam 保留，供 ticket #11 AC 要求的单元测试覆盖，
/// 也为未来需要手动判定保活时机的场景（如非 wait_with_timeout 的自定义循环）预留。
#[allow(dead_code)]
pub const IDLE_RESET_INTERVAL: Duration = Duration::from_secs(29 * 60);

/// IDLE 命令失败的分类（决定降级还是重连）。
///
/// Coremail（论客）等服务器不支持 IMAP IDLE 扩展，对 IDLE 命令返回
/// `BAD command not support`。这种情况下重试 IDLE 永远不会成功，应降级为轮询。
/// 临时错误（网络抖动等）则应重连后重试 IDLE。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleSupport {
    /// 服务器不支持 IDLE（BAD command not support）→ 降级轮询，不再尝试 IDLE。
    Unsupported,
    /// 临时错误（网络抖动等）→ 重连后重试 IDLE。
    Transient,
}

/// 根据错误信息判定是「不支持 IDLE」还是「临时错误」。
///
/// 纯函数，便于单测。错误信息的匹配基于实际观察到的服务器响应：
/// - Coremail: `BAD command not support`（init 返回的 io error message）
/// - 临时错误：网络抖动、连接重置等，不含 "not support" / "unknown command"
pub fn classify_idle_error(err_msg: &str) -> IdleSupport {
    let lower = err_msg.to_lowercase();
    // Coremail: "command not support"
    // 通用 IMAP: "unknown command"（RFC 3501 对未识别命令的标准 BAD 响应）
    if lower.contains("not support") || lower.contains("unknown command") {
        IdleSupport::Unsupported
    } else {
        IdleSupport::Transient
    }
}

/// 断线重连退避上限（指数退避封顶，避免无限增长到离谱的等待时间）。
pub const BACKOFF_MAX: Duration = Duration::from_secs(5 * 60);

/// 判定 IDLE 是否到了该重置的时间点。
///
/// @param elapsed 自上次 IDLE 重置（或启动）以来经过的时间
/// @returns 是否已达到/超过 [`IDLE_RESET_INTERVAL`]
///
/// 纯函数，不读系统时钟，便于单测。当前运行时由 `wait()` 内置超时覆盖，
/// 本函数作为纯逻辑 seam 保留（ticket #11 AC 要求有单元测试）。
#[allow(dead_code)]
pub fn should_reset_idle(elapsed: Duration) -> bool {
    elapsed >= IDLE_RESET_INTERVAL
}

/// 计算第 `retries` 次重连前的等待时间（指数退避，封顶 [`BACKOFF_MAX`]）。
///
/// 策略：初始 2s，每次翻倍（2 → 4 → 8 → 16 → 32 → 64 → 128 → 240=MAX），
/// 到达上限后不再增长。`retries` 从 0 开始（首次重试 = retries=0 → 等 2s）。
///
/// 纯函数，便于单测。
pub fn backoff_delay(retries: u32) -> Duration {
    // 初始 2 秒；每多一次重试翻一倍。用 checked_shl + saturating_mul 防溢出，
    // 最后用 min 封顶到 BACKOFF_MAX。
    let base = Duration::from_secs(2);
    // Duration::saturating_mul 取 u32；retries 很大时 factor 会 saturate 到 u32::MAX
    //（随后被 BACKOFF_MAX 封顶，不会越界）。
    let factor = 1u32.checked_shl(retries).unwrap_or(u32::MAX);
    let delay = base.saturating_mul(factor);
    delay.min(BACKOFF_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod should_reset_idle {
        use super::*;

        #[test]
        fn 零时间不重置() {
            assert!(!should_reset_idle(Duration::ZERO));
        }

        #[test]
        fn 远小于阈值不重置() {
            assert!(!should_reset_idle(Duration::from_secs(60)));
            assert!(!should_reset_idle(Duration::from_secs(10 * 60)));
        }

        #[test]
        fn 接近但未达阈值不重置_28分钟() {
            assert!(!should_reset_idle(Duration::from_secs(28 * 60)));
        }

        #[test]
        fn 恰好达到阈值应重置_29分钟() {
            assert!(should_reset_idle(Duration::from_secs(29 * 60)));
        }

        #[test]
        fn 超过阈值应重置_30分钟() {
            assert!(should_reset_idle(Duration::from_secs(30 * 60)));
            assert!(should_reset_idle(Duration::from_secs(60 * 60)));
        }

        #[test]
        fn 阈值边界_比28分多1秒仍不到29分() {
            assert!(!should_reset_idle(Duration::from_secs(28 * 60 + 59)));
        }
    }

    mod backoff_delay {
        use super::*;

        #[test]
        fn 首次重试_retries0_等2秒() {
            assert_eq!(backoff_delay(0), Duration::from_secs(2));
        }

        #[test]
        fn 逐次翻倍() {
            assert_eq!(backoff_delay(1), Duration::from_secs(4));
            assert_eq!(backoff_delay(2), Duration::from_secs(8));
            assert_eq!(backoff_delay(3), Duration::from_secs(16));
            assert_eq!(backoff_delay(4), Duration::from_secs(32));
            assert_eq!(backoff_delay(5), Duration::from_secs(64));
            assert_eq!(backoff_delay(6), Duration::from_secs(128));
        }

        #[test]
        fn 达到上限后封顶不再增长() {
            // BACKOFF_MAX = 5min = 300s。2 * 2^7 = 256s < 300s（未封顶），
            // 2 * 2^8 = 512s > 300s → retries=8 起封顶到 300s。
            assert_eq!(backoff_delay(8), BACKOFF_MAX);
            assert_eq!(backoff_delay(9), BACKOFF_MAX);
            assert_eq!(backoff_delay(20), BACKOFF_MAX);
            assert_eq!(backoff_delay(100), BACKOFF_MAX);
        }

        #[test]
        fn retries7_未封顶_256秒() {
            // 2 * 2^7 = 256s < 300s(BACKOFF_MAX)，还未到上限
            assert_eq!(backoff_delay(7), Duration::from_secs(256));
        }

        #[test]
        fn 永不返回超过上限的值() {
            for retries in 0..=50 {
                assert!(
                    backoff_delay(retries) <= BACKOFF_MAX,
                    "retries={retries} 超过上限"
                );
            }
        }

        #[test]
        fn 退避序列单调不减() {
            let mut prev = Duration::ZERO;
            for retries in 0..=30 {
                let cur = backoff_delay(retries);
                assert!(cur >= prev, "retries={retries} 退避变小了");
                prev = cur;
            }
        }
    }

    mod classify_idle_error {
        use super::*;

        #[test]
        fn coremail_不支持的响应判定为不支持() {
            // 实际观察到的 Coremail 响应（日志 A0004 BAD command not support）
            assert_eq!(
                classify_idle_error("io: command not support"),
                IdleSupport::Unsupported
            );
            assert_eq!(
                classify_idle_error("BAD command not support"),
                IdleSupport::Unsupported
            );
        }

        #[test]
        fn 通用unknown_command也判定为不支持() {
            // RFC 3501 标准的「未识别命令」响应
            assert_eq!(
                classify_idle_error("BAD unknown command"),
                IdleSupport::Unsupported
            );
            assert_eq!(
                classify_idle_error("BAD Error: unknown command IDLE"),
                IdleSupport::Unsupported
            );
        }

        #[test]
        fn 大小写不敏感() {
            assert_eq!(
                classify_idle_error("COMMAND NOT SUPPORT"),
                IdleSupport::Unsupported
            );
            assert_eq!(
                classify_idle_error("Unknown Command"),
                IdleSupport::Unsupported
            );
        }

        #[test]
        fn 网络抖动判定为临时错误() {
            assert_eq!(
                classify_idle_error("connection reset by peer"),
                IdleSupport::Transient
            );
            assert_eq!(classify_idle_error("broken pipe"), IdleSupport::Transient);
            assert_eq!(classify_idle_error("timed out"), IdleSupport::Transient);
        }

        #[test]
        fn 空字符串判定为临时错误() {
            assert_eq!(classify_idle_error(""), IdleSupport::Transient);
        }
    }
}
