<script setup lang="ts">
/**
 * 手绘 checkbox（HandCheckbox）：四角弧度各不同的不规则方形（模拟手画）。
 * - 未勾：白底 + 棕墨框。
 * - 已勾：粉色填充 + 深粉框 + 手绘对勾（带弧度两段曲线）。
 * SVG path 抠自 docs/designs/todo-panel-exploration/panel.html。
 */
defineProps<{
  checked?: boolean
}>()
</script>

<template>
  <svg
    class="hand-checkbox"
    viewBox="0 0 24 24"
  >
    <!-- 手绘不规则外框（四角弧度各不同） -->
    <path
      d="M 3.5 5.5 Q 3 3.5 5.5 3.2 Q 11 2.8 18.5 3.5 Q 21 3.8 20.8 6 Q 21 11 20.5 18 Q 20.5 21 18 21.2 Q 12 21.5 5.5 20.8 Q 3 20.5 3.2 18 Q 3 12 3.5 5.5 Z"
      :fill="checked ? 'var(--pink)' : 'var(--paper)'"
      :stroke="checked ? 'var(--pink-deep)' : 'var(--ink-soft)'"
      stroke-linecap="round"
      stroke-linejoin="round"
      stroke-width="2.2"
    />
    <!-- 已勾：手绘对勾（带弧度的两段曲线） -->
    <path
      v-if="checked"
      d="M 6 12 Q 8 15 9 16 Q 13 10 16 7"
      fill="none"
      stroke="#fff"
      stroke-linecap="round"
      stroke-linejoin="round"
      stroke-width="2.8"
    />
  </svg>
</template>

<style scoped>
.hand-checkbox {
  width: 100%;
  height: 100%;
  transition: all 0.2s;
}
</style>
