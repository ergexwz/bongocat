<script setup lang="ts">
/**
 * 邮件列表项（MailItem）：手绘风，视觉对照 mail-list.html。
 *
 * 三种状态（由父传入的 status + meta 决定样式）：
 * - unread：左侧红墨点标记（复用 todo ink-dot 风格的 SVG path）。
 * - read：左侧信封图标 + 弱化色 + meta 显示「已读，N 分钟后归档」。
 * - archived：左侧信封图标（更淡）+ opacity 0.7 + 「已归档」标签 + 绝对日期。
 *
 * 点击触发 action（邮件列表=标已读+跳 webmail；归档=跳 webmail）。
 * 右侧操作按钮（T5a）：归档（仅邮件列表，showArchive 控制）+ 删除（两列表都有）。
 * 删除二次确认 inline 变体：点一次变「确认删除?」红色高亮，700ms 强制反应死区
 * （死区内点击忽略，避免误双击直接删），死区后 3.3s 内再点才删，超时自动恢复。
 * 所有线条走 SVG，严禁 CSS border。
 *
 * @see docs/designs/phase2-exploration/mail-list.html
 */
import { computed, onBeforeUnmount, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import type { MailNotification } from '../../types'

import { minutesUntilArchive } from '../../utils/retention'

const props = defineProps<{
  /** 邮件数据。 */
  mail: MailNotification
  /** 状态覆盖（默认取 mail.status；父可传入 read/archived 强制样式）。 */
  status?: 'unread' | 'read' | 'archived'
  /** meta 区显示的 provider 名（如「Gmail」/「QQ 邮箱」），由父解析。 */
  providerName?: string | null
  /** meta 区显示的相对时间（如「2 分钟前」/「昨天」），由父格式化。 */
  timeLabel?: string
  /** meta 区显示的绝对日期（如「2026.8.3」），两列表都传，与相对时间并列。 */
  dateLabel?: string
  /** 是否显示归档按钮（邮件列表传 true；归档列表不传/传 false）。 */
  showArchive?: boolean
}>()

const emit = defineEmits<{
  /** 点击邮件项（父决定标已读 + 跳 webmail）。 */
  action: [id: string]
  /** 点击归档按钮（父调 mailNotificationStore.archive(id)）。 */
  archive: [id: string]
  /** 点击删除按钮（二次确认后；父调 mailNotificationStore.purge(id)）。 */
  delete: [id: string]
}>()

const { t } = useI18n()

/** 实际展示状态：props.status 优先，否则取 mail.status。 */
const effectiveStatus = computed(() => props.status ?? props.mail.status)

/** 已读邮件剩余归档分钟（基于 readAt + 当前时间）。 */
const minutesLeft = computed(() =>
  props.mail.readAt ? minutesUntilArchive(props.mail.readAt, Date.now()) : 0,
)

// ── 删除二次确认（inline 变体 + 强制反应死区）──
// 死区 700ms：确认态出现后的反应死区，死区内点击忽略（防误双击直删）。
// 之后 3.3s 可确认窗口，超时（共 4s）自动恢复普通态。
const DEADZONE_MS = 700
const CONFIRM_WINDOW_MS = 3300
const confirmingDelete = ref(false)
/** 是否处于反应死区（死区内点击忽略，视觉变灰提示用户「请等一下」）。 */
const inDeadzone = ref(false)
let confirmTimer: ReturnType<typeof setTimeout> | undefined

/** 第一次点删除：进入确认态 + 启动死区，死区结束后进入可确认窗口。 */
function armConfirm(): void {
  confirmingDelete.value = true
  inDeadzone.value = true
  if (confirmTimer) {
    clearTimeout(confirmTimer)
  }
  // 死区结束：可点击确认
  confirmTimer = setTimeout(() => {
    inDeadzone.value = false
    // 再启动可确认窗口计时
    confirmTimer = setTimeout(() => {
      confirmingDelete.value = false
      inDeadzone.value = false
      confirmTimer = undefined
    }, CONFIRM_WINDOW_MS)
  }, DEADZONE_MS)
}

/** 删除按钮点击：死区内忽略；未确认→进入确认；已确认→真删。 */
function onClickDelete(): void {
  if (confirmingDelete.value) {
    if (inDeadzone.value) {
      // 反应死区：忽略点击，防止用户双击直接删除
      return
    }
    if (confirmTimer) {
      clearTimeout(confirmTimer)
      confirmTimer = undefined
    }
    confirmingDelete.value = false
    inDeadzone.value = false
    emit('delete', props.mail.id)
  } else {
    armConfirm()
  }
}

onBeforeUnmount(() => {
  if (confirmTimer) {
    clearTimeout(confirmTimer)
  }
})
</script>

<template>
  <div
    class="mail-item"
    :class="effectiveStatus"
    @click="emit('action', mail.id)"
  >
    <!-- 未读：红墨点（复用 todo ink-dot 的三层 path） -->
    <svg
      v-if="effectiveStatus === 'unread'"
      class="unread-dot"
      viewBox="0 0 14 14"
    >
      <path
        d="M 7 2 Q 11 2 12 5 Q 13 7 12 9 Q 11 12 7 12 Q 3 12 2 9 Q 1 7 2 5 Q 3 2 7 2 Z"
        fill="#d4654a"
        opacity="0.25"
      />
      <path
        d="M 7 3.5 Q 9.5 3.5 10.5 5.5 Q 11 7 10 8.5 Q 9 10.5 7 10.5 Q 5 10.5 4 8.5 Q 3 7 3.5 5.5 Q 4.5 3.5 7 3.5 Z"
        fill="#d4654a"
      />
    </svg>
    <!-- 已读/归档：信封图标 -->
    <div
      v-else
      class="mail-icon"
    >
      <svg
        fill="none"
        height="16"
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        viewBox="0 0 24 24"
        width="16"
      >
        <path d="M 4 5 Q 3 5 3 6 L 3 18 Q 3 19 4 19 L 20 19 Q 21 19 21 18 L 21 6 Q 21 5 20 5 Z" />
        <path d="M 4 6 L 12 12 L 20 6" />
      </svg>
    </div>

    <div class="mail-main">
      <div class="mail-from">
        {{ mail.from || '—' }}
      </div>
      <div class="mail-subject">
        {{ mail.subject || '—' }}
      </div>
      <!-- meta 两列网格：左=provider/已读提示/已归档标签，右=相对+绝对日期 -->
      <div class="mail-meta">
        <div class="meta-left">
          <span v-if="providerName">{{ providerName }}</span>
          <span v-if="effectiveStatus === 'read'">{{ t('plugins.mail.labels.mailReadMinutesLater', { n: minutesLeft }) }}</span>
          <span
            v-else-if="effectiveStatus === 'archived'"
            class="archived-tag"
          >{{ t('plugins.mail.labels.mailArchivedTag') }}</span>
        </div>
        <div
          v-if="timeLabel || dateLabel"
          class="meta-right"
        >
          <span v-if="timeLabel">{{ timeLabel }}</span>
          <span v-if="timeLabel && dateLabel">·</span>
          <span v-if="dateLabel">{{ dateLabel }}</span>
        </div>
      </div>
    </div>

    <!-- 右侧操作按钮（hover 显现；确认删除态/死区强制可见） -->
    <div
      class="mail-actions"
      :class="{ 'is-confirming': confirmingDelete }"
    >
      <!-- 归档按钮：手绘带盖箱子（仅邮件列表，确认删除态隐藏聚焦删除） -->
      <button
        v-if="showArchive && !confirmingDelete"
        class="action-btn"
        :title="t('plugins.mail.labels.mailArchiveAction')"
        type="button"
        @click.stop="emit('archive', mail.id)"
      >
        <!-- 手绘归档箱：盖子波浪线 + 箱体 + 把手槽 -->
        <svg
          fill="none"
          height="20"
          stroke="currentColor"
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="1.8"
          viewBox="0 0 24 24"
          width="20"
        >
          <!-- 盖子：手绘波浪横线 -->
          <path d="M 4 7 Q 8 6 12 7 Q 16 8 20 7" />
          <!-- 箱体：手绘不规则矩形（Q 曲线，非直线） -->
          <path d="M 5 8 Q 4 8 4.5 9.5 L 6 19 Q 6 20.5 7.5 20.5 L 16.5 20.5 Q 18 20.5 18 19 L 19.5 9.5 Q 20 8 19 8" />
          <!-- 把手槽：盖子上的小弧 -->
          <path d="M 10 6 Q 12 4.5 14 6" />
        </svg>
      </button>
      <!-- 删除按钮：普通态=手绘垃圾桶，确认态=「确认删除?」红色文字，死区态=灰色禁用感 -->
      <button
        class="action-btn"
        :class="{ confirming: confirmingDelete && !inDeadzone, deadzone: confirmingDelete && inDeadzone }"
        :disabled="confirmingDelete && inDeadzone"
        :title="t('plugins.mail.labels.mailDeleteAction')"
        type="button"
        @click.stop="onClickDelete"
      >
        <span
          v-if="confirmingDelete"
          class="confirm-text"
        >{{ t('plugins.mail.labels.mailConfirmDelete') }}</span>
        <svg
          v-else
          fill="none"
          height="20"
          stroke="currentColor"
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="1.8"
          viewBox="0 0 24 24"
          width="20"
        >
          <!-- 手绘垃圾桶：桶盖横线 + 把手 + 桶身 + 两条竖纹 -->
          <path d="M 4 7 Q 8 6.5 12 7 Q 16 7.5 20 7" />
          <path d="M 10 5 Q 12 4 14 5" />
          <path d="M 6 8 Q 5.5 8 5.7 9 L 7 19 Q 7.2 20 8.2 20 L 15.8 20 Q 16.8 20 17 19 L 18.3 9 Q 18.5 8 18 8" />
          <path d="M 10 11 Q 9.8 14 9.5 17" />
          <path d="M 14 11 Q 14.2 14 14.5 17" />
        </svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
.mail-item {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 12px 10px;
  border-radius: 14px;
  cursor: pointer;
  transition: background 0.2s;
  position: relative;
}

.mail-item:hover {
  background: rgb(244 168 160 / 8%);
}

.unread-dot {
  width: 10px;
  height: 10px;
  flex-shrink: 0;
  margin-top: 8px;
}

.mail-icon {
  display: flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  margin-top: 2px;
  border-radius: 50%;
  background: rgb(155 196 224 / 20%);
  color: var(--ink-soft);
}

.mail-item.archived .mail-icon {
  background: rgb(196 181 160 / 15%);
  color: var(--ink-faint);
}

.mail-main {
  flex: 1;
  min-width: 0;
}

.mail-from {
  font-size: 15px;
  font-weight: 700;
  line-height: 1.5;
  color: var(--ink);
  word-break: break-word;
}

.mail-subject {
  margin-top: 3px;
  font-size: 13px;
  line-height: 1.5;
  color: var(--ink-soft);
  word-break: break-word;
}

/* meta 两列网格：解决归档列表 provider·已归档·相对·绝对 挤换行 */
.mail-meta {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 4px 10px;
  align-items: center;
  margin-top: 6px;
  font-size: 12px;
  color: var(--ink-faint);
}

.meta-left {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.meta-right {
  text-align: right;
  white-space: nowrap;
}

.mail-item.read .mail-from {
  font-weight: 600;
  color: var(--ink-soft);
}

.mail-item.read .mail-subject {
  color: var(--ink-faint);
}

.mail-item.archived {
  opacity: 0.7;
}

.archived-tag {
  display: inline-block;
  padding: 1px 8px;
  border-radius: 6px;
  font-size: 11px;
  font-weight: 600;
  color: var(--ink-faint);
  background: rgb(196 181 160 / 20%);
}

/* ── 右侧操作按钮区 ── */
.mail-actions {
  display: flex;
  flex-shrink: 0;
  align-items: center;
  gap: 4px;
  margin-top: 4px;
  opacity: 0;
  transition: opacity 0.2s;
}

.mail-item:hover .mail-actions {
  opacity: 1;
}

/* 确认删除态/死区强制可见（即使鼠标移开） */
.mail-actions.is-confirming {
  opacity: 1;
}

.action-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 5px;
  border: none;
  background: none;
  color: var(--ink-faint);
  cursor: pointer;
  font-family: inherit;
  font-size: 12px;
  font-weight: 600;
  line-height: 1;
  border-radius: 8px;
  transition:
    color 0.2s,
    background 0.2s;
}

.action-btn:hover {
  color: var(--ink);
}

.action-btn.confirming {
  padding: 4px 10px;
  color: var(--red-ink);
  background: rgb(212 101 74 / 12%);
}

.action-btn.confirming:hover {
  background: rgb(212 101 74 / 20%);
}

/* 死区态：灰色禁用感，提示用户「请稍等」 */
.action-btn.deadzone {
  padding: 4px 10px;
  color: var(--ink-faint);
  background: rgb(196 181 160 / 15%);
  cursor: not-allowed;
  opacity: 0.6;
}

.confirm-text {
  white-space: nowrap;
}
</style>
