<script setup lang="ts">
/**
 * 邮件面板容器（MailPanel）：手绘纸张 + 极淡灰颗粒纹理 + 手绘阴影。
 *
 * 视觉同源 todo PaperPanel（同款 SVG 形状 + token），是邮件列表/归档窗口的主容器。
 * 设计稿 docs/designs/phase2-exploration/mail-list.html 的面板 SVG。
 *
 * 400px 宽（设计稿规格），高度由窗口 outerSize 决定（tauri.conf.json 写死 480/420，
 * SVG preserveAspectRatio="none" 填满）。
 *
 * @see docs/designs/phase2-exploration/mail-list.html
 */
</script>

<template>
  <div class="mail-panel">
    <svg
      class="panel-bg"
      preserveAspectRatio="none"
      viewBox="0 0 400 560"
    >
      <defs>
        <pattern
          id="mail-paper-texture"
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
        </pattern>
        <filter
          id="mail-shadow"
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
        d="M 16 14 Q 18 12 24 13 L 378 16 Q 386 18 385 24 L 388 544 Q 386 552 378 552 L 22 548 Q 14 546 15 540 Z"
        fill="var(--paper-shadow)"
        opacity="0.6"
      />
      <!-- 主面板（纯白底 + 手绘边框） -->
      <path
        d="M 14 10 Q 16 8 22 9 L 376 12 Q 384 14 383 20 L 386 540 Q 384 548 376 548 L 20 544 Q 12 542 13 536 Z"
        fill="url(#mail-paper-texture)"
        filter="url(#mail-shadow)"
        stroke="var(--ink)"
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2.5"
      />
      <!-- 顶部装饰波浪 -->
      <path
        d="M 40 30 Q 50 26 60 30 T 80 30"
        fill="none"
        opacity="0.7"
        stroke="var(--pink)"
        stroke-linecap="round"
        stroke-width="2"
      />
    </svg>

    <div class="panel-content">
      <slot />
    </div>

    <slot name="overlay" />
  </div>
</template>

<style scoped>
.mail-panel {
  position: relative;
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  min-height: 0;
}

.panel-bg {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
  z-index: 0;
}

.panel-content {
  position: relative;
  z-index: 1;
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
  padding: 24px 22px 20px;
}
</style>
