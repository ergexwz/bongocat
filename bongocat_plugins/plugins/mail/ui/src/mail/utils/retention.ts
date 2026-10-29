/**
 * 邮件留存规则纯函数（T5，D9）。
 *
 * 决策来源 ADR 0002 D9：
 * - 未读邮件默认 **24 小时**后自动归档（转入归档列表）。
 * - 点击邮件标本地已读后，**5 分钟**后自动归档。
 * - 归档超过 **30 天**自动清理（从归档列表移除）。
 *
 * 全部纯函数，无副作用，时间戳由调用方注入（方便测试注入固定时间）。
 *
 * @see docs/adr/0002-phase2-mail-and-bubble.md D9
 */

/** 未读邮件自动归档时限（ms）。24 小时。 */
export const UNREAD_ARCHIVE_AFTER_MS = 24 * 60 * 60 * 1000

/** 已读邮件自动归档时限（ms）。5 分钟。 */
export const READ_ARCHIVE_AFTER_MS = 5 * 60 * 1000

/** 归档邮件自动清理时限（ms）。30 天。 */
export const ARCHIVE_PURGE_AFTER_MS = 30 * 24 * 60 * 60 * 1000

/**
 * 判断一封未读邮件是否已超时需要归档。
 *
 * 规则：到达时间超过 UNREAD_ARCHIVE_AFTER_MS（24h）仍未被读 → 归档。
 *
 * @param arrivedAt 邮件到达时间戳（ms）
 * @param now 当前时间戳（ms），由调用方注入
 */
export function shouldArchiveUnread(arrivedAt: number, now: number): boolean {
  return now - arrivedAt >= UNREAD_ARCHIVE_AFTER_MS
}

/**
 * 判断一封已读邮件是否已超时需要归档。
 *
 * 规则：阅读时间超过 READ_ARCHIVE_AFTER_MS（5min）→ 归档。
 *
 * @param readAt 标记已读的时间戳（ms）
 * @param now 当前时间戳（ms），由调用方注入
 */
export function shouldArchiveRead(readAt: number, now: number): boolean {
  return now - readAt >= READ_ARCHIVE_AFTER_MS
}

/**
 * 判断一封已归档邮件是否需要彻底清理（从列表移除）。
 *
 * 规则：归档时间超过 ARCHIVE_PURGE_AFTER_MS（30 天）→ 清理。
 *
 * @param archivedAt 归档时间戳（ms）
 * @param now 当前时间戳（ms），由调用方注入
 */
export function shouldPurgeArchived(archivedAt: number, now: number): boolean {
  return now - archivedAt >= ARCHIVE_PURGE_AFTER_MS
}

/**
 * 计算「已读 · N 分钟后归档」倒计时剩余分钟数（向下取整，最小 0）。
 *
 * 邮件列表设计稿展示「已读，4 分钟后归档」这类文案，本函数算剩余分钟。
 *
 * @param readAt 标记已读的时间戳（ms）
 * @param now 当前时间戳（ms），由调用方注入
 * @returns 剩余分钟（已到时返回 0）
 */
export function minutesUntilArchive(readAt: number, now: number): number {
  const elapsed = now - readAt
  const remaining = READ_ARCHIVE_AFTER_MS - elapsed
  return Math.max(0, Math.floor(remaining / 60000))
}
