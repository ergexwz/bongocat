<script setup lang="ts">
import { computed } from 'vue'

import type { TodoPriority } from '@/plugins/todo'

/**
 * 墨点（InkDot）：「笔画小圈涂黑」感。三层 path：
 *   外圈不规则淡色光晕 + 内部涂黑核心 + 笔触高光（留白椭圆）。
 * 按 priority 变色：high 红 / medium 橙 / low 蓝。
 * SVG path 抠自 docs/designs/todo-panel-exploration/panel.html。
 *
 * `clickable`：T6 输入控件完整化。为 true 时渲染为手绘风按钮（hover 放大 + pointer），
 * 点击 emit `click`（由父决定循环切换或选中逻辑）。默认 false 保持纯展示态，
 * 不影响 TodoItem 已有的展示用法。
 */
const {
  clickable = false,
  priority,
} = defineProps<{
  priority: TodoPriority
  clickable?: boolean
}>()

const emit = defineEmits<{
  click: []
}>()

const color = computed(() => {
  switch (priority) {
    case 'high':
      return 'var(--red-ink)'
    case 'low':
      return 'var(--blue)'
    case 'medium':
    default:
      return 'var(--orange)'
  }
})

function onClick() {
  emit('click')
}
</script>

<template>
  <component
    :is="clickable ? 'button' : 'div'"
    class="ink-dot"
    :class="{ 'is-clickable': clickable }"
    :type="clickable ? 'button' : undefined"
    @click="clickable && onClick()"
  >
    <svg
      viewBox="0 0 14 14"
    >
      <!-- 外圈不规则淡色光晕 -->
      <path
        d="M 7 2 Q 11 2 12 5 Q 13 7 12 9 Q 11 12 7 12 Q 3 12 2 9 Q 1 7 2 5 Q 3 2 7 2 Z"
        :fill="color"
        opacity="0.25"
      />
      <!-- 内部涂黑核心 -->
      <path
        d="M 7 3.5 Q 9.5 3.5 10.5 5.5 Q 11 7 10 8.5 Q 9 10.5 7 10.5 Q 5 10.5 4 8.5 Q 3 7 3.5 5.5 Q 4.5 3.5 7 3.5 Z"
        :fill="color"
      />
      <!-- 笔触高光（留白椭圆） -->
      <ellipse
        cx="5.5"
        cy="5.5"
        :fill="color"
        opacity="0.5"
        rx="1.2"
        ry="0.8"
      />
    </svg>
  </component>
</template>

<style scoped>
.ink-dot {
  width: 14px;
  height: 14px;
  flex-shrink: 0;
}

.ink-dot svg {
  width: 100%;
  height: 100%;
}

/* 手绘风可点击按钮：透明背景无边框，hover 放大反馈（萌系手感）。 */
.ink-dot.is-clickable {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  background: transparent;
  border: none;
  cursor: pointer;
  font-family: inherit;
  transition: transform 0.2s ease;
}

.ink-dot.is-clickable:hover {
  transform: scale(1.3);
}

.ink-dot.is-clickable:active {
  transform: scale(1.15);
}
</style>
