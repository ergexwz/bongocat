<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import type { TodoPriority } from '@/plugins/todo'

import { PRIORITIES } from '@/plugins/todo/utils/priority'

import InkDot from '../InkDot/index.vue'

/**
 * 优先级选择器（PriorityPicker / T6）：三档墨点点击选中。
 *
 * 用于新建 todo 的入口（主面板新建区 + 迷你窗），与 TodoItem 的「点击已有墨点循环切换」
 * 互补——这里是显式三选一（更清晰），那里是原地循环（更省地方）。
 *
 * 视觉：三档墨点横排，选中的下方加手绘波浪底纹（粉墨水系强调），未选中半透明。
 * emit `update:priority`（v-model）。
 *
 * @see docs/designs/todo-panel-exploration/panel.html 墨点 path 事实来源
 */
const { modelValue } = defineProps<{
  modelValue: TodoPriority
}>()

const emit = defineEmits<{
  'update:modelValue': [value: TodoPriority]
}>()

const { t } = useI18n()

function priorityLabel(p: TodoPriority): string {
  switch (p) {
    case 'high':
      return t('plugins.todo.labels.priorityHigh')
    case 'low':
      return t('plugins.todo.labels.priorityLow')
    case 'medium':
    default:
      return t('plugins.todo.labels.priorityMedium')
  }
}

function select(p: TodoPriority) {
  if (p !== modelValue)
    emit('update:modelValue', p)
}
</script>

<template>
  <div
    :aria-label="t('plugins.todo.labels.priorityPickerLabel')"
    class="picker"
    role="radiogroup"
  >
    <button
      v-for="p in PRIORITIES"
      :key="p"
      :aria-checked="p === modelValue"
      :aria-label="priorityLabel(p)"
      class="dot-btn"
      :class="{ selected: p === modelValue }"
      :title="priorityLabel(p)"
      type="button"
      @click="select(p)"
    >
      <InkDot :priority="p" />
      <!-- 选中标记：手绘波浪下划线（粉墨水系强调色） -->
      <svg
        v-if="p === modelValue"
        class="selected-mark"
        viewBox="0 0 16 4"
      >
        <path
          d="M 1 2 Q 4 0.5 8 2 Q 12 3.5 15 1.5"
          fill="none"
          stroke="var(--pink-deep)"
          stroke-linecap="round"
          stroke-width="1.5"
        />
      </svg>
    </button>
  </div>
</template>

<style scoped>
.picker {
  display: inline-flex;
  align-items: flex-end;
  gap: 10px;
}

/* 单个墨点按钮：透明背景，hover 放大反馈。 */
.dot-btn {
  position: relative;
  display: inline-flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  padding: 2px 1px;
  background: transparent;
  border: none;
  cursor: pointer;
  font-family: inherit;
  opacity: 0.45;
  transition:
    opacity 0.2s,
    transform 0.2s;
}

/* 选中态不透明 + 手绘波浪下划线显式标出。 */
.dot-btn.selected {
  opacity: 1;
}

.dot-btn:hover {
  opacity: 1;
  transform: scale(1.2);
}

/* 手绘波浪下划线：选中档下方居中。 */
.selected-mark {
  position: absolute;
  bottom: -4px;
  left: 50%;
  width: 16px;
  height: 4px;
  transform: translateX(-50%);
}
</style>
