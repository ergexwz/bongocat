<script setup lang="ts">
/**
 * 手绘波浪分隔线（WaveDivider）：`Q 25 1 50 3 T 98 3` 波浪，不是直线。
 * 用于分组标签下划线。preserveAspectRatio="none" 横向拉伸填满容器。
 * SVG path 抠自 docs/designs/todo-panel-exploration/panel.html。
 */
</script>

<template>
  <svg
    class="wave-divider"
    preserveAspectRatio="none"
    viewBox="0 0 100 6"
  >
    <path
      d="M 2 3 Q 25 1 50 3 T 98 3"
      fill="none"
      stroke="var(--ink-faint)"
      stroke-linecap="round"
      stroke-width="1.5"
    />
  </svg>
</template>

<style scoped>
.wave-divider {
  flex: 1;
  height: 6px;
}
</style>
