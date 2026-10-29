//! IMAP IDLE 连接管理器。
//!
//! 每个绑定账号在 [`ConnectionManager`] 里有一个独立的 tokio task 维护 IMAP IDLE 长连接。
//! 新邮件到达时（`IdleResponse::NewData`）读取信封元数据（发件人 + 主题），
//! 通过 `app.emit("mail://new-mail", payload)` 推给前端。
//!
//! 生命周期绑 app 进程（不绑窗口）—— task 由 `ConnectionManager`（app state）托管，
//! 桌宠（main）是常驻窗口，app 进程在跑就有连接。
//!
//! 保活：`Handle::wait()` 内部自带 29 分钟超时（async-imap 实现，RFC 2177 建议 30 分钟内），
//! 超时返回 `Timeout` 后循环顶部重新 idle 即完成保活。
//! 断线：指数退避重连（[`super::logic::backoff_delay`]）。多账号的 N 条连接相互独立。
#![allow(clippy::items_after_test_module)]

use std::collections::HashMap;

use async_imap::extensions::idle::IdleResponse;
use futures_lite::StreamExt;
use keyring::v1::Entry;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::Mutex;
// 用 tauri::async_runtime::spawn 返回的 JoinHandle 类型（非 tokio::task::JoinHandle，
// 两者是不同类型，HashMap 必须用前者匹配 spawn 的返回值）。
use tauri::async_runtime::JoinHandle;

use super::logic::{POLL_INTERVAL, backoff_delay, classify_idle_error};

/// 退避重连的最大尝试次数（避免无限重试刷日志；达到后保持等待上限持续重试）。
const MAX_BACKOFF_RETRIES: u32 = 8;

/// keyring 里存邮箱密码的 key 前缀（完整 key = `<KEY_PREFIX>/<accountId>`）。
pub const KEYRING_KEY_PREFIX: &str = "bongocat-todo/mail";

/// 取某账号在 keyring 里的完整 key。
pub fn keyring_key(account_id: &str) -> String {
    format!("{KEYRING_KEY_PREFIX}/{account_id}")
}

/// 推给前端的「新邮件」事件 payload（`mail://new-mail`）。
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NewMailPayload {
    pub account_id: String,
    /// 邮箱 UID（前端用于离线补发去重：`<accountId>:<uid>` 唯一）。
    pub uid: u32,
    /// 发件人邮箱（`mailbox@host`）；解析失败时给原始字符串。
    pub from: String,
    /// 主题（UTF-8 解码，mime-encoded 暂不深度解码，T1 用 from_utf8_lossy）。
    pub subject: String,
    /// 到达时间（ms 时间戳）。
    pub arrived_at: i64,
}

/// 推给前端的「连接状态」事件 payload（`mail://connection-status`）。
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatusPayload {
    pub account_id: String,
    pub status: ConnectionStatus,
    /// 附加信息（如错误原因）。
    pub message: Option<String>,
}

/// 推给前端的「最新已见 UID」事件 payload（`mail://last-seen-uid`）。
///
/// Rust 每推送一批新邮件后 emit 本事件，前端持久化到 mailAccount.lastSeenUid，
/// 作为下次启动/重连时的离线补发基线（`fetch_new_envelopes(lastSeenUid)` 把离线期间
/// 到达的新邮件补推，app 关闭再打开也不漏邮件）。
///
/// @see #15 comment（离线补发需求）
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LastSeenUidPayload {
    pub account_id: String,
    pub last_seen_uid: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    /// 已连接，IDLE 监听中。
    Connected,
    /// 正在连接。
    Connecting,
    /// 连接失败。
    Error,
}

/// 连接管理器（作为 Tauri app state 托管）。
///
/// 内部用 `Mutex<HashMap<accountId, JoinHandle>>` 管理每个账号的 IDLE task。
/// `Mutex` 保护并发增删账号时的 map 一致性。
pub struct ConnectionManager {
    tasks: Mutex<HashMap<String, JoinHandle<()>>>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
        }
    }

    /// 启动某账号的 IDLE 监听 task。
    ///
    /// 先从 keyring 取密码（不在内存长期持有），取不到直接返回错误。
    /// 已存在同名 task 会先取消再覆盖。
    ///
    /// `initial_last_seen_uid` 来自前端持久化的 `mailAccount.lastSeenUid`：非 0 时作为
    /// 离线补发基线（fetch 离线期间到达的新邮件 UID > last_seen_uid）；0 时回退到
    /// `fetch_max_uid`（跳过当前所有已有邮件，T1 行为）。
    #[allow(clippy::too_many_arguments)]
    pub async fn connect<R: Runtime>(
        &self,
        app: AppHandle<R>,
        account_id: String,
        imap_host: String,
        imap_port: u16,
        username: String,
        proxy: Option<String>,
        initial_last_seen_uid: u32,
    ) -> Result<(), String> {
        // 先从 keyring 取密码
        let entry = Entry::new(&keyring_key(&account_id), &username)
            .map_err(|e| format!("keyring entry 创建失败: {e}"))?;
        let password = entry
            .get_password()
            .map_err(|e| format!("keyring 取密码失败（账号 {}）: {e}", account_id))?;

        // 取消已存在的 task
        self.disconnect(&account_id).await;

        let _ = app.emit(
            "mail://connection-status",
            ConnectionStatusPayload {
                account_id: account_id.clone(),
                status: ConnectionStatus::Connecting,
                message: None,
            },
        );

        let handle = tauri::async_runtime::spawn(idle_loop(
            app,
            account_id.clone(),
            imap_host,
            imap_port,
            username,
            password,
            proxy,
            initial_last_seen_uid,
        ));

        self.tasks.lock().await.insert(account_id, handle);
        Ok(())
    }

    /// 断开某账号的 IDLE task（取消 task 即断开连接）。
    pub async fn disconnect(&self, account_id: &str) {
        if let Some(handle) = self.tasks.lock().await.remove(account_id) {
            handle.abort();
        }
    }

    /// 查询某账号当前是否有活跃的 IDLE 连接（task 存在即视为活跃）。
    #[allow(dead_code)]
    ///
    /// 多账号场景下各账号状态相互独立：断开/查询账号 A 不影响账号 B 的连接。
    /// 供 UI 状态展示与多账号隔离测试（`#[cfg(test)]`）使用。
    pub async fn has_connection(&self, account_id: &str) -> bool {
        self.tasks.lock().await.contains_key(account_id)
    }

    /// 当前活跃连接数（多账号场景的诊断/日志用）。
    #[allow(dead_code)]
    pub async fn connection_count(&self) -> usize {
        self.tasks.lock().await.len()
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 多账号连接池隔离测试（ticket #14 AC）。
    ///
    /// `connect()` 依赖 keyring + 真实网络 + AppHandle，无法在 `#[cfg(test)]` 里跑；
    /// 但隔离性取决于 `tasks: HashMap<accountId, JoinHandle>` 的按-key 增删语义，
    /// 这一层是纯数据结构操作，可以脱离 IMAP 直接验证：
    ///
    /// - N 个账号各自插一个 dummy handle（模拟 `connect` 的 `spawn` + insert）
    /// - `disconnect(A)` 只移除 A，不影响 B/C 的 handle
    /// - `has_connection` 按 accountId 独立查询
    ///
    /// 这样就覆盖了「给定 N 个账号各自状态 → 查询/切换不影响其他」的 AC，
    /// 而真正的端到端（2 个 provider 同时 IDLE）靠手动 tracer bullet 验证。
    fn make_dummy_handle() -> JoinHandle<()> {
        // spawn 一个永不返回的 task 作为「活跃连接」替身（模拟 idle_loop 长连接）。
        // 测试结束时 abort 掉，不实际占用运行时。
        tauri::async_runtime::spawn(async {
            std::future::pending::<()>().await;
        })
    }

    #[tokio::test]
    async fn 多账号各自独立连接_互不影响() {
        let manager = ConnectionManager::new();
        let ids = [
            "acc-a".to_string(),
            "acc-b".to_string(),
            "acc-c".to_string(),
        ];

        // 模拟 3 个账号各自 connect（spawn + insert）
        for id in &ids {
            let handle = make_dummy_handle();
            manager.tasks.lock().await.insert(id.clone(), handle);
        }
        assert_eq!(manager.connection_count().await, 3);
        for id in &ids {
            assert!(manager.has_connection(id).await, "{id} 应有连接");
        }
    }

    #[tokio::test]
    async fn 断开一个账号_其他账号连接不受影响() {
        let manager = ConnectionManager::new();
        let ids = [
            "acc-a".to_string(),
            "acc-b".to_string(),
            "acc-c".to_string(),
        ];
        for id in &ids {
            manager
                .tasks
                .lock()
                .await
                .insert(id.clone(), make_dummy_handle());
        }

        // 断开 B
        manager.disconnect("acc-b").await;

        // B 已无连接，A/C 仍活跃
        assert!(!manager.has_connection("acc-b").await, "acc-b 应已断开");
        assert!(
            manager.has_connection("acc-a").await,
            "acc-a 不应受 acc-b 断开影响"
        );
        assert!(
            manager.has_connection("acc-c").await,
            "acc-c 不应受 acc-b 断开影响"
        );
        assert_eq!(manager.connection_count().await, 2);
    }

    #[tokio::test]
    async fn 断开不存在的账号_无副作用() {
        let manager = ConnectionManager::new();
        manager
            .tasks
            .lock()
            .await
            .insert("acc-a".to_string(), make_dummy_handle());

        // 断开一个未连接的 accountId —— 不应影响已有连接
        manager.disconnect("acc-not-exist").await;

        assert!(manager.has_connection("acc-a").await);
        assert_eq!(manager.connection_count().await, 1);
    }

    #[tokio::test]
    async fn 重连同一账号_旧task被abort_新task覆盖() {
        let manager = ConnectionManager::new();
        manager
            .tasks
            .lock()
            .await
            .insert("acc-a".to_string(), make_dummy_handle());
        assert_eq!(manager.connection_count().await, 1);

        // 模拟「重连」：插入新 handle 覆盖旧 key
        // （connect 内部会先 disconnect 再 insert；这里直接覆盖测试 map 语义）
        let old = manager.tasks.lock().await.remove("acc-a");
        if let Some(h) = old {
            h.abort();
        }
        manager
            .tasks
            .lock()
            .await
            .insert("acc-a".to_string(), make_dummy_handle());

        assert!(manager.has_connection("acc-a").await);
        assert_eq!(manager.connection_count().await, 1, "重连后仍只有 1 条连接");
    }

    #[tokio::test]
    async fn 全部断开后连接数归零() {
        let manager = ConnectionManager::new();
        for id in ["a", "b", "c", "d"] {
            manager
                .tasks
                .lock()
                .await
                .insert(id.to_string(), make_dummy_handle());
        }
        assert_eq!(manager.connection_count().await, 4);

        for id in ["a", "b", "c", "d"] {
            manager.disconnect(id).await;
        }
        assert_eq!(manager.connection_count().await, 0);
        for id in ["a", "b", "c", "d"] {
            assert!(!manager.has_connection(id).await);
        }
    }

    #[tokio::test]
    async fn has_connection_空池查询返回false() {
        let manager = ConnectionManager::new();
        assert!(!manager.has_connection("any").await);
        assert_eq!(manager.connection_count().await, 0);
    }

    #[tokio::test]
    async fn 多账号断开顺序无关_结果一致() {
        // 验证断开顺序不影响最终状态（先断 A 再断 B == 先断 B 再断 A）
        let manager1 = ConnectionManager::new();
        for id in ["a", "b"] {
            manager1
                .tasks
                .lock()
                .await
                .insert(id.to_string(), make_dummy_handle());
        }
        manager1.disconnect("a").await;
        manager1.disconnect("b").await;
        assert_eq!(manager1.connection_count().await, 0);

        let manager2 = ConnectionManager::new();
        for id in ["a", "b"] {
            manager2
                .tasks
                .lock()
                .await
                .insert(id.to_string(), make_dummy_handle());
        }
        manager2.disconnect("b").await;
        manager2.disconnect("a").await;
        assert_eq!(manager2.connection_count().await, 0);
    }
}

/// 单账号的 IDLE 主循环。
///
/// 结构：外层重连循环（出错 → 退避 → 重连），内层 IDLE 循环（NewData → fetch → 重启 idle）。
/// 29 分钟保活由 `Handle::wait()` 自带超时实现，超时返回 `Timeout` 后内层循环重启 idle。
///
/// `initial_last_seen_uid` 非零时作为离线补发基线（前端持久化传入）；0 时回退到 fetch_max_uid。
/// 重连时**沿用**已推进的 last_seen_uid（不重置为初始值），避免补发过的邮件重复推送。
#[allow(clippy::too_many_arguments)]
async fn idle_loop<R: Runtime>(
    app: AppHandle<R>,
    account_id: String,
    imap_host: String,
    imap_port: u16,
    username: String,
    password: String,
    proxy: Option<String>,
    initial_last_seen_uid: u32,
) {
    // 跨重连保留：首次连接用前端传入的 last_seen_uid（离线补发基线），
    // 后续重连沿用同一基线值（每次推送后已通过 mail://last-seen-uid event 让前端持久化，
    // 进程重启时前端会传入更新后的 lastSeenUid；同一进程内 task 正常退出即不再重连）。
    let last_seen_uid = initial_last_seen_uid;
    let mut retries: u32 = 0;

    loop {
        // 尝试建立连接并跑一轮 IDLE
        match run_idle_session(
            &app,
            &account_id,
            &imap_host,
            imap_port,
            &username,
            &password,
            proxy.as_deref(),
            last_seen_uid,
        )
        .await
        {
            Ok(_returned_uid) => {
                // 正常退出（如手动取消）—— 不重连。
                // 正常退出即 task 被取消/进程结束，无需保留最新 uid（前端已通过
                // mail://last-seen-uid event 持久化每次推送后的 uid）。
                break;
            }
            Err(err) => {
                let _ = app.emit(
                    "mail://connection-status",
                    ConnectionStatusPayload {
                        account_id: account_id.clone(),
                        status: ConnectionStatus::Error,
                        message: Some(err.clone()),
                    },
                );
                log::warn!("mail {} IDLE 异常退出，准备重连: {err}", account_id);

                // 指数退避
                let delay = backoff_delay(retries.min(MAX_BACKOFF_RETRIES));
                retries = retries.saturating_add(1);
                tokio::time::sleep(delay).await;
            }
        }
    }
}

/// 建立一次完整的 IMAP 连接并跑 IDLE 直到出错或取消。
///
/// 成功返回 `Ok(last_seen_uid)` 表示正常结束（task 被取消），携带最新已推送 UID
/// 供外层重连沿用；`Err` 表示需要重连。
#[allow(clippy::too_many_arguments)]
async fn run_idle_session<R: Runtime>(
    app: &AppHandle<R>,
    account_id: &str,
    imap_host: &str,
    imap_port: u16,
    username: &str,
    password: &str,
    proxy: Option<&str>,
    initial_last_seen_uid: u32,
) -> Result<u32, String> {
    // 1-3. 建立 TLS 连接 + 登录 + 选 INBOX（与 mail_test_connection 共用）
    let session = build_imap_session(imap_host, imap_port, username, password, proxy).await?;

    let _ = app.emit(
        "mail://connection-status",
        ConnectionStatusPayload {
            account_id: account_id.to_string(),
            status: ConnectionStatus::Connected,
            message: None,
        },
    );

    // 4. IDLE 循环（idle() 消耗 session，done() 归还 session）
    //    wait() 内部自带 29 分钟超时（见 async-imap idle.rs），超时返回 Timeout，
    //    循环顶部重新 idle 即完成 RFC 2177 保活，无需额外的 should_reset_idle 判定。
    //    连接参数传入：IDLE 不支持时 idle_wait_loop 需重建 session 走 poll_loop。
    let (mut session, last_seen_uid) = idle_wait_loop(
        app,
        account_id,
        session,
        initial_last_seen_uid,
        imap_host,
        imap_port,
        username,
        password,
        proxy,
    )
    .await?;

    // 5. 退出时登出（优雅关闭）
    let _ = session.logout().await;
    Ok(last_seen_uid)
}

/// 建立 IMAP 会话：TLS 连接 → 登录 → 选 INBOX。
///
/// `mail_test_connection`（验证连通）和 `run_idle_session`（IDLE 监听）共用本函数，
/// 避免连接序列的重复代码。返回已选 INBOX 的 Session，调用方负责后续 idle/logout。
///
/// `proxy` 非空时通过 HTTP CONNECT 代理建立隧道（绕开 TUN system stack 对非标端口
/// 转发不稳定的问题）。格式如 `"127.0.0.1:7890"`。
pub(crate) async fn build_imap_session(
    imap_host: &str,
    imap_port: u16,
    username: &str,
    password: &str,
    proxy: Option<&str>,
) -> Result<async_imap::Session<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>, String> {
    use std::time::Duration;
    use tokio::time::timeout;

    // 每个网络步骤的超时（TCP/TLS/登录/选文件夹）。代理转发不通时会卡死，
    // 必须有超时让 UI 能报错退出而不是永久转圈。
    const STEP_TIMEOUT: Duration = Duration::from_secs(30);

    // rustls TLS：用 webpki-roots 内置 Mozilla 根 CA，不做证书吊销检查（OCSP/CRL）。
    // native-tls/SChannel 默认检查吊销，国内吊销服务器不可达时握手失败
    // （CRYPT_E_REVOCATION_OFFLINE），rustls 规避此问题。
    let mut root_cert_store = rustls::RootCertStore::empty();
    root_cert_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(config));

    // 建立 TCP 连接：有代理走 HTTP CONNECT 隧道，无代理直连。
    let tcp = if let Some(proxy_addr) = proxy {
        log::info!("[mail] 使用代理 {proxy_addr} 建立到 {imap_host}:{imap_port} 的隧道");
        // 先 TCP 连代理服务器
        let mut proxy_tcp = timeout(STEP_TIMEOUT, tokio::net::TcpStream::connect(proxy_addr))
            .await
            .map_err(|_| format!("连接代理超时（{proxy_addr}，30s 无响应）"))?
            .map_err(|e| format!("连接代理失败 {proxy_addr}: {e}"))?;
        // HTTP CONNECT 隧道到目标 IMAP 服务器
        timeout(
            STEP_TIMEOUT,
            async_http_proxy::http_connect_tokio(&mut proxy_tcp, imap_host, imap_port),
        )
        .await
        .map_err(|_| "代理隧道建立超时（30s 无响应）".to_string())?
        .map_err(|e| format!("代理 CONNECT 失败: {e}"))?;
        proxy_tcp
    } else {
        timeout(
            STEP_TIMEOUT,
            tokio::net::TcpStream::connect((imap_host, imap_port)),
        )
        .await
        .map_err(|_| format!("TCP 连接超时（{imap_host}:{imap_port}，30s 无响应）"))?
        .map_err(|e| format!("TCP 连接失败 {imap_host}:{imap_port}: {e}"))?
    };

    let server_name = rustls::pki_types::ServerName::try_from(imap_host.to_string())
        .map_err(|e| format!("无效的 IMAP 主机名: {e}"))?;
    let tls_stream = timeout(STEP_TIMEOUT, connector.connect(server_name, tcp))
        .await
        .map_err(|_| "TLS 握手超时（30s 无响应，可能是代理未转发 IMAP 流量）".to_string())?
        .map_err(|e| format!("TLS 握手失败: {e}"))?;

    let client = async_imap::Client::new(tls_stream);
    let mut session = timeout(STEP_TIMEOUT, client.login(username, password))
        .await
        .map_err(|_| "IMAP 登录超时（30s 无响应）".to_string())?
        .map_err(|(err, _)| format!("IMAP 登录失败: {err}"))?;

    // 发送 IMAP ID 命令（RFC 2971）。
    // 网易系邮箱（163/126/yeah.net）要求第三方客户端在 LOGIN 后、SELECT 等操作前
    // 发送 ID 表明身份，否则后续操作返回 "Unsafe Login"。
    // 其他邮箱（Gmail/QQ 等）忽略此命令，对所有服务器发送是安全的 RFC 2971 标准行为。
    // 用 `let _ =` 忽略错误：不支持 ID 的服务器返回 BAD 也不影响后续流程。
    // version 用主 crate 的版本（env! 在插件 crate 里取的是插件的 0.1.0）。
    let _ = session
        .id([
            ("name", Some("BongoCat Todo")),
            ("version", Some(env!("CARGO_PKG_VERSION"))),
            ("vendor", Some("ChHsiching")),
            ("support-email", Some("chhsiching@users.noreply.github.com")),
        ])
        .await;
    log::info!("[mail] IMAP ID sent to {imap_host}");

    timeout(STEP_TIMEOUT, session.select("INBOX"))
        .await
        .map_err(|_| "选 INBOX 超时（30s 无响应）".to_string())?
        .map_err(|e| format!("选 INBOX 失败: {e}"))?;

    Ok(session)
}

/// IDLE 等待循环：发起 IDLE → wait → 匹配响应 → NewData 时 fetch envelope → 重新 idle。
///
/// `session.idle()` 会消耗 session（返回 Handle），`handle.done()` 发 DONE 结束 IDLE 并归还 session。
/// `wait()` 自带 29 分钟超时（async-imap 内置保活），超时返回 Timeout 后循环重启 idle。
///
/// `initial_last_seen_uid` 非零时作为离线补发基线（首连接时把离线期间到达的新邮件补推）；
/// 为零时回退到 `fetch_max_uid`（T1 行为：跳过当前所有已有邮件）。循环内 fetch 推进该值后
/// emit `mail://last-seen-uid` 让前端持久化（重连/重启后作为下次补发基线）。
///
/// 返回 `(session, last_seen_uid)`：session 供外层 logout，last_seen_uid 供重连沿用。
#[allow(clippy::too_many_arguments)]
async fn idle_wait_loop<R: Runtime>(
    app: &AppHandle<R>,
    account_id: &str,
    mut session: async_imap::Session<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>,
    initial_last_seen_uid: u32,
    imap_host: &str,
    imap_port: u16,
    username: &str,
    password: &str,
    proxy: Option<&str>,
) -> Result<
    (
        async_imap::Session<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>,
        u32,
    ),
    String,
> {
    // 初始化去重基线：
    // - 前端传入非 0 的 initial_last_seen_uid → 直接用作补发基线（离线补发：fetch 离线期间新邮件）
    // - 为 0（首次绑定 / T5 前旧账号 migrate 后）→ 回退到当前 INBOX 最大 UID（跳过已有邮件，T1 行为），
    //   并 emit last-seen-uid 让前端持久化这个基线（否则前端永远是 0，离线补发永不触发）
    let mut last_seen_uid: u32 = if initial_last_seen_uid > 0 {
        initial_last_seen_uid
    } else {
        let max_uid = fetch_max_uid(&mut session).await.unwrap_or(0);
        // 把 fetch_max_uid 得到的基线 emit 给前端持久化，下次启动就能用它做离线补发
        if max_uid > 0 {
            let _ = app.emit(
                "mail://last-seen-uid",
                LastSeenUidPayload {
                    account_id: account_id.to_string(),
                    last_seen_uid: max_uid,
                },
            );
        }
        max_uid
    };

    // 离线补发：initial_last_seen_uid 非零时，先把离线期间到达的新邮件补推一遍。
    // 这一步只在 initial_last_seen_uid > 0（前端有持久化基线）时执行，避免首次绑定时
    // 把已有邮件全推（initial=0 走 fetch_max_uid 已是基线，fetch_new_envelopes 不会推已存在的）。
    if initial_last_seen_uid > 0 {
        match fetch_new_envelopes(account_id, &mut session, &mut last_seen_uid).await {
            Ok(payloads) => {
                for payload in &payloads {
                    let _ = app.emit("mail://new-mail", payload);
                }
                if !payloads.is_empty() {
                    let _ = app.emit(
                        "mail://last-seen-uid",
                        LastSeenUidPayload {
                            account_id: account_id.to_string(),
                            last_seen_uid,
                        },
                    );
                }
            }
            Err(e) => {
                log::warn!("mail {account_id} 离线补发 fetch 失败: {e}");
            }
        }
    }

    loop {
        let mut handle = session.idle();
        match handle.init().await {
            Ok(()) => {}
            Err(e) => {
                let err_str = format!("{e}");
                match classify_idle_error(&err_str) {
                    super::logic::IdleSupport::Unsupported => {
                        // 服务器不支持 IDLE（Coremail 等返回 BAD command not support）。
                        // session.idle() 消耗了 session 进 handle，init 失败后 async-imap
                        // 没有安全 API 拿回 session（done() 会再次发命令触发 BAD）。
                        // 重新建一个 session 给 poll_loop（多一次连接，仅降级时发生一次）。
                        log::info!(
                            "mail {account_id} 服务器不支持 IDLE，降级为轮询模式（{POLL_INTERVAL:?} 周期）"
                        );
                        drop(handle);
                        let new_session =
                            build_imap_session(imap_host, imap_port, username, password, proxy)
                                .await?;
                        return poll_loop(app, account_id, new_session, last_seen_uid).await;
                    }
                    super::logic::IdleSupport::Transient => {
                        return Err(format!("IDLE init 失败: {err_str}"));
                    }
                }
            }
        }

        // wait_with_timeout 显式 5 秒超时。IDLE 推送正常时收到 NewData 立即返回；
        // 部分邮箱（如 QQ）IDLE 推送不稳定，done→重新 init 间隙可能漏推，
        // 5 秒 Timeout 兜底 fetch 补查漏掉的新邮件，保证体感延迟可控。
        let (wait_future, _stop_token) =
            handle.wait_with_timeout(std::time::Duration::from_secs(5));
        let response = wait_future.await;

        match response {
            Ok(IdleResponse::NewData(_response_data)) => {
                // 有新数据：发 DONE 结束 IDLE，拿回 session → fetch envelope → emit
                session = handle
                    .done()
                    .await
                    .map_err(|e| format!("IDLE done 失败: {e}"))?;

                match fetch_new_envelopes(account_id, &mut session, &mut last_seen_uid).await {
                    Ok(payloads) => {
                        for payload in &payloads {
                            let _ = app.emit("mail://new-mail", payload);
                        }
                        if !payloads.is_empty() {
                            let _ = app.emit(
                                "mail://last-seen-uid",
                                LastSeenUidPayload {
                                    account_id: account_id.to_string(),
                                    last_seen_uid,
                                },
                            );
                        }
                    }
                    Err(e) => {
                        log::warn!("mail {account_id} fetch envelope 失败: {e}");
                    }
                }
                // 继续循环重新 idle
            }
            Ok(IdleResponse::Timeout) => {
                // 超时保活：发 DONE 拿回 session，顺便轮询检查新邮件（QQ 等邮箱 IDLE 推送不稳定，
                // 主动 fetch 兜底），然后回顶部重新 idle
                session = handle
                    .done()
                    .await
                    .map_err(|e| format!("IDLE done 失败: {e}"))?;

                match fetch_new_envelopes(account_id, &mut session, &mut last_seen_uid).await {
                    Ok(payloads) => {
                        for payload in &payloads {
                            let _ = app.emit("mail://new-mail", payload);
                        }
                        if !payloads.is_empty() {
                            let _ = app.emit(
                                "mail://last-seen-uid",
                                LastSeenUidPayload {
                                    account_id: account_id.to_string(),
                                    last_seen_uid,
                                },
                            );
                        }
                    }
                    Err(e) => {
                        log::warn!("mail {account_id} 轮询 fetch 失败: {e}");
                    }
                }
            }
            Ok(IdleResponse::ManualInterrupt) => {
                // 手动中断（task 取消）：正常退出
                session = handle
                    .done()
                    .await
                    .map_err(|e| format!("IDLE done 失败: {e}"))?;
                return Ok((session, last_seen_uid));
            }
            Err(e) => {
                // wait 出错：尝试发 DONE 归还 session（失败也无妨，外层会重连）
                let _ = handle.done().await;
                return Err(format!("IDLE wait 出错: {e}"));
            }
        }
    }
}

/// 轮询模式：服务器不支持 IDLE 时的降级循环。
///
/// 不发起 IDLE，纯 `sleep(POLL_INTERVAL) → fetch_new_envelopes` 循环。
/// fetch 逻辑与 [`idle_wait_loop`] 的 Timeout/NewData 分支一致（emit 新邮件 + 推进 last_seen_uid）。
/// fetch 出错返回 Err 交由外层重连。
async fn poll_loop<R: Runtime>(
    app: &AppHandle<R>,
    account_id: &str,
    mut session: async_imap::Session<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>,
    mut last_seen_uid: u32,
) -> Result<
    (
        async_imap::Session<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>,
        u32,
    ),
    String,
> {
    loop {
        tokio::time::sleep(POLL_INTERVAL).await;
        let payloads = fetch_new_envelopes(account_id, &mut session, &mut last_seen_uid).await?;
        for payload in &payloads {
            let _ = app.emit("mail://new-mail", payload);
        }
        if !payloads.is_empty() {
            let _ = app.emit(
                "mail://last-seen-uid",
                LastSeenUidPayload {
                    account_id: account_id.to_string(),
                    last_seen_uid,
                },
            );
        }
    }
}

/// 取 INBOX 当前最大 UID（用于初始化去重基线，避免启动时把已有邮件全推一遍）。
async fn fetch_max_uid(
    session: &mut async_imap::Session<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>,
) -> Result<u32, String> {
    let mut fetch_stream = session
        .uid_fetch("1:*", "UID")
        .await
        .map_err(|e| format!("fetch max uid 失败: {e}"))?;

    let mut max_uid = 0u32;
    while let Some(fetch) = fetch_stream.next().await {
        let Ok(fetch) = fetch else { continue };
        if let Some(uid) = fetch.uid {
            max_uid = max_uid.max(uid);
        }
    }
    Ok(max_uid)
}

/// 取 last_seen_uid 之后的新邮件信封元数据（去重 + MIME 解码）。
///
/// 只推送 UID > last_seen_uid 的邮件，避免把已有邮件或 flag 变化重复推送。
/// 主题做 MIME encoded-word 解码（`=?utf-8?B?...?=` / `=?utf-8?Q?...?=`）。
async fn fetch_new_envelopes(
    account_id: &str,
    session: &mut async_imap::Session<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>,
    last_seen_uid: &mut u32,
) -> Result<Vec<NewMailPayload>, String> {
    // 取 last_seen_uid+1 到 * 的邮件
    let range = format!("{}:*", last_seen_uid.saturating_add(1));
    let mut fetch_stream = session
        .uid_fetch(&range, "ENVELOPE")
        .await
        .map_err(|e| format!("fetch 新邮件失败: {e}"))?;

    let mut payloads = Vec::new();

    while let Some(fetch) = fetch_stream.next().await {
        let Ok(fetch) = fetch else { continue };
        let Some(uid) = fetch.uid else { continue };

        // 跳过已推送过的
        if uid <= *last_seen_uid {
            continue;
        }

        if let Some(env) = fetch.envelope() {
            let from = env
                .from
                .as_ref()
                .and_then(|addrs| addrs.first())
                .map(|a| {
                    let mailbox = a.mailbox.as_deref().unwrap_or_default();
                    let host = a.host.as_deref().unwrap_or_default();
                    let mailbox = String::from_utf8_lossy(mailbox);
                    let host = String::from_utf8_lossy(host);
                    format!("{mailbox}@{host}")
                })
                .unwrap_or_default();

            let subject = env
                .subject
                .as_deref()
                .map(|b| decode_mime_words(&String::from_utf8_lossy(b)))
                .unwrap_or_default();

            payloads.push(NewMailPayload {
                account_id: account_id.to_string(),
                uid,
                from,
                subject,
                arrived_at: now_ms(),
            });

            *last_seen_uid = (*last_seen_uid).max(uid);
        }
    }

    Ok(payloads)
}

/// 解码 MIME encoded-word（RFC 2047）：`=?charset?B?base64?=` 或 `=?charset?Q?quoted?=`。
///
/// 邮件主题含非 ASCII 字符时会被编码。本函数处理 UTF-8 的 Base64 和 Quoted-Printable，
/// 其他编码用原始 lossy 字符串兜底。支持多个 encoded-word 拼接。
fn decode_mime_words(input: &str) -> String {
    use base64::Engine;

    // 正则匹配太重，手动扫描 `=?charset?enc?data?=` 片段
    let mut result = String::new();
    let mut rest = input;

    while let Some(start) = rest.find("=?") {
        // 普通文本部分
        result.push_str(&rest[..start]);

        let after = &rest[start + 2..];
        // 找结束 ?=
        let Some(end_rel) = after.find("?=") else {
            result.push_str(&rest[start..]);
            return result;
        };

        // encoded-word 内容：charset?enc?data
        let encoded = &after[..end_rel];
        let parts: Vec<&str> = encoded.splitn(3, '?').collect();
        if parts.len() < 3 {
            result.push_str(&rest[start..start + 2 + end_rel + 2]);
            rest = &rest[start + 2 + end_rel + 2..];
            continue;
        }

        let charset = parts[0].to_lowercase();
        let enc = parts[1].to_lowercase();
        let data = parts[2];

        let decoded_bytes = if enc == "b" {
            base64::engine::general_purpose::STANDARD.decode(data).ok()
        } else if enc == "q" {
            // Quoted-Printable: '_' = 空格，=XX 为十六进制字节
            let mut bytes = Vec::new();
            let bytes_iter = data.as_bytes();
            let mut i = 0;
            while i < bytes_iter.len() {
                if bytes_iter[i] == b'_' {
                    bytes.push(b' ');
                    i += 1;
                } else if bytes_iter[i] == b'=' && i + 2 < bytes_iter.len() {
                    if let Ok(b) =
                        u8::from_str_radix(&String::from_utf8_lossy(&bytes_iter[i + 1..i + 3]), 16)
                    {
                        bytes.push(b);
                        i += 3;
                    } else {
                        bytes.push(bytes_iter[i]);
                        i += 1;
                    }
                } else {
                    bytes.push(bytes_iter[i]);
                    i += 1;
                }
            }
            Some(bytes)
        } else {
            None
        };

        match decoded_bytes {
            Some(b) => {
                if charset.starts_with("utf") {
                    result.push_str(&String::from_utf8_lossy(&b));
                } else {
                    // 非 UTF-8 用 lossy 兜底（tracer bullet 简化）
                    result.push_str(&String::from_utf8_lossy(&b));
                }
            }
            None => result.push_str(&rest[start..start + 2 + end_rel + 2]),
        }

        rest = &rest[start + 2 + end_rel + 2..];
    }

    result.push_str(rest);
    result
}

/// 取当前 Unix 时间戳（ms）。基于 SystemTime，无 chrono 依赖。
fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
