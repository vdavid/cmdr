import { describe, it, expect } from 'vitest'
import type { ConflictResolution } from '#lib/file-explorer/types.js'
import { conflictPolicyCaption } from './conflict-policy-labels'

describe('conflictPolicyCaption', () => {
  // English names each policy with the setup dialog's own words, but through
  // dedicated `policyName.*` keys: a language whose radios are imperatives
  // names the policy as a noun phrase here. Rename is only reached by "Rename all".
  const cases: [ConflictResolution, string][] = [
    ['skip', 'If a file already exists: Skip'],
    ['overwrite', 'If a file already exists: Overwrite'],
    ['overwrite_smaller', 'If a file already exists: Overwrite if smaller'],
    ['overwrite_older', 'If a file already exists: Overwrite if older'],
    ['stop', 'If a file already exists: Ask for each'],
    ['rename', 'If a file already exists: Rename'],
  ]

  it.each(cases)('words %s as the setup dialog does', (policy, expected) => {
    expect(conflictPolicyCaption(policy)).toBe(expected)
  })
})
