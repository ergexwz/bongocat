<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import HandClock from '../HandClock/index.vue'

/**
 * 手写日期时间输入（HandDateInput / T6）。
 *
 * 抽自 T5 迷你输入窗的「5 个手写数字框」模式（年/月/日 + 时:分），供主面板新建复用。
 * 不用 native `<input type="date">`（WebView2 占位符乱码）也不用 antd DatePicker（弹层溢出 +
 * 企业蓝皮肤），皮肤完全手绘可控（见 CONTEXT.md「已踩坑清单 · native date input 在 WebView2 不可用」）。
 *
 * emit `change`：每次输入变化时把计算出的 timestamp（未填完整日期时为 undefined）上报父组件。
 * 父组件拿这个值作为 createTodo 的 dueDate 入参。
 *
 * 「focus 自动填充当前时间」：任一框获得焦点且全部为空时，自动填入当前系统时间的
 * 年/月/日/时/分，并全选当前框文字——用户点一下就有完整时间，只需微调个别数字，
 * 不用从零敲一长串。已有值时不覆盖（不破坏用户已填的内容）。
 *
 * 拖动：行挂 data-tauri-drag-region，图标/分隔符 pointer-events:none 透传，input 天然豁免。
 *
 * @see src/plugins/todo/components/MiniInput/index.vue T5 原始模式来源
 */
const emit = defineEmits<{
  change: [dueDate: number | undefined]
  enter: []
}>()

const { t } = useI18n()

// ── 日期时间输入（纯数字字符串，空串=未填）──
const year = ref('')
const month = ref('')
const day = ref('')
const hour = ref('')
const minute = ref('')

/** 日期部分（年月日）是否完整填写。 */
const hasDate = computed(() => year.value !== '' && month.value !== '' && day.value !== '')
/** 时间部分（时分）是否完整填写；未填则默认 09:00。 */
const hasTime = computed(() => hour.value !== '' && minute.value !== '')

/**
 * 提交用 timestamp：日期必填（年月日都填），时间可选（未填默认 09:00）。
 * 日期/时间都未填 → undefined（无到期日的普通 todo）。
 * 用 new Date(y, m-1, d, h, m) 构造本地时间，避开 UTC 偏移。
 */
const dueTimestamp = computed<number | undefined>(() => {
  if (!hasDate.value)
    return undefined
  const y = Number(year.value)
  const m = Number(month.value)
  const d = Number(day.value)
  // 合法性校验：用 Date 构造后回读对比，非法日期（如 2/30）Date 会溢出到下月。
  const probe = new Date(y, m - 1, d)
  if (probe.getFullYear() !== y || probe.getMonth() !== m - 1 || probe.getDate() !== d)
    return undefined
  const h = hasTime.value ? Number(hour.value) : 9
  const min = hasTime.value ? Number(minute.value) : 0
  return new Date(y, m - 1, d, h, min, 0, 0).getTime()
})

/** emit 变化（input 事件每次触发后调）。 */
function emitChange() {
  emit('change', dueTimestamp.value)
}

/**
 * 任一框 focus 时：若全部为空，自动填入当前系统时间（年/月/日/时/分），
 * 然后全选当前框文字（方便整体覆盖或微调）。已有值时不覆盖。
 *
 * 这样用户点一下任意框即得完整时间，只改需要变的那几位即可，免去敲一长串数字。
 */
function onFocusFillNow(e: FocusEvent) {
  const allEmpty = !year.value && !month.value && !day.value && !hour.value && !minute.value
  if (allEmpty) {
    const now = new Date()
    year.value = String(now.getFullYear())
    month.value = String(now.getMonth() + 1)
    day.value = String(now.getDate())
    hour.value = String(now.getHours())
    minute.value = String(now.getMinutes())
    emitChange()
  }
  // 全选当前框文字（无论是否刚填充，都方便整体覆盖或定位编辑）。
  const el = e.target as HTMLInputElement
  el.select()
}

/** 数字输入框：限制只能输数字，并 clamp 到合法范围。 */
function clampInt(value: string, min: number, max: number): string {
  if (value === '')
    return ''
  const n = Number(value)
  if (Number.isNaN(n))
    return ''
  return String(Math.max(min, Math.min(n, max)))
}

function onYearInput() {
  year.value = year.value.replace(/\D/g, '').slice(0, 4)
  emitChange()
}

function onMonthInput() {
  month.value = clampInt(month.value.replace(/\D/g, ''), 1, 12)
  emitChange()
}

function onDayInput() {
  day.value = clampInt(day.value.replace(/\D/g, ''), 1, 31)
  emitChange()
}

function onHourInput() {
  hour.value = clampInt(hour.value.replace(/\D/g, ''), 0, 23)
  emitChange()
}

function onMinuteInput() {
  minute.value = clampInt(minute.value.replace(/\D/g, ''), 0, 59)
  emitChange()
}

defineExpose({ reset: () => {
  year.value = ''
  month.value = ''
  day.value = ''
  hour.value = ''
  minute.value = ''
} })
</script>

<template>
  <div
    class="date-row"
    data-tauri-drag-region
  >
    <span class="icon-deco">
      <HandClock />
    </span>
    <input
      v-model="year"
      class="num-input num-year"
      inputmode="numeric"
      :placeholder="t('plugins.todo.labels.miniYearPh')"
      :title="t('plugins.todo.labels.miniYearLabel')"
      type="text"
      @focus="onFocusFillNow"
      @input="onYearInput"
      @keydown.enter.prevent="emit('enter')"
    >
    <span class="num-sep">-</span>
    <input
      v-model="month"
      class="num-input num-md"
      inputmode="numeric"
      :placeholder="t('plugins.todo.labels.miniMonthPh')"
      :title="t('plugins.todo.labels.miniMonthLabel')"
      type="text"
      @focus="onFocusFillNow"
      @input="onMonthInput"
      @keydown.enter.prevent="emit('enter')"
    >
    <span class="num-sep">-</span>
    <input
      v-model="day"
      class="num-input num-md"
      inputmode="numeric"
      :placeholder="t('plugins.todo.labels.miniDayPh')"
      :title="t('plugins.todo.labels.miniDayLabel')"
      type="text"
      @focus="onFocusFillNow"
      @input="onDayInput"
      @keydown.enter.prevent="emit('enter')"
    >
    <span class="date-gap" />
    <input
      v-model="hour"
      class="num-input num-md"
      inputmode="numeric"
      :placeholder="t('plugins.todo.labels.miniHourPh')"
      :title="t('plugins.todo.labels.miniHourLabel')"
      type="text"
      @focus="onFocusFillNow"
      @input="onHourInput"
      @keydown.enter.prevent="emit('enter')"
    >
    <span class="num-sep">:</span>
    <input
      v-model="minute"
      class="num-input num-md"
      inputmode="numeric"
      :placeholder="t('plugins.todo.labels.miniMinutePh')"
      :title="t('plugins.todo.labels.miniMinuteLabel')"
      type="text"
      @focus="onFocusFillNow"
      @input="onMinuteInput"
      @keydown.enter.prevent="emit('enter')"
    >
  </div>
</template>

<style scoped>
.date-row {
  display: flex;
  align-items: center;
  gap: 5px;
}

/* 时钟图标：装饰，pointer-events:none 透传给行 drag-region。 */
.icon-deco {
  display: flex;
  align-items: center;
  flex-shrink: 0;
  pointer-events: none;
}

/* 手写数字输入框：年/月/日/时/分统一皮肤（851 字体/加粗/粉墨色/透明无边框）。 */
.num-input {
  padding: 0;
  background: transparent;
  border: none;
  outline: none;
  text-align: center;
  font-family: inherit;
  font-size: 14px;
  font-weight: 700;
  color: var(--ink-soft);
}

.num-input::placeholder {
  color: var(--ink-faint);
  font-weight: 500;
}

.num-input:focus {
  color: var(--ink);
}

/* 年 4 位宽，月/日/时/分 2 位宽。 */
.num-year {
  width: 38px;
}

.num-md {
  width: 22px;
}

/* 分隔符 - / :（装饰，透传 drag-region） */
.num-sep {
  flex-shrink: 0;
  font-family: inherit;
  font-size: 14px;
  font-weight: 700;
  color: var(--ink-faint);
  pointer-events: none;
}

/* 中间留白：撑开日期组与时间组的间距，行本身是 drag-region 所以这里空白也能拖。 */
.date-gap {
  flex: 1 1 auto;
  min-width: 12px;
}
</style>
