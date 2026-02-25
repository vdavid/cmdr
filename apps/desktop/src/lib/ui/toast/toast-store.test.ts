import { describe, it, expect, beforeEach } from 'vitest'
import type { Snippet } from 'svelte'
import { addToast, dismissToast, dismissTransientToasts, clearAllToasts, getToasts } from './toast-store.svelte'

const dummyContent = (() => {}) as unknown as Snippet

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
        const content1 = (() => {}) as unknown as Snippet
        const content2 = (() => {}) as unknown as Snippet

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

describe('default values', () => {
    it('defaults level to info', () => {
        addToast(dummyContent)
        expect(getToasts()[0].level).toBe('info')
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
