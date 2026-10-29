<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification'
import { computed, nextTick, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import Bubble from '@/bubble/Bubble.vue'
import type { Todo, TodoPriority } from '@/plugins/todo'
import MiniInput from '@/plugins/todo/components/MiniInput/index.vue'
import TodoPanel from '@/plugins/todo/components/TodoPanel/index.vue'
import { changePriority, createTodo, refresh, removeTodo, setUiScale, toggleTodo, todos, uiSettings } from '@/store'
import { currentVisible, hideCurrent, showAt } from '@/window'

const rawView = new URLSearchParams(window.location.search).get('view') ?? 'runtime'
const view = rawView === 'mini' ? 'todo-mini' : rawView
const { t, locale } = useI18n()

const visible = ref(false)
const miniKey = ref(0)
const previewScale = ref<number | null>(null)
const miniRef = ref<{ focusTitle: () => Promise<void> } | null>(null)
const bubbles = ref<Array<{
  id: string
  source: string
  title: string
  subtitle: string
  urgent: boolean
}>>([])
const bubbleHeight = ref(120)

const currentBubble = computed(() => bubbles.value[0])
const currentScale = computed(() =>
  previewScale.value ?? (view === 'todo-mini' ? uiSettings.value.miniScale : uiSettings.value.panelScale),
)

function localized(key: string, fallback: string) {
  const value = t(key)
  return value === key ? fallback : value
}

async function show() {
  if (view === 'runtime') return
  if (view === 'bubble') {
    if (!currentBubble.value) return
  }
  visible.value = false
  await nextTick()
  await showAt(view as 'todo' | 'todo-mini' | 'bubble', bubbleHeight.value + 16, currentScale.value)
  visible.value = true
  if (view === 'todo-mini') {
    await focusMini()
  }
}

async function focusMini() {
  await nextTick()
  await miniRef.value?.focusTitle()
}

async function persistScale(value: number) {
  if (view !== 'todo' && view !== 'todo-mini') return
  await setUiScale(view as 'todo' | 'todo-mini', value)
}

async function changeScale(value: number) {
  previewScale.value = null
  try {
    await persistScale(value)
  } catch {
    return
  }
  if (visible.value) {
    await showAt(view as 'todo' | 'todo-mini' | 'bubble', bubbleHeight.value + 16, value)
    if (view === 'todo-mini') {
      await focusMini()
    }
  }
}

function previewScaleValue(value: number) {
  previewScale.value = value
}

async function commitScale(value: number) {
  const previous = view === 'todo-mini' ? uiSettings.value.miniScale : uiSettings.value.panelScale
  try {
    await persistScale(value)
  } catch {
    previewScale.value = null
    if (visible.value) {
      await showAt(view as 'todo' | 'todo-mini' | 'bubble', bubbleHeight.value + 16, previous)
    }
  } finally {
    previewScale.value = null
  }
}

async function close() {
  visible.value = false
  await new Promise((resolve) => setTimeout(resolve, 200))
  await hideCurrent()
}

async function hydrate() {
  try {
    await refresh()
  } catch {
    // The next state event retries; a transient read must not blank the panel.
  }
}

async function handleCreate(title: string, dueDate: number | undefined, priority: TodoPriority) {
  await createTodo(title, dueDate, priority)
}

async function handleToggle(id: string) {
  await toggleTodo(id)
}

async function handleRemove(id: string) {
  await removeTodo(id)
}

async function handlePriority(id: string, priority: TodoPriority) {
  await changePriority(id, priority)
}

function addBubble(bubble: {
  id: string
  source: string
  title: string
  subtitle: string
  urgent: boolean
}) {
  if (bubbles.value.some((item) => item.id === bubble.id)) return
  bubbles.value.push(bubble)
  void show()
}

async function bubbleResize(height: number) {
  bubbleHeight.value = Math.max(80, Math.round(height))
  if (visible.value && view === 'bubble') {
    await showAt('bubble', bubbleHeight.value + 16)
  }
}

async function bubbleAction() {
  await invoke('open_panel')
  bubbles.value.shift()
  if (bubbles.value.length) {
    await show()
  } else {
    await close()
  }
}

async function bubbleClose() {
  bubbles.value.shift()
  if (bubbles.value.length) {
    await show()
  } else {
    await close()
  }
}

async function sendDesktopNotification(title: string, body: string) {
  try {
    if (!(await isPermissionGranted())) {
      const permission = await requestPermission()
      if (permission !== 'granted') return
    }
    sendNotification({ title, body })
  } catch {
    // The bubble remains the fallback when the OS notification path is unavailable.
  }
}

onMounted(async () => {
  const initialLocale = await invoke<string>('get_locale')
  locale.value = initialLocale

  await listen('companion://event', async ({ payload }: { payload: any }) => {
    switch (payload?.type) {
      case 'open':
        if (payload.kind === view) {
          if (view === 'todo-mini') miniKey.value += 1
          await show()
        }
        break
      case 'state_changed':
        await hydrate()
        break
      case 'locale':
        locale.value = payload.locale
        break
      case 'bubble':
        if (view === 'bubble') addBubble(payload.bubble)
        break
      case 'notify':
        if (view === 'runtime') void sendDesktopNotification(payload.title, payload.body)
        break
      default:
        break
    }
  })

  await hydrate()
  if (view === 'runtime') return

  const pending = await invoke<string | null>('take_pending_open', { view })
  if (pending === view) {
    if (view === 'todo-mini') miniKey.value += 1
    await show()
  }

  if (view === 'bubble') {
    const pendingBubbles = await invoke<Array<any>>('take_pending_bubbles')
    for (const bubble of pendingBubbles) addBubble(bubble)
    if (await currentVisible()) {
      // A bubble window restored by the host still needs the current geometry.
      await show()
    }
  }
})

const source = computed(() => currentBubble.value?.source ?? localized('plugins.todo.labels.bubbleSource', 'Todo due'))
const title = computed(() => currentBubble.value?.title ?? '')
const subtitle = computed(() => currentBubble.value?.subtitle ?? localized('plugins.todo.labels.bubbleDueBody', 'is due now'))
</script>

<template>
  <div
    v-if="view === 'todo' || view === 'todo-mini'"
    class="todo-handdrawn todo-root fade"
    :class="{ 'fade-shown': visible }"
  >
    <TodoPanel
      v-if="view === 'todo'"
      :todos="todos"
      :scale="currentScale"
      @change-priority="handlePriority"
      @close="close"
      @create="handleCreate"
      @remove="handleRemove"
      @scale-change="changeScale"
      @scale-commit="commitScale"
      @scale-preview="previewScaleValue"
      @toggle="handleToggle"
    />
    <MiniInput
      v-else
      ref="miniRef"
      :key="miniKey"
      :scale="currentScale"
      @close="close"
      @create="handleCreate"
      @scale-change="changeScale"
      @scale-commit="commitScale"
      @scale-preview="previewScaleValue"
    />
  </div>
  <div
    v-else-if="view === 'bubble'"
    class="bubble-handdrawn bubble-root fade"
    :class="{ 'fade-shown': visible }"
  >
    <Bubble
      v-if="currentBubble"
      type="todo"
      :source="source"
      :title="title"
      :subtitle="subtitle"
      :urgent="currentBubble.urgent"
      :width="360"
      @action="bubbleAction"
      @close="bubbleClose"
      @resize="bubbleResize"
    />
  </div>
</template>

<style>
html,
body,
#app {
  background: transparent !important;
  margin: 0;
  width: 100%;
  height: 100%;
  overflow: hidden;
}

* {
  box-sizing: border-box;
}

.todo-root {
  width: 100vw;
  height: 100vh;
  padding: 12px;
  overflow: hidden;
}

.bubble-root {
  width: 100vw;
  height: 100vh;
  overflow: hidden;
  display: flex;
  justify-content: center;
}

.fade {
  opacity: 0;
  transition: opacity 200ms ease;
}

.fade.fade-shown {
  opacity: 1;
}

body.is-resizing-window,
body.is-resizing-window * {
  cursor: nwse-resize !important;
  user-select: none !important;
}
</style>
