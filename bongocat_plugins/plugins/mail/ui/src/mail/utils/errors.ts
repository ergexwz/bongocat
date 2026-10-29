/**
 * 连接错误分类 + 友好提示生成。
 *
 * Rust 后端返回的错误是技术性字符串（如 `TLS 握手失败: tls handshake eof`），
 * 直接展示给用户不可读。本模块把错误按模式分类，映射成简短的友好提示。
 *
 * ⚠️ 报错框只做错误分类翻译，不写长篇建议。输入时的引导由设置页表单的
 * providerHint（授权码指引区域）在 blur 时实时展示。
 */

/**
 * 生成连接失败的友好提示。
 *
 * @param rawErr Rust 后端返回的原始错误字符串
 * @param t i18n 翻译函数（取 `plugins.mail.labels.errXxx` 文案）
 * @returns 友好提示字符串（已翻译）
 */
export function formatConnectionError(
  rawErr: string,
  t: (key: string) => string,
): string {
  const lower = rawErr.toLowerCase()

  // TLS 握手失败（含 eof / handshake）
  if (lower.includes('tls') || lower.includes('handshake') || lower.includes('eof')) {
    return t('plugins.mail.labels.errTlsBlocked')
  }

  // domain not local（用错 IMAP 服务器）
  // ⚠️ 必须在"登录失败"之前判断——domain not local 错误串也含"登录失败"前缀
  if (lower.includes('domain not local')) {
    return t('plugins.mail.labels.errDomainNotLocal')
  }

  // 登录失败（授权码错误 / 认证失败）
  if (lower.includes('登录失败') || lower.includes('auth') || lower.includes('login failed') || lower.includes('authenticate')) {
    return t('plugins.mail.labels.errAuthFailed')
  }

  // Unsafe Login（网易系邮箱新开 IMAP 的临时拦截）
  if (lower.includes('unsafe login') || lower.includes('command not support')) {
    return t('plugins.mail.labels.errUnsafeLogin')
  }

  // 超时
  if (lower.includes('超时') || lower.includes('timeout')) {
    return t('plugins.mail.labels.errTimeout')
  }

  // TCP / 网络（含"不知道这样的主机"等 DNS 错误）
  if (lower.includes('tcp') || lower.includes('网络') || lower.includes('主机') || lower.includes('host')) {
    return t('plugins.mail.labels.errNetwork')
  }

  // 兜底：无法分类的错误，原样展示（方便排查）
  return rawErr
}
