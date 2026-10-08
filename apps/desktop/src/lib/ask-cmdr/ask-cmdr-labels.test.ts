import { describe, it, expect } from 'vitest'
import { errorMessage } from './ask-cmdr-labels'
import { managedAiRefusalMessage } from '$lib/managed-policy/ai-refusal'

describe('errorMessage', () => {
  it('words the organization’s refusal by the rule that refused, through the shared copy map', () => {
    expect(errorMessage('managedByOrganization', 'cloudAiOff')).toBe(managedAiRefusalMessage('cloudAiOff'))
    expect(errorMessage('managedByOrganization', 'aiOff')).toBe(managedAiRefusalMessage('aiOff'))
    expect(errorMessage('managedByOrganization', 'hostNotAllowed')).toBe(managedAiRefusalMessage('hostNotAllowed'))
  })

  it('keeps its own line when no reason came with it', () => {
    expect(errorMessage('managedByOrganization')).toBe(errorMessage('managedByOrganization', null))
    expect(errorMessage('timeout', null)).not.toBe('')
  })
})
