<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

const props = withDefaults(defineProps<{
  scale: number
  compact?: boolean
  min?: number
  max?: number
}>(), {
  compact: false,
  min: 0.8,
  max: 1.5,
})

const emit = defineEmits<{
  change: [scale: number]
}>()

const { t } = useI18n()

const percent = computed(() => `${Math.round(props.scale * 100)}%`)

function clamp(value: number) {
  return Math.min(props.max, Math.max(props.min, Math.round(value * 100) / 100))
}

function step(delta: number) {
  emit('change', clamp(props.scale + delta))
}

function reset() {
  emit('change', 1)
}
</script>

<template>
  <div
    class="zoom-control"
    :class="{ compact }"
    @pointerdown.stop
  >
    <button
      class="zoom-button"
      type="button"
      :title="t('plugins.todo.labels.zoomOut')"
      :aria-label="t('plugins.todo.labels.zoomOut')"
      @click="step(-0.1)"
    >
      −
    </button>
    <button
      class="zoom-value"
      type="button"
      :title="t('plugins.todo.labels.zoomReset')"
      :aria-label="t('plugins.todo.labels.zoomReset')"
      @click="reset"
    >
      {{ percent }}
    </button>
    <button
      class="zoom-button"
      type="button"
      :title="t('plugins.todo.labels.zoomIn')"
      :aria-label="t('plugins.todo.labels.zoomIn')"
      @click="step(0.1)"
    >
      +
    </button>
  </div>
</template>

<style scoped>
.zoom-control {
  display: flex;
  align-items: center;
  gap: 1px;
  color: var(--ink-faint);
  font-family: inherit;
  user-select: none;
}

.zoom-button,
.zoom-value {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  border: none;
  background: transparent;
  color: inherit;
  font-family: inherit;
  cursor: pointer;
  transition: color 0.2s;
}

.zoom-button {
  width: 20px;
  height: 20px;
  font-size: 17px;
  line-height: 1;
}

.zoom-value {
  min-width: 38px;
  height: 20px;
  font-size: 12px;
  font-weight: 700;
}

.zoom-button:hover,
.zoom-value:hover {
  color: var(--pink-deep);
}

.compact .zoom-button {
  width: 15px;
  height: 16px;
  font-size: 14px;
}

.compact .zoom-value {
  min-width: 30px;
  height: 16px;
  font-size: 10px;
}
</style>
