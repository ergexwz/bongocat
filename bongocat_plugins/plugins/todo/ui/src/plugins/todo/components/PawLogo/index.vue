<script setup lang="ts">
/**
 * 爪印 logo（PawLogo）：SVG 画的 4 圆 + 椭圆（真爪印形状），不用 emoji。
 * SVG path 抠自 docs/designs/todo-panel-exploration/panel.html。
 */
defineProps<{
  /** 尺寸 px，默认 34（设计稿标题区尺寸） */
  size?: number
}>()
</script>

<template>
  <svg
    :height="size ?? 34"
    viewBox="0 0 34 34"
    :width="size ?? 34"
  >
    <ellipse
      cx="17"
      cy="23"
      fill="var(--pink)"
      rx="9"
      ry="6.5"
    />
    <circle
      cx="8"
      cy="15"
      fill="var(--pink)"
      r="3.2"
    />
    <circle
      cx="14"
      cy="10"
      fill="var(--pink)"
      r="3.2"
    />
    <circle
      cx="21"
      cy="10"
      fill="var(--pink)"
      r="3.2"
    />
    <circle
      cx="27"
      cy="15"
      fill="var(--pink)"
      r="3.2"
    />
  </svg>
</template>
