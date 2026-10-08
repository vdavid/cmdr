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

import CloudAiOffToastContent from './CloudAiOffToastContent.svelte'
import {
  aiTranslateErrorToast,
  isAiTranslateError,
  showAiTranslateErrorToast,
  type AiTranslateThrown,
} from './translate-error-toast'

const ALL_KINDS: AiTranslateErrorKind[] = [
  'off',
  'noCloudConsent',
  'notConfigured',
  'authFailed',
  'rateLimited',
  'timeout',
  'unavailable',
  'emptyResponse',
  'serverError',
  'parseError',
  'unknownProvider',
  'managed',
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

  it("says cloud AI is off, as a warning, when the user hasn't allowed it", () => {
    const copy = aiTranslateErrorToast('noCloudConsent')
    expect(copy.title).toBe('Cloud AI is off')
    expect(copy.body).toBe('Allow it in Settings > AI, then try again.')
    expect(copy.level).toBe('warn')
  })

  it('names the organization, calmly, when its policy refused the request', () => {
    const copy = aiTranslateErrorToast('managed')
    expect(copy.title).toBe('Your organization manages AI in Cmdr')
    expect(copy.body).toBe('Your IT team can tell you which AI services you can use.')
    expect(copy.level).toBe('info')
  })

  it('says which rule refused, when the backend names it', () => {
    expect(aiTranslateErrorToast('managed', 'aiOff').body).toBe('Your organization turned off AI in Cmdr.')
    expect(aiTranslateErrorToast('managed', 'cloudAiOff').body).toBe('Your organization allows only on-device AI.')
    expect(aiTranslateErrorToast('managed', 'hostNotAllowed').body).toBe(
      'Your organization doesn’t allow this AI service. Your IT team can tell you which ones you can use.',
    )
    expect(aiTranslateErrorToast('managed', 'hostNotAllowed').title).toBe('Your organization manages AI in Cmdr')
  })

  it('toasts the specific rule a thrown managed refusal carries', () => {
    addToastMock.mockClear()
    const thrown = Object.assign(new Error('refused'), { kind: 'managed' as const, managed: 'cloudAiOff' as const })
    expect(showAiTranslateErrorToast(thrown)).toBe(true)
    expect(addToastMock).toHaveBeenCalledWith(
      'Your organization manages AI in Cmdr\nYour organization allows only on-device AI.',
      expect.objectContaining({ level: 'info' }),
    )
  })

  it('points the quota case at the plan/billing and the empty case at a smaller model', () => {
    expect(aiTranslateErrorToast('rateLimited').body.toLowerCase()).toContain('billing')
    expect(aiTranslateErrorToast('emptyResponse').body).toContain('gpt-4.1-mini')
  })
})

describe('isAiTranslateError', () => {
  it('accepts a thrown error carrying a known kind', () => {
    expect(isAiTranslateError(makeThrown('rateLimited'))).toBe(true)
    expect(isAiTranslateError(makeThrown('noCloudConsent'))).toBe(true)
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

  it('gives "cloud AI is off" a way into the switch, not only a sentence', () => {
    expect(showAiTranslateErrorToast(makeThrown('noCloudConsent'))).toBe(true)
    const [content, options] = addToastMock.mock.calls[0] as [unknown, { level: string; id: string }]
    expect(content).toBe(CloudAiOffToastContent)
    expect(options.level).toBe('warn')
    expect(options.id).toBe('ai-translate-error')
  })

  it('returns false and does not toast for an unrelated error', () => {
    expect(showAiTranslateErrorToast(new Error('network blip'))).toBe(false)
    expect(addToastMock).not.toHaveBeenCalled()
  })
})
