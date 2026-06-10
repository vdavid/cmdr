import type { Component } from 'svelte'

export type ToastLevel = 'default' | 'info' | 'success' | 'warn' | 'error'
export type ToastDismissal = 'transient' | 'persistent'

/** Content can be a plain string (rendered as text) or a Svelte component (mounted as-is). */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type ToastContent = string | Component<any>

/**
 * Grace period applied by `ToastItem` after the pointer leaves a transient
 * toast that has already passed its natural `timeoutMs`. Catches accidental
 * cursor exits so the toast doesn't vanish the instant the mouse drifts off.
 *
 * Exported so a future tuning lives in one place; `ToastItem.svelte` imports
 * this constant rather than hard-coding the value.
 */
export const HOVER_LEAVE_GRACE_MS = 2000

/** Default per-group cap when `toastGroup` is set but `maxInGroup` is not. */
export const DEFAULT_MAX_IN_GROUP = 5

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
  /**
   * Optional group key. Toasts that share a `toastGroup` count against a per-group cap
   * (`maxInGroup`, default 5) BEFORE the global cap of 5 applies. When a new grouped toast
   * arrives and the group is full, the oldest transient toast in that same group is evicted
   * first — even if the global cap hasn't been hit. Persistent toasts in the group still
   * block group-level eviction (mirrors how persistent toasts block global eviction).
   *
   * Use this for streams of homogeneous notifications (downloads detected, share-disconnect
   * events) so a burst can't push unrelated toasts off the screen.
   */
  toastGroup?: string
  /**
   * Per-group cap. Defaults to {@link DEFAULT_MAX_IN_GROUP} (5) when `toastGroup` is set,
   * ignored otherwise. The group cap cannot exceed the global cap by design: if you set it
   * higher, the global cap kicks in first.
   */
  maxInGroup?: number
  /**
   * Props forwarded to a component-shaped `content`. Ignored for string content.
   *
   * The toast ID is appended to the props object under the `toastId` key so the
   * content component can self-dismiss without a module-state bridge.
   * (Earlier component-content toasts used a module-state setter pattern for
   * their single callback; once a toast carries structured data per instance
   * — a burst of downloads each with different filenames — props-forwarding
   * is the right shape, since module state would clobber across consecutive
   * toasts.)
   */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- Svelte component prop maps are heterogenous
  props?: Record<string, any>
  /**
   * Optional per-toast max width in px, overriding the default 360. Use for a
   * toast whose content needs more room (for example one carrying a wide
   * illustration). Other toasts keep the 360 default; the container hugs the
   * right edge, so a wider toast just extends leftward. Capped by the
   * container's own max-width.
   */
  widthPx?: number
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
  toastGroup?: string
  maxInGroup?: number
  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- see ToastOptions.props
  props?: Record<string, any>
  widthPx?: number
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

function findOldestTransientIndexInGroup(group: string): number {
  return toasts.findIndex((t) => t.dismissal === 'transient' && t.toastGroup === group)
}

function countInGroup(group: string): number {
  return toasts.reduce((n, t) => (t.toastGroup === group ? n + 1 : n), 0)
}

function replaceExisting(index: number, content: ToastContent, level: ToastLevel, options?: ToastOptions): void {
  toasts[index].content = content
  toasts[index].level = level
  toasts[index].closeTooltip = options?.closeTooltip
  toasts[index].onDismiss = options?.onDismiss
}

/**
 * Make room for an incoming toast, applying group-aware eviction first and the
 * global cap second.
 *
 * Order of operations:
 *
 *  1. If `toastGroup` is set and the group is already at `maxInGroup`, evict
 *     the oldest *transient* toast in that same group. If only persistent
 *     toasts fill the group, return `false` — the new toast is silently dropped
 *     (consistent with the global-cap behavior when all 5 slots are persistent).
 *     Group eviction can also free a global slot when both caps are hit.
 *  2. If we're still at the global cap of 5 after step 1, evict the oldest
 *     transient toast globally. If all are persistent, return `false`.
 *
 * Returns `true` when there's room to push the new toast.
 */
function makeRoomForNewToast(toastGroup: string | undefined, maxInGroup: number): boolean {
  if (toastGroup !== undefined && countInGroup(toastGroup) >= maxInGroup) {
    const oldestInGroup = findOldestTransientIndexInGroup(toastGroup)
    if (oldestInGroup === -1) {
      // Group is full of persistent toasts: drop the new one.
      return false
    }
    removeAtIndex(oldestInGroup)
  }

  if (toasts.length < maxVisibleToasts) return true

  const oldestTransientIndex = findOldestTransientIndex()
  if (oldestTransientIndex === -1) {
    // All slots are persistent: drop the new toast.
    return false
  }
  removeAtIndex(oldestTransientIndex)
  return true
}

interface ResolvedToastOptions {
  level: ToastLevel
  dismissal: ToastDismissal
  timeoutMs: number
  id: string
  toastGroup: string | undefined
  maxInGroup: number | undefined
}

function resolveOptions(options: ToastOptions | undefined): ResolvedToastOptions {
  const dismissal = options?.dismissal ?? 'transient'
  const toastGroup = options?.toastGroup
  return {
    level: options?.level ?? 'default',
    dismissal,
    timeoutMs: dismissal === 'persistent' ? 0 : (options?.timeoutMs ?? 4000),
    id: options?.id ?? crypto.randomUUID(),
    toastGroup,
    maxInGroup: toastGroup === undefined ? undefined : (options?.maxInGroup ?? DEFAULT_MAX_IN_GROUP),
  }
}

export function addToast(content: ToastContent, options?: ToastOptions): string {
  const resolved = resolveOptions(options)
  const { id, level, dismissal, timeoutMs, toastGroup, maxInGroup } = resolved

  // Dedup: replace content and level in place if ID already exists.
  const existingIndex = findIndexById(id)
  if (existingIndex !== -1) {
    replaceExisting(existingIndex, content, level, options)
    return id
  }

  if (!makeRoomForNewToast(toastGroup, maxInGroup ?? maxVisibleToasts)) return id

  toasts.push({
    id,
    content,
    level,
    dismissal,
    timeoutMs,
    createdAt: Date.now(),
    closeTooltip: options?.closeTooltip,
    onDismiss: options?.onDismiss,
    toastGroup,
    maxInGroup,
    props: options?.props,
    widthPx: options?.widthPx,
  })
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
