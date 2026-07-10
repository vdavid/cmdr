import { describe, it, expect, vi } from 'vitest'
import { registerDialogClose, unregisterDialogClose, closeDialogById } from './dialog-close-registry'

describe('dialog-close-registry', () => {
  it('closeDialogById runs the registered close and reports success', () => {
    const close = vi.fn()
    registerDialogClose('whats-new', close)
    try {
      expect(closeDialogById('whats-new')).toBe(true)
      expect(close).toHaveBeenCalledOnce()
    } finally {
      unregisterDialogClose('whats-new', close)
    }
  })

  it('returns false for a dialog that is not registered', () => {
    expect(closeDialogById('not-open')).toBe(false)
  })

  it('unregister removes the close so a later call is a no-op', () => {
    const close = vi.fn()
    registerDialogClose('search', close)
    unregisterDialogClose('search', close)

    expect(closeDialogById('search')).toBe(false)
    expect(close).not.toHaveBeenCalled()
  })

  it('a stale unregister does not evict a newer registration (remount safety)', () => {
    const oldClose = vi.fn()
    const newClose = vi.fn()
    registerDialogClose('feedback', oldClose)
    // The new instance registers before the old instance's destroy runs.
    registerDialogClose('feedback', newClose)
    // The old instance's late unregister must NOT remove the new registration.
    unregisterDialogClose('feedback', oldClose)
    try {
      expect(closeDialogById('feedback')).toBe(true)
      expect(newClose).toHaveBeenCalledOnce()
      expect(oldClose).not.toHaveBeenCalled()
    } finally {
      unregisterDialogClose('feedback', newClose)
    }
  })

  it('swallows a throwing close and still reports it attempted the close', () => {
    const close = vi.fn(() => {
      throw new Error('boom')
    })
    registerDialogClose('about', close)
    try {
      expect(() => closeDialogById('about')).not.toThrow()
      expect(close).toHaveBeenCalled()
    } finally {
      unregisterDialogClose('about', close)
    }
  })
})
