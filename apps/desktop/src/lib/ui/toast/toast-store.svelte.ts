import type { Component } from 'svelte'

export type ToastLevel = 'default' | 'info' | 'success' | 'warn' | 'error'
export type ToastDismissal = 'transient' | 'persistent'

/** Content can be a plain string (rendered as text) or a Svelte component (mounted as-is). */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type ToastContent = string | Component<any>

export interface ToastOptions {
  level?: ToastLevel
  dismissal?: ToastDismissal
  /** Timeout in ms. Default 4000 for transient toasts, ignored for persistent. */
  timeoutMs?: number
  /** Optional dedup key. If a toast with this ID exists, its content and level are replaced in place. */
  id?: string
  /**
   * Optional tooltip shown on the X (close) button. Useful when the toast also contains its own
   * action buttons (for example, "Cancel"), so users can tell what each control does. When unset,
   * no tooltip is rendered on the X.
   */
  closeTooltip?: string
  /**
   * Optional callback fired when the user dismisses the toast via the X button. Auto-dismiss on
   * timeout and programmatic `dismissToast` calls do NOT trigger this: it's strictly a signal
   * that the user actively closed the toast.
   */
  onDismiss?: () => void
}

export interface Toast {
  id: string
  content: ToastContent
  level: ToastLevel
  dismissal: ToastDismissal
  timeoutMs: number
  createdAt: number
  closeTooltip?: string
  onDismiss?: () => void
}

const maxVisibleToasts = 5

const toasts = $state<Toast[]>([])

function findIndexById(id: string): number {
  return toasts.findIndex((t) => t.id === id)
}

function removeAtIndex(index: number): void {
  toasts.splice(index, 1)
}

function findOldestTransientIndex(): number {
  return toasts.findIndex((t) => t.dismissal === 'transient')
}

function replaceExisting(index: number, content: ToastContent, level: ToastLevel, options?: ToastOptions): void {
  toasts[index].content = content
  toasts[index].level = level
  toasts[index].closeTooltip = options?.closeTooltip
  toasts[index].onDismiss = options?.onDismiss
}

/** Returns true if there's room for a new toast (after evicting if needed). */
function makeRoomForNewToast(): boolean {
  if (toasts.length < maxVisibleToasts) return true
  const oldestTransientIndex = findOldestTransientIndex()
  if (oldestTransientIndex === -1) {
    // All slots are persistent: drop the new toast.
    return false
  }
  removeAtIndex(oldestTransientIndex)
  return true
}

export function addToast(content: ToastContent, options?: ToastOptions): string {
  const level = options?.level ?? 'default'
  const dismissal = options?.dismissal ?? 'transient'
  const timeoutMs = dismissal === 'persistent' ? 0 : (options?.timeoutMs ?? 4000)
  const id = options?.id ?? crypto.randomUUID()

  // Dedup: replace content and level in place if ID already exists.
  const existingIndex = findIndexById(id)
  if (existingIndex !== -1) {
    replaceExisting(existingIndex, content, level, options)
    return id
  }

  if (!makeRoomForNewToast()) return id

  const toast: Toast = {
    id,
    content,
    level,
    dismissal,
    timeoutMs,
    createdAt: Date.now(),
    closeTooltip: options?.closeTooltip,
    onDismiss: options?.onDismiss,
  }

  toasts.push(toast)
  return id
}

export function dismissToast(id: string): void {
  const index = findIndexById(id)
  if (index !== -1) {
    removeAtIndex(index)
  }
}

export function dismissTransientToasts(): void {
  for (let i = toasts.length - 1; i >= 0; i--) {
    if (toasts[i].dismissal === 'transient') {
      removeAtIndex(i)
    }
  }
}

export function clearAllToasts(): void {
  toasts.length = 0
}

export function getToasts(): Toast[] {
  return toasts
}
