/**
 * The transfer error messages that name a key name the one that runs the command right now,
 * in the reader's own spelling, and name the command instead once the user removed every key.
 * The refused-trash pair lives in `trash-refused-messages.test.ts`.
 */
import { describe, expect, it, onTestFinished, vi } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'
import type { WriteOperationError } from '#lib/file-explorer/types.js'
import { getUserFriendlyMessage } from './transfer-error-messages'

// The commands whose every key the user removed; the rest keep their real live keys.
const unbound = vi.hoisted(() => new Set<string>())
vi.mock(import('#lib/shortcuts/index.js'), async (importOriginal) => {
  const actual = await importOriginal()
  return { ...actual, getLiveShortcuts: (id: string) => (unbound.has(id) ? [] : actual.getLiveShortcuts(id)) }
})

function unbind(commandId: string): void {
  unbound.add(commandId)
  onTestFinished(() => {
    unbound.clear()
  })
}

describe('trash_not_supported', () => {
  const error: WriteOperationError = { type: 'trash_not_supported', path: '/Volumes/USB/file.txt' }

  it('names the live delete-permanently key in the reader’s spelling', () => {
    onTestFinished(withPlatform('macos'))
    expect(getUserFriendlyMessage(error, 'trash').suggestion).toBe('Use ⇧F8 to delete permanently instead.')
  })

  it('names the Delete permanently command once no key runs it', () => {
    unbind('file.deletePermanently')
    expect(getUserFriendlyMessage(error, 'trash').suggestion).toBe('Use the Delete permanently command instead.')
  })
})

describe('renaming the home folder itself', () => {
  const error: WriteOperationError = {
    type: 'home_folder',
    path: '/Users/test_user_11',
    relation: 'isHome',
    action: 'rename',
  }

  it('jokes about the live rename key', () => {
    expect(getUserFriendlyMessage(error, 'move').suggestion).toContain('It’s not a quick F2, though.')
  })

  it('jokes about a rename once no key runs it', () => {
    unbind('file.rename')
    expect(getUserFriendlyMessage(error, 'move').suggestion).toContain('It’s not a quick rename, though.')
  })
})
