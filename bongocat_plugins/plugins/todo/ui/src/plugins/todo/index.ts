export type TodoPriority = 'low' | 'medium' | 'high'

export interface Todo {
  id: string
  title: string
  completed: boolean
  priority: TodoPriority
  dueDate?: number
  createdAt: number
  updatedAt: number
  notifiedDueAt?: number
}
