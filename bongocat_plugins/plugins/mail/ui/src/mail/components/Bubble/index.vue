<script setup lang="ts">
/**
 * 桌宠气泡（手绘风，T2）。
 *
 * 视觉严格对照 `docs/designs/phase2-exploration/bubble.html`：圆胖手绘 SVG 形状
 * （严禁 CSS border 表现线条）、尾部三角贴桌宠头顶指向猫、851 字体 font-weight 600 基线。
 *
 * 邮件与 todo 共用本组件（通过 prop `type` 区分），为 T6 预留。本 ticket 只做邮件气泡，
 * todo 分支的内容（标题/副标题文案、红墨波浪）也一并实现以让 prop 结构稳定。
 *
 * 交互：点 × 关闭，点气泡本体触发 `action`（邮件→打开 webmail，todo→打开面板，由父决定），
 * 都随后触发 `close`。纯渐入渐出由父级 fade class 控制，本组件只渲染静态气泡。
 *
 * 形状自适应：SVG path 由 `genBubbleShape(textHeight)` 按内容真实高度动态生成——内容换行
 * 变高时，底部边框和尾部三角跟随下移，文字永远在形状内（修复用户报告的「内容超出气泡外」
 * bug，原写死 path 底部 y≈66 导致内容溢出）。容器高度 = viewBox 高（1:1 不拉伸），
 * 内容绝对定位在形状内部正确位置（上下留白对称）。
 *
 * @see docs/designs/phase2-exploration/bubble.html
 * @see ADR 0002 D7
 */
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { genBubbleShape } from '../../utils/bubbleShape'

/** 气泡类型（prop，为 T6 todo 气泡预留）。 */
type BubbleType = 'mail' | 'todo'

const props = defineProps<{
  /** 气泡类型：mail（信封图标 + 粉波浪）或 todo（手绘时钟 + 红墨波浪）。 */
  type: BubbleType
  /** 来源标签（如「新邮件 · Gmail」/「待办到期」）。 */
  source: string
  /** 主标题（邮件=发件人；todo=任务标题）。 */
  title: string
  /** 副标题（邮件=主题；todo=「已到期 · 高优先级」）。 */
  subtitle: string
  /** 副标题是否高亮（todo 到期用红墨）。 */
  urgent?: boolean
  /** 气泡宽度（px，父级根据所有气泡的最大自然宽度决定，默认 360）。 */
  width?: number
}>()

const emit = defineEmits<{
  /** 关闭气泡（点 × 触发，只消失不跳转）。 */
  close: []
  /** 点击气泡本体（邮件→打开 webmail；todo→打开面板），随后父级也会 close。 */
  action: []
  /**
   * 内容尺寸变化（文字换行 / 长链接），父级据以重设窗口尺寸。
   * payload = { height: 纯文字高度, naturalWidth: 内容自然宽度（不限宽，含 padding） }。
   */
  resize: [height: number, naturalWidth: number]
}>()

const { t } = useI18n()

/** SVG 滤镜 id 在窗口内需唯一，多气泡堆叠时用随机后缀避免冲突。 */
const filterId = `b-sh-${Math.random().toString(36).slice(2, 8)}`

/**
 * 纯文字高度（px，不含 padding）。由 ResizeObserver 测量 .bubble-content 的
 * getBoundingClientRect().height 决定（content 无上下 padding，只有左右 padding）。
 * 初始 60（设计稿单行内容估值），首帧后由 observer 校正到真实值。
 */
const textHeight = ref(60)

/**
 * 内容自然宽度（px，含左右 padding）。由 scrollWidth 测量，反映长链接/长文本不换行时的真实宽度。
 * 父级收集所有气泡的 naturalWidth 取 max，决定窗口宽度（≥360）。
 */
const naturalWidth = ref(0)

/** 根据 textHeight + width 动态生成形状（path + viewBox + 尺寸 + contentTop）。 */
const shape = computed(() => genBubbleShape(textHeight.value, props.width))

/** 根元素 ref（容器，设高度 = shape.height）。 */
const rootEl = ref<HTMLElement | null>(null)

/** 内容元素 ref（绝对定位，top = shape.contentTop）。 */
const contentEl = ref<HTMLElement | null>(null)

let ro: ResizeObserver | undefined

/**
 * 观察内容尺寸变化（文字换行 / 长链接），重算形状 + 通知父级重设窗口尺寸。
 *
 * 测量两个维度：
 * - height：纯文字高度（getBoundingClientRect().height），驱动 SVG path 高度
 * - naturalWidth：内容自然宽度（scrollWidth，含溢出部分）。父级取所有气泡的 max 决定窗口宽度，
 *   避免长链接在固定 360 内被挤压或溢出。
 */
function measureContent() {
  const el = contentEl.value
  if (!el) {
    return
  }
  const h = Math.round(el.getBoundingClientRect().height)
  const nw = el.scrollWidth
  const hChanged = h > 0 && h !== textHeight.value
  const wChanged = nw > 0 && nw !== naturalWidth.value
  if (hChanged) {
    textHeight.value = h
  }
  if (wChanged) {
    naturalWidth.value = nw
  }
  if (hChanged || wChanged) {
    emit('resize', h, nw)
  }
}

watch(contentEl, (el) => {
  if (ro) {
    ro.disconnect()
  }
  if (!el) {
    return
  }
  ro = new ResizeObserver(() => measureContent())
  ro.observe(el)
  measureContent()
})
</script>

<template>
  <div
    ref="rootEl"
    class="bubble"
    :style="{ height: `${shape.height}px`, width: `${shape.width}px` }"
    @click="emit('action')"
  >
    <!-- 圆胖手绘气泡背景（SVG path，严禁 CSS border）。path 由 genBubbleShape 按文字高度+宽度动态生成。
         width/height = viewBox 尺寸（1:1，preserveAspectRatio="none" 不拉伸）。 -->
    <svg
      class="bubble-bg"
      :height="shape.height"
      preserveAspectRatio="none"
      :viewBox="shape.viewBox"
      :width="shape.width"
    >
      <defs>
        <filter
          :id="filterId"
          height="135%"
          width="120%"
          x="-10%"
          y="-10%"
        >
          <feGaussianBlur
            in="SourceAlpha"
            stdDeviation="2.5"
          />
          <feOffset
            dx="1.5"
            dy="3"
          />
          <feComponentTransfer><feFuncA
            slope="0.12"
            type="linear"
          /></feComponentTransfer>
          <feMerge><feMergeNode /><feMergeNode in="SourceGraphic" /></feMerge>
        </filter>
      </defs>
      <!-- 阴影层（错位手绘形状，淡棕） -->
      <path
        :d="shape.shadowD"
        fill="var(--paper-shadow)"
        opacity="0.6"
      />
      <!-- 主气泡（纯白底 + 手绘圆胖边框 + 尾部三角指向猫） -->
      <path
        :d="shape.mainD"
        fill="#fff"
        :filter="`url(#${filterId})`"
        stroke="var(--ink)"
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2.5"
      />
      <!-- 顶部装饰手绘波浪：邮件=粉，todo=红墨 -->
      <path
        :d="shape.waveD"
        fill="none"
        opacity="0.6"
        :stroke="type === 'todo' ? 'var(--red-ink)' : 'var(--pink)'"
        stroke-linecap="round"
        stroke-width="1.8"
      />
    </svg>

    <!-- 内容区：绝对定位在形状内部（top = shape.contentTop），上下留白对称。
         无上下 padding（左右才有），高度 = 纯文字高度，驱动形状。 -->
    <div
      ref="contentEl"
      class="bubble-content"
      :style="{ top: `${shape.contentTop}px` }"
    >
      <!-- 来源图标圆底（邮件=蓝调信封；todo=红墨手绘时钟） -->
      <div
        class="source-icon"
        :class="type"
      >
        <template v-if="type === 'mail'">
          <svg
            fill="none"
            height="20"
            stroke="#5a85b0"
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            viewBox="0 0 24 24"
            width="20"
          >
            <rect
              height="13"
              rx="2.5"
              width="18"
              x="3"
              y="6"
            />
            <path d="M 4 8 Q 12 13 20 8" />
          </svg>
        </template>
        <template v-else>
          <svg
            fill="none"
            height="20"
            stroke="#d4654a"
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            viewBox="0 0 24 24"
            width="20"
          >
            <path d="M 12 2 Q 18 2 21 6 Q 23 9 22.5 13 Q 21.5 19 16 21 Q 12 22 8 21 Q 3 19 2.5 13.5 Q 2 9 4 6 Q 6 2 12 2 Z" />
            <path d="M 12 12 Q 12 8 13 6" />
            <path d="M 12 12 Q 16 12 18 11" />
            <circle
              cx="12"
              cy="12"
              fill="#d4654a"
              r="0.8"
              stroke="none"
            />
          </svg>
        </template>
      </div>

      <div class="bubble-body">
        <div class="bubble-source">
          {{ source }}
        </div>
        <div class="bubble-title">
          {{ title }}
        </div>
        <div
          class="bubble-subtitle"
          :class="{ urgent }"
        >
          {{ subtitle }}
        </div>
      </div>

      <!-- 手绘 × 关闭（两笔 Q 曲线交叉，非 CSS border） -->
      <button
        :aria-label="t('plugins.mail.labels.bubbleClose')"
        class="bubble-close"
        type="button"
        @click.stop="emit('close')"
      >
        <svg
          fill="none"
          height="18"
          stroke="currentColor"
          stroke-linecap="round"
          stroke-width="2.5"
          viewBox="0 0 24 24"
          width="18"
        >
          <!-- 手绘 ×：两段顺滑大弧交叉，与 todo 面板一致 -->
          <path d="M 6 6 Q 9.5 13 19 19" />
          <path d="M 18 6 Q 8.5 10 5 18" />
        </svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
.bubble {
  position: relative;
  cursor: pointer;
  user-select: none;
}

.bubble-bg {
  position: absolute;
  left: 0;
  top: 0;
  z-index: 0;
  pointer-events: none;
}

/* 内容区绝对定位（top 由 :style 绑定 shape.contentTop）。
   只含左右 padding（撑开文字宽度），无上下 padding（高度 = 纯文字高度，驱动形状）。 */
.bubble-content {
  position: absolute;
  left: 0;
  width: 100%;
  z-index: 1;
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding-left: 20px;
  padding-right: 20px;
  box-sizing: border-box;
}

.source-icon {
  display: flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  width: 36px;
  height: 36px;
  border-radius: 50%;
}

.source-icon.mail {
  background: rgb(155 196 224 / 25%);
}

.source-icon.todo {
  background: rgb(244 168 160 / 20%);
}

.bubble-body {
  flex: 1;
  min-width: 0;
}

.bubble-source {
  margin-bottom: 4px;
  font-size: 12px;
  font-weight: 600;
  color: var(--ink-faint);
}

.bubble-title {
  font-size: 17px;
  font-weight: 700;
  line-height: 1.5;
  color: var(--ink);
  overflow-wrap: break-word;
}

.bubble-subtitle {
  margin-top: 4px;
  font-size: 13px;
  line-height: 1.5;
  color: var(--ink-soft);
  overflow-wrap: break-word;
}

.bubble-subtitle.urgent {
  font-weight: 600;
  color: var(--red-ink);
}

.bubble-close {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--ink-faint);
  cursor: pointer;
  transition: color 0.2s;
}

.bubble-close:hover {
  color: var(--ink);
}
</style>
