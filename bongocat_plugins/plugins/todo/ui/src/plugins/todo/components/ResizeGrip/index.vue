<script setup lang="ts">
import { getCurrentWebview } from '@tauri-apps/api/webview'
import {
  PhysicalSize,
  currentMonitor,
  getCurrentWindow,
} from '@tauri-apps/api/window'
import { onBeforeUnmount } from 'vue'
import { useI18n } from 'vue-i18n'

const props = withDefaults(defineProps<{
  baseWidth: number
  baseHeight: number
  scale: number
  min?: number
  max?: number
}>(), {
  min: 0.8,
  max: 1.5,
})

const emit = defineEmits<{
  input: [scale: number]
  commit: [scale: number]
}>()

const { t } = useI18n()

let dragging = false
let pointerId: number | null = null
let grip: HTMLElement | null = null
let startClientX = 0
let startClientY = 0
let offsetX = 0
let offsetY = 0
let startWidth = 1
let startHeight = 1
let dpi = 1
let maxScale = props.max
let latest = props.scale
let applied = props.scale
let frame = 0

function clampScale(value: number) {
  return Math.min(maxScale, Math.max(props.min, Math.round(value * 1000) / 1000))
}

async function resizeTo(scale: number) {
  await getCurrentWindow().setSize(new PhysicalSize(
    Math.round(props.baseWidth * scale * dpi),
    Math.round(props.baseHeight * scale * dpi),
  ))
  await getCurrentWebview().setZoom(scale).catch((error) => {
    console.error('could not apply todo zoom', error)
  })
  emit('input', scale)
}

async function flush() {
  const next = latest
  if (Math.abs(next - applied) < 0.001)
    return
  applied = next
  await resizeTo(next)
}

async function pointerDown(event: PointerEvent) {
  if (event.button !== 0)
    return
  event.preventDefault()

  const win = getCurrentWindow()
  const [size, monitor, position] = await Promise.all([
    win.innerSize(),
    currentMonitor(),
    win.outerPosition(),
  ])
  dpi = monitor?.scaleFactor ?? 1
  startClientX = event.clientX
  startClientY = event.clientY
  startWidth = size.width
  startHeight = size.height
  const startDpr = globalThis.devicePixelRatio || dpi
  offsetX = startWidth - startClientX * startDpr
  offsetY = startHeight - startClientY * startDpr
  latest = props.scale
  applied = props.scale
  maxScale = props.max

  const area = monitor?.workArea
  if (area && position) {
    const availableWidth = (area.position.x + area.size.width - position.x) / dpi
    const availableHeight = (area.position.y + area.size.height - position.y) / dpi
    maxScale = Math.max(
      props.min,
      Math.min(props.max, availableWidth / props.baseWidth, availableHeight / props.baseHeight),
    )
  }

  dragging = true
  pointerId = event.pointerId
  grip = event.currentTarget as HTMLElement
  grip.setPointerCapture(event.pointerId)
  document.body.classList.add('is-resizing-window')
}

function pointerMove(event: PointerEvent) {
  if (!dragging || event.pointerId !== pointerId)
    return
  event.preventDefault()

  // clientX/Y are WebView CSS pixels. devicePixelRatio converts them to the
  // physical pixels used by the window, including page zoom and monitor DPI.
  const dpr = globalThis.devicePixelRatio || dpi
  const desiredWidth = event.clientX * dpr + offsetX
  const desiredHeight = event.clientY * dpr + offsetY
  const movedX = Math.abs(event.clientX - startClientX) * dpr
  const movedY = Math.abs(event.clientY - startClientY) * dpr
  const useWidth =
    movedX / Math.max(1, startWidth) >= movedY / Math.max(1, startHeight)
  const raw = useWidth
    ? desiredWidth / (props.baseWidth * dpi)
    : desiredHeight / (props.baseHeight * dpi)
  latest = clampScale(raw)

  if (Math.abs(latest - applied) < 0.001 || frame)
    return
  frame = requestAnimationFrame(() => {
    frame = 0
    void flush()
  })
}

async function pointerUp(event: PointerEvent) {
  if (!dragging || event.pointerId !== pointerId)
    return
  dragging = false
  if (frame) {
    cancelAnimationFrame(frame)
    frame = 0
  }
  await flush()
  if (grip?.hasPointerCapture(event.pointerId))
    grip.releasePointerCapture(event.pointerId)
  grip = null
  pointerId = null
  document.body.classList.remove('is-resizing-window')
  emit('commit', latest)
}

function pointerCancel(event: PointerEvent) {
  void pointerUp(event)
}

onBeforeUnmount(() => {
  dragging = false
  if (frame)
    cancelAnimationFrame(frame)
  document.body.classList.remove('is-resizing-window')
})
</script>

<template>
  <div
    class="resize-grip"
    role="separator"
    :title="t('plugins.todo.labels.resizeHandle')"
    :aria-label="t('plugins.todo.labels.resizeHandle')"
    @pointerdown="pointerDown"
    @pointermove="pointerMove"
    @pointerup="pointerUp"
    @pointercancel="pointerCancel"
  >
    <svg
      aria-hidden="true"
      fill="none"
      viewBox="0 0 16 16"
    >
      <path d="M 5 14 L 14 5" />
      <path d="M 8.5 14 L 14 8.5" />
      <path d="M 12 14 L 14 12" />
    </svg>
  </div>
</template>

<style scoped>
.resize-grip {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  color: var(--ink-faint);
  cursor: nwse-resize;
  opacity: 0.7;
  touch-action: none;
  z-index: 20;
}

.resize-grip:hover {
  color: var(--ink-soft);
  opacity: 1;
}

.resize-grip svg {
  width: 14px;
  height: 14px;
  pointer-events: none;
  stroke: currentColor;
  stroke-linecap: round;
  stroke-width: 1.6;
}
</style>
