import type { Snippet } from 'svelte'

export type ToastLevel = 'info' | 'warn' | 'error'
export type ToastDismissal = 'transient' | 'persistent'

export interface ToastOptions {
    level?: ToastLevel
    dismissal?: ToastDismissal
    /** Timeout in ms. Default 4000 for transient toasts, ignored for persistent. */
    timeoutMs?: number
    /** Optional dedup key. If a toast with this ID exists, its content and level are replaced in place. */
    id?: string
}

export interface Toast {
    id: string
    content: Snippet | string
    level: ToastLevel
    dismissal: ToastDismissal
    timeoutMs: number
    createdAt: number
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

export function addToast(content: Snippet | string, options?: ToastOptions): string {
    const level = options?.level ?? 'info'
    const dismissal = options?.dismissal ?? 'transient'
    const timeoutMs = dismissal === 'persistent' ? 0 : (options?.timeoutMs ?? 4000)
    const id = options?.id ?? crypto.randomUUID()

    // Dedup: replace content and level in place if ID already exists
    const existingIndex = findIndexById(id)
    if (existingIndex !== -1) {
        toasts[existingIndex].content = content
        toasts[existingIndex].level = level
        return id
    }

    // Enforce max visible limit
    if (toasts.length >= maxVisibleToasts) {
        const oldestTransientIndex = findOldestTransientIndex()
        if (oldestTransientIndex === -1) {
            // All 5 are persistent — drop the new toast
            return id
        }
        removeAtIndex(oldestTransientIndex)
    }

    const toast: Toast = {
        id,
        content,
        level,
        dismissal,
        timeoutMs,
        createdAt: Date.now(),
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
