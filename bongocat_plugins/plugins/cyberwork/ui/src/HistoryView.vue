<template>
  <div class="history-root">
    <div class="paper">
      <header class="paper-head" data-tauri-drag-region>
        <div class="ink-dot" aria-hidden="true"></div>
        <h1 class="title" data-tauri-drag-region>{{ t('cyberwork.title') }}</h1>
        <button class="close" type="button" :title="t('cyberwork.close')" @click="close">
          ×
        </button>
      </header>

      <div class="paper-body">
        <div v-if="errorText" class="error-banner">{{ errorText }}</div>

        <section class="metric">
          <div class="metric-head">
            <span class="metric-title">{{ t('cyberwork.taps') }}</span>
            <span class="metric-total">{{ formatCount(snapshot?.taps.total ?? 0) }}</span>
          </div>
          <div class="period-grid">
            <div class="period-card">
              <span class="period-label">{{ t('cyberwork.today') }}</span>
              <strong class="period-value">{{ formatCount(snapshot?.taps.day ?? 0) }}</strong>
              <span class="period-from">{{ fromLabel(snapshot?.dayStart) }}</span>
            </div>
            <div class="period-card">
              <span class="period-label">{{ t('cyberwork.week') }}</span>
              <strong class="period-value">{{ formatCount(snapshot?.taps.week ?? 0) }}</strong>
              <span class="period-from">{{ fromLabel(snapshot?.weekStart) }}</span>
            </div>
            <div class="period-card">
              <span class="period-label">{{ t('cyberwork.month') }}</span>
              <strong class="period-value">{{ formatCount(snapshot?.taps.month ?? 0) }}</strong>
              <span class="period-from">{{ fromLabel(snapshot?.monthStart) }}</span>
            </div>
            <div class="period-card total">
              <span class="period-label">{{ t('cyberwork.total') }}</span>
              <strong class="period-value">{{ formatCount(snapshot?.taps.total ?? 0) }}</strong>
            </div>
          </div>
        </section>

        <div class="divider" aria-hidden="true"></div>

        <section class="metric">
          <div class="metric-head">
            <span class="metric-title">{{ t('cyberwork.game') }}</span>
            <span class="metric-total">{{ formatDuration(snapshot?.gameSeconds.total ?? 0) }}</span>
          </div>
          <div class="period-grid">
            <div class="period-card">
              <span class="period-label">{{ t('cyberwork.today') }}</span>
              <strong class="period-value">{{ formatDuration(snapshot?.gameSeconds.day ?? 0) }}</strong>
              <span class="period-from">{{ fromLabel(snapshot?.dayStart) }}</span>
            </div>
            <div class="period-card">
              <span class="period-label">{{ t('cyberwork.week') }}</span>
              <strong class="period-value">{{ formatDuration(snapshot?.gameSeconds.week ?? 0) }}</strong>
              <span class="period-from">{{ fromLabel(snapshot?.weekStart) }}</span>
            </div>
            <div class="period-card">
              <span class="period-label">{{ t('cyberwork.month') }}</span>
              <strong class="period-value">{{ formatDuration(snapshot?.gameSeconds.month ?? 0) }}</strong>
              <span class="period-from">{{ fromLabel(snapshot?.monthStart) }}</span>
            </div>
            <div class="period-card total">
              <span class="period-label">{{ t('cyberwork.total') }}</span>
              <strong class="period-value">{{ formatDuration(snapshot?.gameSeconds.total ?? 0) }}</strong>
            </div>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

interface PeriodValues {
  day: number
  week: number
  month: number
  total: number
}

interface HistorySnapshot {
  taps: PeriodValues
  gameSeconds: PeriodValues
  dayStart: string
  weekStart: string
  monthStart: string
  scalePercent: number
  locale: string
  error: string | null
}

const { t, locale } = useI18n()
const snapshot = ref<HistorySnapshot | null>(null)
let refreshTimer: number | null = null

const errorText = computed(() => {
  const code = snapshot.value?.error
  if (!code) return ''
  const key = `cyberwork.error.${code}`
  const text = t(key)
  return text === key ? t('cyberwork.error.unknown') : text
})

function fromLabel(date: string | undefined) {
  if (!date) return ''
  return t('cyberwork.from', { date })
}

function formatCount(value: number) {
  return new Intl.NumberFormat(locale.value).format(value)
}

function formatDuration(seconds: number) {
  if (!Number.isFinite(seconds) || seconds <= 0) return t('cyberwork.duration.zero')
  const whole = Math.floor(seconds)
  const days = Math.floor(whole / 86400)
  const hours = Math.floor((whole % 86400) / 3600)
  const minutes = Math.floor((whole % 3600) / 60)
  const rest = whole % 60
  const d = t('cyberwork.duration.day')
  const h = t('cyberwork.duration.hour')
  const m = t('cyberwork.duration.minute')
  const s = t('cyberwork.duration.second')
  if (days > 0) return `${days}${d} ${hours}${h}`
  if (hours > 0) return `${hours}${h} ${minutes}${m}`
  if (minutes > 0) return `${minutes}${m} ${rest}${s}`
  return `${rest}${s}`
}

async function refresh() {
  try {
    snapshot.value = await invoke<HistorySnapshot>('cyberwork_history')
  } catch {
    // Keep the previous snapshot; the next open or interval retries.
  }
}

async function close() {
  await getCurrentWindow().hide().catch(() => undefined)
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    void close()
  }
}

function onVisibilityChange() {
  if (document.visibilityState === 'visible') {
    void refresh()
  }
}

onMounted(async () => {
  try {
    locale.value = await invoke<string>('get_locale')
  } catch {
    // The English fallback is already selected.
  }
  await refresh()
  const pending = await invoke<boolean>('take_pending_open').catch(() => false)
  if (pending) {
    await refresh()
  }
  await listen('companion://event', ({ payload }: { payload: any }) => {
    switch (payload?.type) {
      case 'open':
        if (payload.kind === 'history') {
          void refresh()
        }
        break
      case 'locale':
        locale.value = payload.locale
        break
      case 'state_changed':
        if (document.visibilityState === 'visible') {
          void refresh()
        }
        break
      default:
        break
    }
  })
  document.addEventListener('visibilitychange', onVisibilityChange)
  document.addEventListener('keydown', onKeydown)
  refreshTimer = window.setInterval(() => {
    if (document.visibilityState === 'visible') {
      void refresh()
    }
  }, 2000)
})

onBeforeUnmount(() => {
  document.removeEventListener('visibilitychange', onVisibilityChange)
  document.removeEventListener('keydown', onKeydown)
  if (refreshTimer !== null) {
    window.clearInterval(refreshTimer)
  }
})
</script>

<style scoped>
.history-root {
  width: 100vw;
  height: 100vh;
  padding: 12px;
  background: transparent;
  overflow: hidden;
}

.paper {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  color: #26221d;
  background:
    radial-gradient(circle at 18% 8%, rgba(255, 255, 255, 0.92), transparent 42%),
    linear-gradient(153deg, #fefbf3 0%, #f8f0df 100%);
  border: 3px solid #29251f;
  border-radius: 23px 19px 26px 20px;
  box-shadow:
    0 6px 0 rgba(41, 37, 31, 0.9),
    0 14px 26px rgba(30, 24, 15, 0.18);
}

.paper-head {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 18px 20px 10px;
  -webkit-app-region: drag;
  user-select: none;
}

.ink-dot {
  width: 12px;
  height: 12px;
  border-radius: 50% 44% 55% 46%;
  background: #f19aae;
  box-shadow: inset 0 -2px 0 rgba(41, 37, 31, 0.18);
}

.title {
  flex: 1;
  margin: 0;
  font-family: 'Handwriting851', 'Segoe UI', sans-serif;
  font-size: 26px;
  font-weight: 400;
  letter-spacing: 0.03em;
  color: #29251f;
}

.close {
  flex: 0 0 auto;
  width: 34px;
  height: 34px;
  border: 2px solid #29251f;
  border-radius: 50% 46% 52% 45%;
  background: #fdf8ec;
  color: #29251f;
  font-family: 'Segoe UI', sans-serif;
  font-size: 21px;
  line-height: 1;
  padding: 0 0 3px;
  -webkit-app-region: no-drag;
}

.close:hover {
  background: #f7dfe4;
}

.paper-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 4px 20px 20px;
  scrollbar-width: thin;
}

.error-banner {
  margin: 2px 0 12px;
  padding: 9px 12px;
  border: 2px dashed #b35a63;
  border-radius: 10px 8px 11px 9px;
  background: #fdecef;
  color: #7c2f38;
  font-size: 14px;
  line-height: 1.45;
}

.metric {
  padding: 4px 0 8px;
}

.metric-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
  margin: 0 2px 9px;
}

.metric-title {
  font-size: 21px;
  letter-spacing: 0.02em;
}

.metric-total {
  font-size: 16px;
  color: #6e6252;
}

.period-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
}

.period-card {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  padding: 8px 10px 8px;
  border: 2px solid #29251f;
  border-radius: 13px 10px 15px 11px;
  background: rgba(255, 252, 244, 0.84);
  box-shadow: 0 3px 0 rgba(41, 37, 31, 0.14);
}

.period-card.total {
  grid-column: 1 / -1;
  background: #f6e9d8;
}

.period-label {
  font-size: 13px;
  color: #6e6252;
  letter-spacing: 0.03em;
}

.period-value {
  font-family: 'Handwriting851', 'Segoe UI', sans-serif;
  font-size: 23px;
  font-weight: 400;
  line-height: 1.15;
  color: #29251f;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.period-from {
  font-size: 12px;
  color: #8d7f6c;
}

.divider {
  height: 8px;
  margin: 2px 4px 10px;
  background: radial-gradient(circle at 4px 4px, #29251f 1.6px, transparent 1.8px) repeat-x;
  background-size: 14px 8px;
  opacity: 0.24;
}
</style>
