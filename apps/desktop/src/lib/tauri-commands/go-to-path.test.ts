/**
 * Tests for the go-to-path wrapper: typed text reaches the resolver in the wire form.
 */

import { describe, it, expect, vi, beforeEach, onTestFinished } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'

vi.mock('#lib/ipc/bindings.js', () => ({
  commands: {
    resolveGoToPath: vi.fn(),
  },
}))

import { commands } from '#lib/ipc/bindings.js'
import { resolveGoToPath } from './go-to-path'

describe('resolveGoToPath wrapper', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('sends a backslashed Windows path in the wire form', async () => {
    onTestFinished(withPlatform('windows'))
    await resolveGoToPath('c:\\Users\\test_user_1\\', 'C:/')
    expect(commands.resolveGoToPath).toHaveBeenCalledWith('C:/Users/test_user_1', 'C:/')
  })

  it.each(['macos', 'linux'] as const)(
    'sends the text untouched on %s, where `\\` is a filename character',
    async (platform) => {
      onTestFinished(withPlatform(platform))
      await resolveGoToPath('/Users/test_user_1/a\\b', '/Users/test_user_1')
      expect(commands.resolveGoToPath).toHaveBeenCalledWith('/Users/test_user_1/a\\b', '/Users/test_user_1')
    },
  )
})
