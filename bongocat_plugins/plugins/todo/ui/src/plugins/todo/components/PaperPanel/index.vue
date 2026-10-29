<script setup lang="ts">
/**
 * 纸张容器（PaperPanel）：手绘不规则边框 + 纯白底 + 极淡灰颗粒纹理 + 手绘阴影。
 * 所有线条走 SVG path（无 CSS border）。窗口 resizable 时 SVG preserveAspectRatio="none"
 * 填满容器，手绘边框轻微拉伸仍属手绘感。
 *
 * SVG path 抠自 docs/designs/todo-panel-exploration/panel.html。
 */
</script>

<template>
  <div class="paper-panel">
    <!-- SVG 面板背景：纯白 + 手绘不规则边框 -->
    <svg
      class="paper-bg"
      preserveAspectRatio="none"
      viewBox="0 0 360 540"
    >
      <defs>
        <!-- 纯白纸张的轻微颗粒纹理 -->
        <pattern
          id="paper-texture"
          height="40"
          patternUnits="userSpaceOnUse"
          width="40"
          x="0"
          y="0"
        >
          <rect
            fill="var(--paper)"
            height="40"
            width="40"
          />
          <circle
            cx="8"
            cy="12"
            fill="var(--ink-faint)"
            opacity="0.18"
            r="0.4"
          />
          <circle
            cx="28"
            cy="6"
            fill="var(--ink-faint)"
            opacity="0.14"
            r="0.3"
          />
          <circle
            cx="18"
            cy="30"
            fill="var(--ink-faint)"
            opacity="0.18"
            r="0.4"
          />
          <circle
            cx="35"
            cy="25"
            fill="var(--ink-faint)"
            opacity="0.14"
            r="0.3"
          />
        </pattern>
        <!-- 手绘阴影 -->
        <filter
          id="hand-shadow"
          height="120%"
          width="120%"
          x="-10%"
          y="-10%"
        >
          <feGaussianBlur
            in="SourceAlpha"
            stdDeviation="3"
          />
          <feOffset
            dx="2"
            dy="4"
          />
          <feComponentTransfer><feFuncA
            slope="0.12"
            type="linear"
          /></feComponentTransfer>
          <feMerge><feMergeNode /><feMergeNode in="SourceGraphic" /></feMerge>
        </filter>
      </defs>

      <!-- 阴影层（错位手绘形状） -->
      <path
        d="M 16 14 Q 18 12 24 13 L 338 16 Q 346 18 345 24 L 348 524 Q 346 532 338 532 L 22 528 Q 14 526 15 520 Z"
        fill="var(--paper-shadow)"
        opacity="0.6"
      />

      <!-- 主面板（纯白底 + 手绘边框） -->
      <path
        d="M 14 10 Q 16 8 22 9 L 336 12 Q 344 14 343 20 L 346 520 Q 344 528 336 528 L 20 524 Q 12 522 13 516 Z"
        fill="url(#paper-texture)"
        filter="url(#hand-shadow)"
        stroke="var(--ink)"
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2.5"
      />

      <!-- 顶部装饰：手绘小波浪（粉色） -->
      <path
        d="M 40 30 Q 50 26 60 30 T 80 30 T 100 30"
        fill="none"
        opacity="0.7"
        stroke="var(--pink)"
        stroke-linecap="round"
        stroke-width="2"
      />
      <!-- 右上角小装饰 -->
      <circle
        cx="320"
        cy="32"
        fill="none"
        opacity="0.6"
        r="3"
        stroke="var(--pink)"
        stroke-width="1.5"
      />
      <circle
        cx="330"
        cy="36"
        fill="var(--pink)"
        opacity="0.5"
        r="2"
      />
    </svg>

    <div class="paper-content">
      <slot />
    </div>
  </div>
</template>

<style scoped>
/* pattern/filter id 需全局唯一，避免多实例冲突；用 :deep 不需要因为 svg 在本组件 scope 内。
   但 SVG <use>/fill 的 id 引用在 scoped 下仍正常（id 是文档级的，vue scoped 给元素加 data-v 不影响 id 解析）。*/
.paper-panel {
  position: relative;
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
}

.paper-bg {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
  z-index: 0;
}

.paper-content {
  position: relative;
  z-index: 1;
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  padding: 26px 22px 22px;
}
</style>
