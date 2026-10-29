import { invoke } from '@tauri-apps/api/core'
import { ref } from 'vue'

import type { Todo, TodoPriority } from '@/plugins/todo'

export interface TodoUiSettings {
  panelScale: number
  miniScale: number
}

export interface TodoSnapshot {
  tasks: Todo[]
  error: string | null
  ui: TodoUiSettings
}

const DEFAULT_UI: TodoUiSettings = {
  panelScale: 1,
  miniScale: 1,
}

export const todos = ref<Todo[]>([])
export const snapshotError = ref<string | null>(null)
export const uiSettings = ref<TodoUiSettings>({ ...DEFAULT_UI })

async function applySnapshot(snapshot: TodoSnapshot) {
  todos.value = snapshot.tasks
  snapshotError.value = snapshot.error
  uiSettings.value = snapshot.ui ?? { ...DEFAULT_UI }
}

export async function refresh() {
  await applySnapshot(await invoke<TodoSnapshot>('todo_snapshot'))
}

export async function createTodo(title: string, dueDate: number | undefined, priority: TodoPriority) {
  const snapshot = await invoke<TodoSnapshot>('todo_create', {
    title,
    dueDate: dueDate ?? null,
    priority,
  })
  await applySnapshot(snapshot)
}

export async function toggleTodo(id: string) {
  await applySnapshot(await invoke<TodoSnapshot>('todo_toggle', { id }))
}

export async function removeTodo(id: string) {
  await applySnapshot(await invoke<TodoSnapshot>('todo_delete', { id }))
}

export async function changePriority(id: string, priority: TodoPriority) {
  await applySnapshot(await invoke<TodoSnapshot>('todo_set_priority', { id, priority }))
}

export async function setUiScale(view: 'todo' | 'todo-mini', scale: number) {
  const snapshot = await invoke<TodoSnapshot>('todo_set_ui_scale', { view, scale })
  await applySnapshot(snapshot)
}
