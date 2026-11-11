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
    await resolveGoToPath('c:\\Users\\ada\\', 'C:/')
    expect(commands.resolveGoToPath).toHaveBeenCalledWith('C:/Users/ada', 'C:/')
  })

  it.each(['macos', 'linux'] as const)(
    'sends the text untouched on %s, where `\\` is a filename character',
    async (platform) => {
      onTestFinished(withPlatform(platform))
      await resolveGoToPath('/Users/ada/a\\b', '/Users/ada')
      expect(commands.resolveGoToPath).toHaveBeenCalledWith('/Users/ada/a\\b', '/Users/ada')
    },
  )
})
