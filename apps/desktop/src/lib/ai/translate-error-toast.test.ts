/**
 * Tests for the AI-translation error → toast mapping.
 *
 * Pins:
 *   - Every `AiTranslateErrorKind` maps to non-empty, style-guide-clean copy.
 *   - `isAiTranslateError` accepts thrown errors carrying a known `kind`, rejects everything else.
 *   - `showAiTranslateErrorToast` toasts a recognized error and returns whether it handled it.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { AiTranslateErrorKind } from '$lib/ipc/bindings'

const addToastMock = vi.fn()
vi.mock('$lib/ui/toast/toast-store.svelte', () => ({
  addToast: (...args: unknown[]): string => {
    addToastMock(...args)
    return 'toast-id'
  },
}))

import {
  aiTranslateErrorToast,
  isAiTranslateError,
  showAiTranslateErrorToast,
  type AiTranslateThrown,
} from './translate-error-toast'

const ALL_KINDS: AiTranslateErrorKind[] = [
  'off',
  'notConfigured',
  'authFailed',
  'rateLimited',
  'timeout',
  'unavailable',
  'emptyResponse',
  'serverError',
  'parseError',
  'unknownProvider',
]

function makeThrown(kind: AiTranslateErrorKind): AiTranslateThrown {
  return Object.assign(new Error(`detail for ${kind}`), { kind })
}

describe('aiTranslateErrorToast', () => {
  it('returns non-empty, actionable copy for every kind', () => {
    for (const kind of ALL_KINDS) {
      const copy = aiTranslateErrorToast(kind)
      expect(copy.title.length, kind).toBeGreaterThan(0)
      expect(copy.body.length, kind).toBeGreaterThan(0)
      expect(['default', 'info', 'success', 'warn', 'error']).toContain(copy.level)
    }
  })

  it('never uses the words "error" or "failed" in user-facing copy (style guide)', () => {
    for (const kind of ALL_KINDS) {
      const copy = aiTranslateErrorToast(kind)
      const text = `${copy.title} ${copy.body}`.toLowerCase()
      expect(text, kind).not.toContain('error')
      expect(text, kind).not.toContain('failed')
    }
  })

  it('points the quota case at the plan/billing and the empty case at a smaller model', () => {
    expect(aiTranslateErrorToast('rateLimited').body.toLowerCase()).toContain('billing')
    expect(aiTranslateErrorToast('emptyResponse').body).toContain('gpt-4.1-mini')
  })
})

describe('isAiTranslateError', () => {
  it('accepts a thrown error carrying a known kind', () => {
    expect(isAiTranslateError(makeThrown('rateLimited'))).toBe(true)
  })

  it('rejects a plain Error, a string, a kindless object, and an unknown kind', () => {
    expect(isAiTranslateError(new Error('boom'))).toBe(false)
    expect(isAiTranslateError('rateLimited')).toBe(false)
    expect(isAiTranslateError({ kind: 'rateLimited' })).toBe(false) // not an Error
    expect(isAiTranslateError(Object.assign(new Error('x'), { kind: 'bogus' }))).toBe(false)
    expect(isAiTranslateError(null)).toBe(false)
  })
})

describe('showAiTranslateErrorToast', () => {
  beforeEach(() => addToastMock.mockClear())

  it('toasts and returns true for a recognized translation error', () => {
    const handled = showAiTranslateErrorToast(makeThrown('authFailed'))
    expect(handled).toBe(true)
    expect(addToastMock).toHaveBeenCalledTimes(1)
    const [content, options] = addToastMock.mock.calls[0] as [string, { level: string; id: string }]
    expect(content).toContain('API key')
    expect(options.level).toBe('error')
    expect(options.id).toBe('ai-translate-error')
  })

  it('returns false and does not toast for an unrelated error', () => {
    expect(showAiTranslateErrorToast(new Error('network blip'))).toBe(false)
    expect(addToastMock).not.toHaveBeenCalled()
  })
})
