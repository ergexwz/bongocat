import type { TodoPriority } from '@/plugins/todo'

/**
 * 优先级三档顺序（低 → 中 → 高），用于点击循环切换。
 * 导出供 PriorityPicker 渲染三档、TodoItem 循环切换共享同一顺序。
 */
export const PRIORITIES: TodoPriority[] = ['low', 'medium', 'high']

/** 优先级在 PRIORITIES 中的下标（0/1/2）。 */
export function priorityIndex(p: TodoPriority): number {
  return PRIORITIES.indexOf(p)
}

/**
 * 点击循环切换优先级：low → medium → high → low。
 * 用于 TodoItem 上点击已有 todo 的墨点改优先级。
 */
export function nextPriority(current: TodoPriority): TodoPriority {
  return PRIORITIES[(priorityIndex(current) + 1) % PRIORITIES.length]!
}
