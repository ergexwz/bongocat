<template>
  <div class="counter-shell">
    <div v-if="visible" class="counter">
      <span class="counter-text" :style="{ fontSize }">{{ text }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { computed, onMounted, ref } from 'vue'

interface CounterRender {
  mode: 'hidden' | 'standard' | 'gamepad'
  text: string
  scalePercent: number
}

const render = ref<CounterRender | null>(null)

const text = computed(() => render.value?.text ?? '')
const visible = computed(() => !!text.value && render.value?.mode !== 'hidden')

const fontSize = computed(() => {
  const scale = (render.value?.scalePercent ?? 100) / 100
  const characters = text.value.length
  const mode = render.value?.mode
  let base = 58
  if (mode === 'gamepad') {
    base = characters > 8 ? 34 : 44
  } else if (characters > 9) {
    base = 30
  } else if (characters > 7) {
    base = 34
  } else if (characters > 5) {
    base = 42
  } else if (characters > 4) {
    base = 50
  }
  return `${Math.max(9, base * scale)}px`
})

async function refresh() {
  try {
    render.value = await invoke<CounterRender>('cyberwork_render')
  } catch {
    // The next state event retries; a transient read must not flash a stale counter.
  }
}

function onVisibilityChange() {
  if (document.visibilityState === 'visible') {
    void refresh()
  }
}

onMounted(async () => {
  await refresh()
  await listen('companion://event', ({ payload }: { payload: any }) => {
    switch (payload?.type) {
      case 'state_changed':
        void refresh()
        break
      case 'locale':
        // The counter text is numeric; the locale does not change its glyphs.
        break
      default:
        break
    }
  })
  document.addEventListener('visibilitychange', onVisibilityChange)
})
</script>

<style scoped>
.counter-shell {
  width: 100vw;
  height: 100vh;
  background: transparent;
  overflow: hidden;
}

.counter {
  width: 100%;
  height: 100%;
  background-image: url('/counter/counter_background.png');
  background-position: center;
  background-repeat: no-repeat;
  background-size: 100% 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 10% 9%;
}

.counter-text {
  font-family: 'Handwriting851', 'Segoe UI', sans-serif;
  font-weight: 400;
  line-height: 1;
  color: #101010;
  white-space: nowrap;
  text-align: center;
  letter-spacing: 0.01em;
}
</style>
