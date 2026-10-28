import { describe, expect, it } from 'vitest'
import type { AiEffortOptions } from '#lib/ipc/bindings.js'
import {
  effectiveEffortChoice,
  effortChoices,
  effortLabelKey,
  readEffortChoice,
  showsEffortPicker,
  writeEffortChoice,
} from './ai-effort'

describe('the picker’s label', () => {
  it('uses each provider’s own name for the setting', () => {
    expect(effortLabelKey('openai')).toBe('settings.ai.reasoningEfforts.label.reasoningEffort')
    expect(effortLabelKey('xai')).toBe('settings.ai.reasoningEfforts.label.reasoningEffort')
    expect(effortLabelKey('openrouter')).toBe('settings.ai.reasoningEfforts.label.reasoningEffort')
    expect(effortLabelKey('anthropic')).toBe('settings.ai.reasoningEfforts.label.effort')
    expect(effortLabelKey('google-gemini')).toBe('settings.ai.reasoningEfforts.label.thinkingLevel')
  })

  it('falls back to Thinking level for a provider with no name of its own, or an unknown id', () => {
    expect(effortLabelKey('ollama')).toBe('settings.ai.reasoningEfforts.label.thinkingLevel')
    expect(effortLabelKey('custom')).toBe('settings.ai.reasoningEfforts.label.thinkingLevel')
    expect(effortLabelKey('no-such-provider')).toBe('settings.ai.reasoningEfforts.label.thinkingLevel')
  })
})

const claude: AiEffortOptions = {
  reasons: true,
  levels: ['low', 'medium', 'high', 'max'],
  canTurnOff: true,
  recommended: 'high',
  source: 'provider',
}

describe('the Effort picker', () => {
  it('offers Recommended, Off, then the levels the model takes', () => {
    expect(effortChoices(claude)).toEqual(['recommended', 'off', 'low', 'medium', 'high', 'max'])
    expect(effortChoices({ ...claude, canTurnOff: false, levels: ['low', 'high'] })).toEqual([
      'recommended',
      'low',
      'high',
    ])
  })

  it('offers Recommended alone when nothing knows the levels', () => {
    expect(effortChoices({ reasons: null, levels: [], canTurnOff: false, recommended: null, source: null })).toEqual([
      'recommended',
    ])
  })

  it('hides only for a model known not to reason', () => {
    expect(showsEffortPicker(claude)).toBe(true)
    expect(showsEffortPicker({ ...claude, reasons: null, levels: [] })).toBe(true)
    expect(showsEffortPicker({ ...claude, reasons: false, levels: [] })).toBe(false)
    expect(showsEffortPicker(null)).toBe(false)
  })
})

describe('what a stored pick amounts to on this model', () => {
  const gemini: AiEffortOptions = { ...claude, canTurnOff: false, levels: ['low', 'high'], recommended: null }

  it('keeps a pick the model takes', () => {
    expect(effectiveEffortChoice(claude, 'max')).toBe('max')
    expect(effectiveEffortChoice(claude, 'off')).toBe('off')
  })

  it('maps one it does not to the nearest it does, the cheaper on a tie', () => {
    expect(effectiveEffortChoice(gemini, 'medium')).toBe('low')
    expect(effectiveEffortChoice(gemini, 'max')).toBe('high')
    expect(effectiveEffortChoice(gemini, 'off')).toBe('low')
  })

  it('reads as Recommended on a model with no known levels', () => {
    const unknown: AiEffortOptions = { reasons: null, levels: [], canTurnOff: false, recommended: null, source: null }
    expect(effectiveEffortChoice(unknown, 'high')).toBe('recommended')
  })
})

describe('storing a pick per provider and model', () => {
  it('keys it by provider and model, and reads Recommended when absent or unreadable', () => {
    const stored = writeEffortChoice('{}', 'anthropic', 'claude-opus-5-5', 'max')
    expect(readEffortChoice(stored, 'anthropic', 'claude-opus-5-5')).toBe('max')
    expect(readEffortChoice(stored, 'openrouter', 'claude-opus-5-5')).toBe('recommended')
    expect(readEffortChoice('not json', 'anthropic', 'claude-opus-5-5')).toBe('recommended')
    expect(readEffortChoice('{"anthropic/x":"xhigh"}', 'anthropic', 'x')).toBe('recommended')
  })

  it('drops the entry when the pick goes back to Recommended, keeping the others', () => {
    const both = writeEffortChoice(writeEffortChoice('{}', 'anthropic', 'a', 'low'), 'openai', 'b', 'high')
    expect(JSON.parse(writeEffortChoice(both, 'anthropic', 'a', 'recommended'))).toEqual({ 'openai/b': 'high' })
  })
})
