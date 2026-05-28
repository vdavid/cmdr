import { describe, it, expect, beforeEach } from 'vitest'
import type { ToastContent } from './toast-store.svelte'
import { addToast, dismissToast, dismissTransientToasts, clearAllToasts, getToasts } from './toast-store.svelte'

const dummyContent = (() => {}) as unknown as ToastContent

beforeEach(() => {
  clearAllToasts()
})

describe('addToast', () => {
  it('adds a toast to the array', () => {
    addToast(dummyContent)
    expect(getToasts()).toHaveLength(1)
  })

  it('returns a string ID', () => {
    const id = addToast(dummyContent)
    expect(typeof id).toBe('string')
    expect(id.length).toBeGreaterThan(0)
  })

  it('deduplicates when called twice with the same custom ID', () => {
    addToast(dummyContent, { id: 'dup' })
    addToast(dummyContent, { id: 'dup' })
    expect(getToasts()).toHaveLength(1)
  })

  it('replaces content and level in place for duplicate IDs', () => {
    const content1 = (() => {}) as unknown as ToastContent
    const content2 = (() => {}) as unknown as ToastContent

    addToast(content1, { id: 'dup', level: 'info' })
    addToast(content2, { id: 'dup', level: 'error' })

    const toasts = getToasts()
    expect(toasts).toHaveLength(1)
    expect(toasts[0].content).toBe(content2)
    expect(toasts[0].level).toBe('error')
  })

  it('accepts a string as content', () => {
    addToast('Hello world')
    const toasts = getToasts()
    expect(toasts).toHaveLength(1)
    expect(toasts[0].content).toBe('Hello world')
  })
})

describe('dismissToast', () => {
  it('removes a specific toast by ID', () => {
    const id = addToast(dummyContent)
    expect(getToasts()).toHaveLength(1)

    dismissToast(id)
    expect(getToasts()).toHaveLength(0)
  })

  it('does nothing if the ID does not exist', () => {
    addToast(dummyContent)
    dismissToast('nonexistent')
    expect(getToasts()).toHaveLength(1)
  })
})

describe('dismissTransientToasts', () => {
  it('removes only transient toasts, leaving persistent ones', () => {
    addToast(dummyContent, { dismissal: 'transient' })
    addToast(dummyContent, { dismissal: 'persistent' })
    addToast(dummyContent, { dismissal: 'transient' })

    dismissTransientToasts()

    const toasts = getToasts()
    expect(toasts).toHaveLength(1)
    expect(toasts[0].dismissal).toBe('persistent')
  })
})

describe('clearAllToasts', () => {
  it('empties the array', () => {
    addToast(dummyContent)
    addToast(dummyContent)
    addToast(dummyContent)

    clearAllToasts()
    expect(getToasts()).toHaveLength(0)
  })
})

describe('closeTooltip and onDismiss', () => {
  it('stores closeTooltip on the toast when passed', () => {
    addToast(dummyContent, { closeTooltip: 'Close (work continues in background)' })
    expect(getToasts()[0].closeTooltip).toBe('Close (work continues in background)')
  })

  it('leaves closeTooltip undefined when not passed', () => {
    addToast(dummyContent)
    expect(getToasts()[0].closeTooltip).toBeUndefined()
  })

  it('stores onDismiss on the toast when passed', () => {
    const onDismiss = (): void => {}
    addToast(dummyContent, { onDismiss })
    expect(getToasts()[0].onDismiss).toBe(onDismiss)
  })

  it('replaces closeTooltip and onDismiss on dedup', () => {
    const first = (): void => {}
    const second = (): void => {}
    addToast(dummyContent, { id: 'dup', closeTooltip: 'first', onDismiss: first })
    addToast(dummyContent, { id: 'dup', closeTooltip: 'second', onDismiss: second })

    const toast = getToasts()[0]
    expect(toast.closeTooltip).toBe('second')
    expect(toast.onDismiss).toBe(second)
  })

  it('clears closeTooltip and onDismiss on dedup when subsequent call omits them', () => {
    addToast(dummyContent, { id: 'dup', closeTooltip: 'first', onDismiss: () => {} })
    addToast(dummyContent, { id: 'dup' })

    const toast = getToasts()[0]
    expect(toast.closeTooltip).toBeUndefined()
    expect(toast.onDismiss).toBeUndefined()
  })
})

describe('default values', () => {
  it('defaults level to default', () => {
    addToast(dummyContent)
    expect(getToasts()[0].level).toBe('default')
  })

  it('defaults dismissal to transient', () => {
    addToast(dummyContent)
    expect(getToasts()[0].dismissal).toBe('transient')
  })

  it('defaults timeoutMs to 4000 for transient toasts', () => {
    addToast(dummyContent)
    expect(getToasts()[0].timeoutMs).toBe(4000)
  })

  it('sets timeoutMs to 0 for persistent toasts', () => {
    addToast(dummyContent, { dismissal: 'persistent' })
    expect(getToasts()[0].timeoutMs).toBe(0)
  })
})

describe('max 5 toasts', () => {
  it('adding a 6th toast dismisses the oldest transient', () => {
    const firstId = addToast(dummyContent, { id: 'first' })
    addToast(dummyContent, { id: 'second' })
    addToast(dummyContent, { id: 'third' })
    addToast(dummyContent, { id: 'fourth' })
    addToast(dummyContent, { id: 'fifth' })

    expect(getToasts()).toHaveLength(5)

    addToast(dummyContent, { id: 'sixth' })

    expect(getToasts()).toHaveLength(5)
    expect(getToasts().find((t) => t.id === firstId)).toBeUndefined()
    expect(getToasts().find((t) => t.id === 'sixth')).toBeDefined()
  })

  it('does not remove persistent toasts when evicting for max limit', () => {
    addToast(dummyContent, { id: 'p1', dismissal: 'persistent' })
    addToast(dummyContent, { id: 't1', dismissal: 'transient' })
    addToast(dummyContent, { id: 't2', dismissal: 'transient' })
    addToast(dummyContent, { id: 't3', dismissal: 'transient' })
    addToast(dummyContent, { id: 't4', dismissal: 'transient' })

    addToast(dummyContent, { id: 'new' })

    const toasts = getToasts()
    expect(toasts).toHaveLength(5)
    expect(toasts.find((t) => t.id === 'p1')).toBeDefined()
    expect(toasts.find((t) => t.id === 't1')).toBeUndefined()
  })
})

describe('toastGroup eviction', () => {
  it('a toast without toastGroup behaves identically to before (no grouping side effects)', () => {
    addToast(dummyContent, { id: 'a1' })
    addToast(dummyContent, { id: 'a2' })
    addToast(dummyContent, { id: 'a3' })
    const toasts = getToasts()
    expect(toasts).toHaveLength(3)
    expect(toasts.every((t) => t.toastGroup === undefined)).toBe(true)
  })

  it('stores toastGroup and maxInGroup on the toast', () => {
    addToast(dummyContent, { id: 'g1', toastGroup: 'downloads' })
    const toast = getToasts()[0]
    expect(toast.toastGroup).toBe('downloads')
    expect(toast.maxInGroup).toBe(5)
  })

  it('defaults maxInGroup to 5 when toastGroup is set', () => {
    addToast(dummyContent, { id: 'g1', toastGroup: 'downloads' })
    expect(getToasts()[0].maxInGroup).toBe(5)
  })

  it('honors an explicit maxInGroup of 3', () => {
    addToast(dummyContent, { id: 'g1', toastGroup: 'downloads', maxInGroup: 3 })
    addToast(dummyContent, { id: 'g2', toastGroup: 'downloads', maxInGroup: 3 })
    addToast(dummyContent, { id: 'g3', toastGroup: 'downloads', maxInGroup: 3 })
    addToast(dummyContent, { id: 'g4', toastGroup: 'downloads', maxInGroup: 3 })

    const toasts = getToasts()
    expect(toasts).toHaveLength(3)
    expect(toasts.find((t) => t.id === 'g1')).toBeUndefined()
    expect(toasts.find((t) => t.id === 'g4')).toBeDefined()
  })

  it('with 5 of group A + 1 of group B, a new group-A toast evicts the oldest A (not B)', () => {
    addToast(dummyContent, { id: 'a1', toastGroup: 'A' })
    addToast(dummyContent, { id: 'a2', toastGroup: 'A' })
    addToast(dummyContent, { id: 'a3', toastGroup: 'A' })
    addToast(dummyContent, { id: 'a4', toastGroup: 'A' })
    addToast(dummyContent, { id: 'a5', toastGroup: 'A' })
    // Adding a 6th item (b1, group B) pushes past the global cap of 5; the
    // oldest transient (a1) is evicted by the global rule. Then a new group-A
    // toast must evict the oldest remaining A (a2), not B.
    addToast(dummyContent, { id: 'b1', toastGroup: 'B' })
    addToast(dummyContent, { id: 'a6', toastGroup: 'A' })

    const toasts = getToasts()
    expect(toasts).toHaveLength(5)
    expect(toasts.find((t) => t.id === 'b1')).toBeDefined()
    expect(toasts.find((t) => t.id === 'a6')).toBeDefined()
    expect(toasts.find((t) => t.id === 'a2')).toBeUndefined()
  })

  it('6 toasts of group A in succession: only 5 visible, oldest dropped', () => {
    addToast(dummyContent, { id: 'a1', toastGroup: 'A' })
    addToast(dummyContent, { id: 'a2', toastGroup: 'A' })
    addToast(dummyContent, { id: 'a3', toastGroup: 'A' })
    addToast(dummyContent, { id: 'a4', toastGroup: 'A' })
    addToast(dummyContent, { id: 'a5', toastGroup: 'A' })
    addToast(dummyContent, { id: 'a6', toastGroup: 'A' })

    const toasts = getToasts()
    expect(toasts).toHaveLength(5)
    expect(toasts.find((t) => t.id === 'a1')).toBeUndefined()
    expect(toasts.find((t) => t.id === 'a6')).toBeDefined()
  })

  it('persistent toast in a group blocks group eviction (new transient dropped)', () => {
    addToast(dummyContent, { id: 'p1', toastGroup: 'A', dismissal: 'persistent' })
    addToast(dummyContent, { id: 'p2', toastGroup: 'A', dismissal: 'persistent' })
    addToast(dummyContent, { id: 'p3', toastGroup: 'A', dismissal: 'persistent' })
    addToast(dummyContent, { id: 'p4', toastGroup: 'A', dismissal: 'persistent' })
    addToast(dummyContent, { id: 'p5', toastGroup: 'A', dismissal: 'persistent' })

    addToast(dummyContent, { id: 'a6', toastGroup: 'A' })

    const toasts = getToasts()
    expect(toasts).toHaveLength(5)
    expect(toasts.find((t) => t.id === 'a6')).toBeUndefined()
    expect(toasts.filter((t) => t.toastGroup === 'A')).toHaveLength(5)
  })

  it('group eviction frees a global slot when at the global cap', () => {
    addToast(dummyContent, { id: 'a1', toastGroup: 'A', maxInGroup: 4 })
    addToast(dummyContent, { id: 'a2', toastGroup: 'A', maxInGroup: 4 })
    addToast(dummyContent, { id: 'a3', toastGroup: 'A', maxInGroup: 4 })
    addToast(dummyContent, { id: 'a4', toastGroup: 'A', maxInGroup: 4 })
    addToast(dummyContent, { id: 'x1' })

    // Global cap (5) is hit AND group cap (4) is hit. Adding a5 should evict
    // the oldest A (a1) via the group rule, freeing the global slot.
    addToast(dummyContent, { id: 'a5', toastGroup: 'A', maxInGroup: 4 })

    const toasts = getToasts()
    expect(toasts).toHaveLength(5)
    expect(toasts.find((t) => t.id === 'a1')).toBeUndefined()
    expect(toasts.find((t) => t.id === 'x1')).toBeDefined()
    expect(toasts.find((t) => t.id === 'a5')).toBeDefined()
  })

  it('dismissTransientToasts still drops grouped transient toasts', () => {
    addToast(dummyContent, { id: 'g1', toastGroup: 'downloads' })
    addToast(dummyContent, { id: 'g2', toastGroup: 'downloads', dismissal: 'persistent' })
    addToast(dummyContent, { id: 'g3', toastGroup: 'downloads' })

    dismissTransientToasts()

    const toasts = getToasts()
    expect(toasts).toHaveLength(1)
    expect(toasts[0].id).toBe('g2')
  })
})

