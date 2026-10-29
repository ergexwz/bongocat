<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import type { Todo, TodoPriority } from '@/plugins/todo'

import { nextPriority } from '@/plugins/todo/utils/priority'

import HandCheckbox from '../HandCheckbox/index.vue'
import HandClock from '../HandClock/index.vue'
import InkDot from '../InkDot/index.vue'

/**
 * Todo 项（TodoItem）：组合 HandCheckbox + InkDot + HandClock + 标题/meta。
 * - 优先级墨点：按 todo.priority 渲染红/橙/蓝；T6 起点击墨点循环切换优先级（low→med→high→low）。
 * - 到期文案：T6 起相对表述 + 具体时分并存（如「明天 14:30」「今天 09:00」「已逾期 7/28 14:30」）。
 * - 到期视觉：有 dueDate 且（今天/逾期）时 HandClock urgent 态（红描边）+ 红字。
 * - emit toggle/remove/changePriority 给父组件操作 store。
 */
const { todo } = defineProps<{
  todo: Todo
}>()

const emit = defineEmits<{
  changePriority: [id: string, priority: TodoPriority]
  remove: [id: string]
  toggle: [id: string]
}>()

const { t } = useI18n()

const priorityLabel = computed(() => {
  switch (todo.priority) {
    case 'high':
      return t('plugins.todo.labels.priorityHigh')
    case 'low':
      return t('plugins.todo.labels.priorityLow')
    case 'medium':
    default:
      return t('plugins.todo.labels.priorityMedium')
  }
})

/** 两位补零。 */
function pad2(n: number): string {
  return String(n).padStart(2, '0')
}

/**
 * 到期文案（T6）：相对表述 + 具体时分并存。
 * - 今天未到期：`今天 14:30`
 * - 今天已逾期：`已逾期 14:30`（时点已过，精确到分钟）
 * - 明天：`明天 14:30`
 * - 往日逾期：`已逾期 7/28 14:30`
 * - 更远：`7/30 14:30`（相对词不够用时回退到月/日 + 时分）
 *
 * 逾期判定精确到分钟（`dueDate < now`），不再按整天比较——避免「今天 10:15 到期、
 * 现在 10:20 仍显示蓝色「今天」」的体感问题。
 *
 * 所有 dueDate 都带时分（MiniInput 构造的 timestamp 精确到分钟），故一律显示 HH:mm。
 */
const dueLabel = computed<null | string>(() => {
  if (!todo.dueDate)
    return null

  const due = new Date(todo.dueDate)
  const hm = `${pad2(due.getHours())}:${pad2(due.getMinutes())}`
  const md = `${due.getMonth() + 1}/${due.getDate()}`

  const now = Date.now()
  const overdue = todo.dueDate < now

  // 逾期优先显示「已逾期」：今天过时点也显示「已逾期 HH:mm」（不带月日，今天不需要）
  if (overdue) {
    const today = startOfDay(now)
    const dueDay = startOfDay(todo.dueDate)
    return dueDay < today
      ? `${t('plugins.todo.labels.dueOverdueShort')} ${md} ${hm}`
      : `${t('plugins.todo.labels.dueOverdueShort')} ${hm}`
  }

  const today = startOfDay(now)
  const dueDay = startOfDay(todo.dueDate)
  const diffDays = Math.round((dueDay - today) / 86400000)

  if (diffDays === 0)
    return `${t('plugins.todo.labels.dueTodayShort')} ${hm}`
  if (diffDays === 1)
    return `${t('plugins.todo.labels.dueTomorrow')} ${hm}`
  return `${md} ${hm}`
})

/** 到期需高亮（今天或逾期）。 */
const isUrgent = computed(() => {
  if (!todo.dueDate)
    return false
  return todo.dueDate <= Date.now()
})

/** 已逾期（时点已过，精确到分钟），用红色警告；未到时点用蓝色提示。 */
const isOverdue = computed(() => {
  if (!todo.dueDate)
    return false
  return todo.dueDate < Date.now()
})

/** 点击墨点循环切换优先级。 */
function cyclePriority() {
  emit('changePriority', todo.id, nextPriority(todo.priority))
}

function startOfDay(ts: number): number {
  const d = new Date(ts)
  d.setHours(0, 0, 0, 0)
  return d.getTime()
}
</script>

<template>
  <div
    class="todo-item"
    :class="{ done: todo.completed }"
  >
    <div
      class="checkbox-wrap"
      @click="emit('toggle', todo.id)"
    >
      <HandCheckbox :checked="todo.completed" />
    </div>

    <div class="todo-main">
      <div class="todo-title">
        {{ todo.title }}
      </div>

      <div class="todo-meta">
        <span
          class="pri"
          :class="todo.priority"
        >
          <InkDot
            :clickable="!todo.completed"
            :priority="todo.priority"
            @click="!todo.completed && cyclePriority()"
          />
          {{ priorityLabel }}
        </span>

        <span
          v-if="dueLabel"
          class="due"
          :class="{ 'urgent': !todo.completed && isOverdue, 'due-soon': !todo.completed && isUrgent && !isOverdue }"
        >
          <HandClock :urgent="!todo.completed && isOverdue" />
          {{ dueLabel }}
        </span>
      </div>
    </div>

    <!-- 删除：手绘风垃圾桶（与邮件列表删除按钮一致），hover 时显隐 -->
    <button
      class="remove-btn"
      :title="t('plugins.todo.labels.deleteButton')"
      type="button"
      @click="emit('remove', todo.id)"
    >
      <svg
        fill="none"
        height="16"
        stroke="currentColor"
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        viewBox="0 0 24 24"
        width="16"
      >
        <!-- 手绘垃圾桶：桶盖横线 + 把手 + 桶身 + 两条竖纹（复刻 MailItem） -->
        <path d="M 4 7 Q 8 6.5 12 7 Q 16 7.5 20 7" />
        <path d="M 10 5 Q 12 4 14 5" />
        <path d="M 6 8 Q 5.5 8 5.7 9 L 7 19 Q 7.2 20 8.2 20 L 15.8 20 Q 16.8 20 17 19 L 18.3 9 Q 18.5 8 18 8" />
        <path d="M 10 11 Q 9.8 14 9.5 17" />
        <path d="M 14 11 Q 14.2 14 14.5 17" />
      </svg>
    </button>
  </div>
</template>

<style scoped>
.todo-item {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 12px 10px;
  position: relative;
  cursor: pointer;
  border-radius: 14px;
  transition: background 0.2s;
}

.todo-item:hover {
  background: color-mix(in srgb, var(--pink) 8%, transparent);
}

/* 暗色下 hover 用更柔和的粉 */
html.dark .todo-item:hover {
  background: color-mix(in srgb, var(--pink) 14%, transparent);
}

.checkbox-wrap {
  width: 24px;
  height: 24px;
  flex-shrink: 0;
  margin-top: 2px;
  cursor: pointer;
}

.todo-main {
  flex: 1;
  min-width: 0;
}

.todo-title {
  font-size: 17px;
  line-height: 1.6;
  word-break: break-word;
  font-weight: 700;
  color: var(--ink);
}

.todo-item.done .todo-title {
  color: var(--ink-faint);
  text-decoration: line-through;
  text-decoration-color: var(--ink-faint);
}

.todo-meta {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 6px;
  font-size: 13px;
}

.pri {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-weight: 600;
}

.pri.high {
  color: var(--red-ink);
}

.pri.medium {
  color: var(--orange);
}

.pri.low {
  color: var(--blue);
}

.due {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--ink-soft);
  font-weight: 600;
}

.due.due-soon {
  color: var(--blue);
}

.due.urgent {
  color: var(--red-ink);
}

.remove-btn {
  flex-shrink: 0;
  margin-top: 4px;
  padding: 2px;
  background: none;
  border: none;
  color: var(--ink-faint);
  cursor: pointer;
  opacity: 0;
  transition:
    color 0.2s,
    opacity 0.2s;
  font-family: inherit;
}

.todo-item:hover .remove-btn {
  opacity: 1;
}

.remove-btn:hover {
  color: var(--red-ink);
}
</style>
