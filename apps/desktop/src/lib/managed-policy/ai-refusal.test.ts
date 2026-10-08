import { describe, it, expect } from 'vitest'
import type { ManagedAiRefusal } from '$lib/ipc/bindings'
import { managedAiRefusalMessage } from './ai-refusal'

describe('managedAiRefusalMessage', () => {
  it('words each refusal the way the person meets it, naming the organization', () => {
    expect(managedAiRefusalMessage('aiOff')).toBe('Your organization turned off AI in Cmdr.')
    expect(managedAiRefusalMessage('cloudAiOff')).toBe('Your organization allows only on-device AI.')
    expect(managedAiRefusalMessage('hostNotAllowed')).toBe(
      'Your organization doesn’t allow this AI service. Your IT team can tell you which ones you can use.',
    )
  })

  it('stays clear of the words the style guide keeps out of refusals', () => {
    const all: ManagedAiRefusal[] = ['aiOff', 'cloudAiOff', 'hostNotAllowed']
    for (const refusal of all) {
      const text = managedAiRefusalMessage(refusal).toLowerCase()
      expect(text, refusal).not.toContain('error')
      expect(text, refusal).not.toContain('failed')
    }
  })
})
