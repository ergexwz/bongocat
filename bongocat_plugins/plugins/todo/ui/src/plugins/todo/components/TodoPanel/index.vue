<script setup lang="ts">
import { computed, nextTick, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import type { Todo, TodoPriority } from '@/plugins/todo'

import HandDateInput from '../HandDateInput/index.vue'
import PaperPanel from '../PaperPanel/index.vue'
import PawLogo from '../PawLogo/index.vue'
import PriorityPicker from '../PriorityPicker/index.vue'
import ResizeGrip from '../ResizeGrip/index.vue'
import TodoItem from '../TodoItem/index.vue'
import WaveDivider from '../WaveDivider/index.vue'
import ZoomControl from '../ZoomControl/index.vue'

/**
 * Todo 面板主体（TodoPanel）。
 * - 标题区：PawLogo + 标题 + 手绘关闭 X。
 * - 待办分组：未完成 todo，带 WaveDivider 分隔。
 * - 已完成分组：completed todo。
 * - 「＋ 新建待办」按钮：点击内联展开手绘风输入框（标题 + 优先级选择 + 日期输入），回车保存。
 * - 空状态提示。
 *
 * T6 输入控件完整化：新建区加 PriorityPicker（三档默认 medium）+ HandDateInput（复用 T5 手写数字框）。
 *
 * 数据操作通过 props（todos）/emits（toggle/remove/create/changePriority）与父解耦，父负责接 store。
 */
const { todos, scale } = defineProps<{
  todos: Todo[]
  scale: number
}>()

const emit = defineEmits<{
  changePriority: [id: string, priority: TodoPriority]
  close: []
  create: [title: string, dueDate: number | undefined, priority: TodoPriority]
  remove: [id: string]
  toggle: [id: string]
  scaleChange: [scale: number]
  scalePreview: [scale: number]
  scaleCommit: [scale: number]
}>()

const { t } = useI18n()

/** 优先级权重：high > medium > low。 */
const priorityWeight: Record<TodoPriority, number> = { high: 0, medium: 1, low: 2 }

/** 排序：优先级升序 → dueDate（无则排末尾）→ createdAt（先创建在上）。 */
function sortTodos(a: Todo, b: Todo): number {
  const pDiff = priorityWeight[a.priority] - priorityWeight[b.priority]
  if (pDiff !== 0)
    return pDiff
  if (a.dueDate !== undefined && b.dueDate !== undefined)
    return a.dueDate - b.dueDate
  if (a.dueDate !== undefined)
    return -1
  if (b.dueDate !== undefined)
    return 1
  return a.createdAt - b.createdAt
}

const pendingTodos = computed(() => todos.filter(todo => !todo.completed).sort(sortTodos))
const completedTodos = computed(() => todos.filter(todo => todo.completed).sort(sortTodos))

const showAddInput = ref(false)
const newTitle = ref('')
/** 新建 todo 的优先级（默认 medium）。 */
const newPriority = ref<TodoPriority>('medium')
/** 新建 todo 的截止日期 timestamp（未填日期时 undefined）。 */
const newDueDate = ref<number | undefined>(undefined)
const addInputEl = ref<HTMLInputElement | null>(null)
const dateInputEl = ref<InstanceType<typeof HandDateInput> | null>(null)

async function focusAddInput() {
  await nextTick()
  addInputEl.value?.focus()
}

function handleAdd() {
  const title = newTitle.value.trim()
  if (!title)
    return
  emit('create', title, newDueDate.value, newPriority.value)
  resetAddForm()
}

function handleCancelAdd() {
  resetAddForm()
}

/** 日期框回车：标题有内容则提交，无内容则聚焦标题框。 */
function handleDateEnter() {
  if (newTitle.value.trim()) {
    handleAdd()
  } else {
    focusAddInput()
  }
}

function resetAddForm() {
  newTitle.value = ''
  newPriority.value = 'medium'
  newDueDate.value = undefined
  dateInputEl.value?.reset()
  showAddInput.value = false
}
</script>

<template>
  <PaperPanel>
    <!-- 标题区（可拖拽移动窗口；子元素 pointer-events:none 让拖拽穿透，按钮单独恢复） -->
    <div
      class="panel-header"
      data-tauri-drag-region
    >
      <div class="panel-title-row">
        <PawLogo />
        <span class="panel-title">{{ t('plugins.todo.labels.windowTitle') }}</span>
      </div>
      <div class="panel-actions">
        <ZoomControl
          :scale="scale"
          @change="emit('scaleChange', $event)"
        />
        <button
          class="panel-close"
          :title="t('plugins.todo.labels.closeButton')"
          type="button"
          @click="emit('close')"
        >
          <!-- 手绘 ×：两段顺滑弧线交叉，像钢笔一笔画下来带自然弧度 -->
          <svg
            fill="none"
            height="22"
            stroke="currentColor"
            stroke-linecap="round"
            stroke-width="2.5"
            viewBox="0 0 24 24"
            width="22"
          >
            <!-- 左上→右下：顺滑大弧，控制点偏离直线向左下 -->
            <path d="M 6 6 Q 9.5 13 19 19" />
            <!-- 右上→左下：顺滑大弧，控制点偏离直线向左上 -->
            <path d="M 18 6 Q 8.5 10 5 18" />
          </svg>
        </button>
      </div>
    </div>

    <!-- 中间可滚动区：列表内容超出时在此滚动，不撑破黑框 -->
    <div class="panel-scroll">
      <!-- 待办分组 -->
      <div
        v-if="pendingTodos.length"
        class="section-label"
      >
        <span>{{ t('plugins.todo.labels.sectionPending') }} · {{ pendingTodos.length }}</span>
        <WaveDivider />
      </div>

      <div class="todo-list">
        <TodoItem
          v-for="todo in pendingTodos"
          :key="todo.id"
          :todo="todo"
          @change-priority="(id, priority) => emit('changePriority', id, priority)"
          @remove="emit('remove', $event)"
          @toggle="emit('toggle', $event)"
        />
      </div>

      <!-- 已完成分组 -->
      <div
        v-if="completedTodos.length"
        class="section-label"
      >
        <span>{{ t('plugins.todo.labels.sectionCompleted') }}</span>
        <WaveDivider />
      </div>

      <div class="todo-list">
        <TodoItem
          v-for="todo in completedTodos"
          :key="todo.id"
          :todo="todo"
          @change-priority="(id, priority) => emit('changePriority', id, priority)"
          @remove="emit('remove', $event)"
          @toggle="emit('toggle', $event)"
        />
      </div>

      <!-- 空状态 -->
      <div
        v-if="!todos.length"
        class="empty-hint"
      >
        {{ t('plugins.todo.labels.emptyHint') }}
      </div>
    </div>

    <!-- 底部新建区（固定，不随列表滚动；空白区可拖拽移动窗口） -->
    <div
      class="panel-footer"
      data-tauri-drag-region
    >
      <!-- 内联展开输入：标题 + 优先级 + 日期 -->
      <div
        v-if="showAddInput"
        class="add-form"
      >
        <input
          ref="addInputEl"
          v-model="newTitle"
          class="add-input"
          :placeholder="t('plugins.todo.labels.addTodoPlaceholder')"
          type="text"
          @keydown.enter.prevent="handleAdd"
          @keydown.esc.prevent="handleCancelAdd"
        >
        <div class="add-controls">
          <PriorityPicker v-model="newPriority" />
          <HandDateInput
            ref="dateInputEl"
            @change="newDueDate = $event"
            @enter="handleDateEnter"
          />
        </div>
        <div class="add-actions">
          <button
            class="confirm-btn"
            type="button"
            @click="handleAdd"
          >
            {{ t('plugins.todo.labels.confirmAddButton') }}
          </button>
          <button
            class="cancel-btn"
            type="button"
            @click="handleCancelAdd"
          >
            {{ t('plugins.todo.labels.cancelAddButton') }}
          </button>
        </div>
      </div>
      <!-- 「＋ 新建待办」按钮 -->
      <button
        v-else
        class="add-btn"
        type="button"
        @click="(showAddInput = true), focusAddInput()"
      >
        ＋ {{ t('plugins.todo.labels.addTodoButton') }}
      </button>
    </div>

    <ResizeGrip
      class="panel-resize"
      :base-width="380"
      :base-height="560"
      :scale="scale"
      @input="emit('scalePreview', $event)"
      @commit="emit('scaleCommit', $event)"
    />
  </PaperPanel>
</template>

<style scoped>
.panel-header {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 22px;
}

.panel-actions {
  display: flex;
  align-items: center;
  gap: 4px;
}

.panel-resize {
  position: absolute;
  right: 5px;
  bottom: 5px;
}

/* 标题行也参与拖拽：子内容不拦截鼠标，让 drag-region 生效 */
.panel-title-row {
  display: flex;
  align-items: center;
  gap: 10px;
  pointer-events: none;
}

.panel-title {
  font-size: 22px;
  font-weight: 700;
  color: var(--ink);
}

.panel-close {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  margin-right: 8px;
  padding: 0;
  background: none;
  border: none;
  color: var(--ink-faint);
  cursor: pointer;
  transition: color 0.2s;
  font-family: inherit;
}

.panel-close:hover {
  color: var(--ink);
}

.section-label {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 18px 0 10px;
  font-size: 14px;
  font-weight: 600;
  color: var(--ink-soft);
}

.section-label:first-of-type {
  margin-top: 0;
}

.todo-list {
  display: flex;
  flex-direction: column;
}

/* 中间滚动区：超出黑框在此滚动，隐藏滚动条 */
.panel-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  /* 隐藏滚动条（webkit + firefox）*/
  scrollbar-width: none;
}

.panel-scroll::-webkit-scrollbar {
  display: none;
}

.empty-hint {
  padding: 32px 0;
  text-align: center;
  font-size: 14px;
  color: var(--ink-faint);
}

.panel-footer {
  flex-shrink: 0;
  display: flex;
  justify-content: center;
  margin-top: 18px;
}

.add-btn {
  padding: 12px 28px;
  background: transparent;
  border: none;
  color: var(--ink-soft);
  cursor: pointer;
  font-size: 15px;
  font-weight: 600;
  font-family: inherit;
  transition: color 0.2s;
}

.add-btn:hover {
  color: var(--pink-deep);
}

/* 新建表单：标题输入 + 优先级/日期控件 + 确认/取消按钮。 */
.add-form {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.add-input {
  width: 100%;
  padding: 10px 14px;
  background: color-mix(in srgb, var(--pink) 8%, var(--paper));
  border: none;
  outline: none;
  border-radius: 12px;
  font-family: inherit;
  font-size: 16px;
  font-weight: 600;
  color: var(--ink);
  box-sizing: border-box;
}

.add-input::placeholder {
  color: var(--ink-faint);
}

.add-input:focus {
  background: color-mix(in srgb, var(--pink) 14%, var(--paper));
}

/* 优先级选择 + 日期输入横排。 */
.add-controls {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
}

.add-actions {
  display: flex;
  gap: 10px;
  justify-content: flex-end;
}

.confirm-btn,
.cancel-btn {
  padding: 6px 16px;
  background: transparent;
  border: none;
  cursor: pointer;
  font-family: inherit;
  font-size: 14px;
  font-weight: 600;
  transition: color 0.2s;
}

.confirm-btn {
  color: var(--pink-deep);
}

.confirm-btn:hover {
  color: var(--red-ink);
}

.cancel-btn {
  color: var(--ink-faint);
}

.cancel-btn:hover {
  color: var(--ink-soft);
}
</style>
