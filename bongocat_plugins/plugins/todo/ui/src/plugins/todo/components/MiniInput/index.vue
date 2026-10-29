<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import type { TodoPriority } from '@/plugins/todo'

import HandDateInput from '../HandDateInput/index.vue'
import PawLogo from '../PawLogo/index.vue'
import PriorityPicker from '../PriorityPicker/index.vue'
import ResizeGrip from '../ResizeGrip/index.vue'
import ZoomControl from '../ZoomControl/index.vue'

/**
 * 迷你输入窗（MiniInput / T5，T6 补优先级选择）。
 *
 * 定位锚点是桌宠（main 窗口）上方偏右，而非光标——实测桌宠右键菜单弹出位置距光标远，
 * 跟随光标会让迷你窗出现在意料外的地方。
 *
 * 三态：idle（占位符）→ typing（用户输入）→ saved（绿色「已添加」整窗居中放大，淡出后关闭）。
 * 布局（T6 两行）：
 *   Row1 = PawLogo（拖动）+ 标题 input + 关闭×
 *   Row2 = PriorityPicker（三档墨点，默认 medium）+ HandDateInput（复用 T5 的 5 个手写数字框）
 *
 * 日期时间全部手写数字输入框（无日历弹层、无 antd 依赖），皮肤完全手绘可控。
 * T6 起日期输入抽成 HandDateInput 组件供主面板新建复用，本组件不再内联数字框逻辑。
 *
 * 拖动：爪印、Row2/Row3 的非 input 区显式挂 data-tauri-drag-region；input/button 天然豁免。
 *
 * 关闭路径：右上角 × / Esc / 提交后淡出 timer。**不**用 input blur 或窗口失焦自动关闭
 * （前者会破坏 tab 到下一框）。
 *
 * 视觉复用 T2 资产：851 手写字体（.todo-handdrawn 继承）、粉墨水系 CSS 变量、PawLogo、HandClock。
 * 边框 SVG path 抠自 docs/designs/todo-panel-exploration/mini-input.html 变体 B（中笔画）。
 *
 * @see docs/designs/todo-panel-exploration/mini-input.html 视觉事实来源
 */
const { scale } = defineProps<{
  scale: number
}>()

const emit = defineEmits<{
  close: []
  create: [title: string, dueDate: number | undefined, priority: TodoPriority]
  scaleChange: [scale: number]
  scalePreview: [scale: number]
  scaleCommit: [scale: number]
}>()

const { t } = useI18n()

type State = 'idle' | 'typing' | 'saved'
const state = ref<State>('idle')
/** saved 态淡出标志：true 时整个迷你窗 opacity→0 平滑淡出。 */
const fading = ref(false)

const title = ref('')
const titleEl = ref<HTMLInputElement | null>(null)

/** 新建 todo 的优先级（默认 medium）。 */
const priority = ref<TodoPriority>('medium')
/** 新建 todo 的截止日期 timestamp（未填日期时 undefined）。 */
const dueDate = ref<number | undefined>(undefined)

/** saved 展示时长：前半段静止显示「已添加」，后半段淡出。 */
const SAVED_HOLD_MS = 300
const SAVED_FADE_MS = 300
let holdTimer: null | ReturnType<typeof setTimeout> = null
let closeTimer: null | ReturnType<typeof setTimeout> = null

onMounted(async () => {
  await nextTick()
  titleEl.value?.focus()
})

async function focusTitle() {
  await nextTick()
  titleEl.value?.focus()
}

defineExpose({ focusTitle })

onBeforeUnmount(() => {
  clearTimers()
})

function markTyping() {
  if (state.value === 'idle') {
    state.value = 'typing'
  }
}

function onDateChange(value: number | undefined) {
  dueDate.value = value
  markTyping()
}

/** 日期框回车：标题有内容则提交，无内容则聚焦标题框。 */
function handleDateEnter() {
  if (title.value.trim()) {
    handleSubmit()
  } else {
    titleEl.value?.focus()
  }
}

function handleSubmit() {
  const trimmed = title.value.trim()
  if (!trimmed || state.value === 'saved')
    return

  emit('create', trimmed, dueDate.value, priority.value)

  state.value = 'saved'
  // 前半段静止展示「已添加」，后半段淡出，然后关闭。
  holdTimer = setTimeout(() => {
    fading.value = true
  }, SAVED_HOLD_MS)
  closeTimer = setTimeout(() => {
    emit('close')
  }, SAVED_HOLD_MS + SAVED_FADE_MS)
}

function handleClose() {
  // 已保存态由 timer 驱动关闭，避免重复 emit。
  if (state.value === 'saved')
    return
  clearTimers()
  emit('close')
}

function clearTimers() {
  if (holdTimer) {
    clearTimeout(holdTimer)
    holdTimer = null
  }
  if (closeTimer) {
    clearTimeout(closeTimer)
    closeTimer = null
  }
}
</script>

<template>
  <div
    class="mini-window"
    :class="{ 'is-fading': fading }"
  >
    <!-- SVG 手绘边框：viewBox 与窗口尺寸一致，preserveAspectRatio="none" 拉伸填满。
         saved 态描边/底色变绿（#6b9c47 / #f6fbf2）；其他态白底 + 棕墨描边。 -->
    <svg
      class="frame-bg"
      preserveAspectRatio="none"
      viewBox="0 0 380 130"
    >
      <defs>
        <filter
          id="mini-shadow"
          height="120%"
          width="120%"
          x="-10%"
          y="-10%"
        >
          <feGaussianBlur
            in="SourceAlpha"
            stdDeviation="2"
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
      <!-- 阴影层（按 280→380 等比缩放） -->
      <path
        d="M 19 12 Q 21.7 10 27.1 10 L 361 12 Q 369.1 14 367.8 18 L 370.5 98 Q 367.8 104 361 104 L 24.4 102 Q 16.3 100 17.6 96 Z"
        :fill="state === 'saved' ? '#e0f0d8' : '#e8dcc8'"
        opacity="0.6"
      />
      <!-- 主框（变体 B 中笔画；按 280→380 等比缩放，四边中间有起伏） -->
      <path
        d="M 14.9 9 Q 17.6 6 23.1 6.5 Q 190 5 356.9 8 Q 365.1 10 364.4 14 Q 366.4 55 363.7 98 Q 362.4 104 355.6 104.5 Q 190 106 17.6 103 Q 10.9 101 12.2 97 Q 13.6 50 14.9 9 Z"
        :fill="state === 'saved' ? '#f6fbf2' : 'var(--paper)'"
        filter="url(#mini-shadow)"
        :stroke="state === 'saved' ? '#6b9c47' : 'var(--ink)'"
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
      />
    </svg>

    <div class="mini-content">
      <!-- 右上角关闭 ×（复用 TodoPanel 的手绘 × path，缩小到 16px） -->
      <button
        class="close-btn"
        :title="t('plugins.todo.labels.closeButton')"
        type="button"
        @click="handleClose"
      >
        <svg
          fill="none"
          height="16"
          stroke="currentColor"
          stroke-linecap="round"
          stroke-width="2.5"
          viewBox="0 0 24 24"
          width="16"
        >
          <path d="M 6 6 Q 9.5 13 19 19" />
          <path d="M 18 6 Q 8.5 10 5 18" />
        </svg>
      </button>

      <ZoomControl
        class="mini-zoom"
        compact
        :scale="scale"
        @change="emit('scaleChange', $event)"
      />

      <!-- saved 态：整窗上下居中放大显示「已添加」+ 手绘对勾 -->
      <div
        v-if="state === 'saved'"
        class="saved-view"
      >
        <svg
          class="saved-check"
          fill="none"
          height="28"
          stroke="currentColor"
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="3"
          viewBox="0 0 18 18"
          width="28"
        >
          <path d="M 3 9 Q 6 12 7 13 Q 11 7 15 4" />
        </svg>
        <span class="saved-text">{{ t('plugins.todo.labels.miniSaved') }}</span>
      </div>

      <!-- 输入态：标题 + 优先级 + 日期三行。
           可拖动区显式挂 data-tauri-drag-region（爪印、优先级行空白、日期行空白），
           input/button 天然豁免仍可交互。 -->
      <template v-else>
        <!-- Row1：整行 drag-region（学 TodoPanel .panel-header 模式）。
             爪印设 pointer-events:none 透传给行 drag-region；title input 天然豁免仍可交互。 -->
        <div
          class="title-row"
          data-tauri-drag-region
        >
          <span class="paw-deco">
            <PawLogo :size="18" />
          </span>
          <input
            ref="titleEl"
            v-model="title"
            class="title-input"
            :placeholder="t('plugins.todo.labels.miniPlaceholder')"
            type="text"
            @input="markTyping"
            @keydown.enter.prevent="handleSubmit"
            @keydown.esc.prevent="handleClose"
          >
        </div>

        <!-- Row2：优先级 + 日期时间同一行（与主面板新建区一致的紧凑布局）。
             整行 drag-region，PriorityPicker 按钮和 HandDateInput 的 input 天然豁免可交互。
             HandDateInput 内含左侧时钟图标，右侧不再放图标避免溢出（用户反馈）。 -->
        <div
          class="controls-row"
          data-tauri-drag-region
        >
          <PriorityPicker v-model="priority" />
          <HandDateInput
            class="date-row-wrap"
            @change="onDateChange"
            @enter="handleDateEnter"
          />
        </div>
      </template>
    </div>

    <ResizeGrip
      class="mini-resize"
      :base-width="380"
      :base-height="130"
      :scale="scale"
      @input="emit('scalePreview', $event)"
      @commit="emit('scaleCommit', $event)"
    />
  </div>
</template>

<style scoped>
/* 迷你窗尺寸与窗口 setSize 一致（380×130，与主面板同宽）。透明背景透出桌面。 */
.mini-window {
  position: relative;
  width: 100%;
  height: 100%;
  /* saved 淡出：opacity 平滑过渡到 0（spec「淡出关闭」）。 */
  transition: opacity 300ms ease-out;
}

.mini-window.is-fading {
  opacity: 0;
}

.frame-bg {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
  z-index: 0;
}

.mini-content {
  position: relative;
  z-index: 1;
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: 100%;
  height: 100%;
  padding: 14px 18px;
  box-sizing: border-box;
}

/* 右上角关闭 × */
.close-btn {
  position: absolute;
  top: 6px;
  right: 14px;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  padding: 0;
  background: none;
  border: none;
  color: var(--ink-faint);
  cursor: pointer;
  transition: color 0.2s;
  font-family: inherit;
  z-index: 2;
}

.close-btn:hover {
  color: var(--ink);
}

.title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 6px;
  padding-right: 94px;
}

.mini-zoom {
  position: absolute;
  top: 6px;
  right: 40px;
  z-index: 2;
}

.mini-resize {
  position: absolute;
  right: 5px;
  bottom: 5px;
}

/* 爪印：装饰，pointer-events:none 让点击/拖动透传给行 drag-region（TodoPanel proven 模式）。 */
.paw-deco {
  display: flex;
  align-items: center;
  flex-shrink: 0;
  pointer-events: none;
}

.title-input {
  flex: 1;
  min-width: 0;
  padding: 0;
  background: transparent;
  border: none;
  outline: none;
  font-family: inherit;
  font-size: 16px;
  font-weight: 700;
  line-height: 1.5;
  color: var(--ink);
}

.title-input::placeholder {
  color: var(--ink-faint);
  font-weight: 500;
}

/* Row2：优先级 + 日期横排，两端对齐（优先级贴左、日期贴右）。
   整行空白可拖动（drag-region），PriorityPicker 按钮 / HandDateInput 的 input 天然豁免。 */
.controls-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding-right: 16px;
}

/* HandDateInput 外层包裹：日期组不收缩，贴右排布。 */
.date-row-wrap {
  flex-shrink: 0;
}

/* saved 态：整窗上下居中放大，对勾也放大。 */
.saved-view {
  display: flex;
  flex: 1;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
}

.saved-check {
  color: #6b9c47;
  flex-shrink: 0;
}

.saved-text {
  font-size: 18px;
  font-weight: 700;
  color: #6b9c47;
}
</style>
