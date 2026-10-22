import { describe, it, expect } from 'vitest'
import type { ConflictResolution } from '#lib/file-explorer/types.js'
import { conflictPolicyCaption } from './conflict-policy-labels'

describe('conflictPolicyCaption', () => {
  // The setup dialog's own wording for the radio a person picked, so the two
  // dialogs can't word one policy two ways. "Ask for each" is the radio's
  // count-free wording, and Rename is only ever reached by "Rename all".
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
