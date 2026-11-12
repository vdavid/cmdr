/**
 * A settings switch says "Turn on" on Windows, where Microsoft's style guide words switches that
 * way, and keeps "Enable" on macOS and Linux (#1121). The SMB summary names the switch by the
 * label its OS shows.
 */
import { describe, it, expect, beforeAll, afterAll, beforeEach } from 'vitest'
import { _setLocaleForTests } from '#lib/intl/locale.js'
import { tString } from '#lib/intl/messages.svelte.js'
import { withPlatform } from '#lib/platform/test-platform.js'
import { getSettingDefinition } from './settings-registry'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

const expected = {
  macos: ['Enable networking', 'Enable MCP server', 'Enable networking, direct SMB connections'],
  linux: ['Enable networking', 'Enable MCP server', 'Enable networking, direct SMB connections'],
  windows: ['Turn on networking', 'Turn on MCP server', 'Turn on networking, direct SMB connections'],
} as const

describe.each(['macos', 'windows', 'linux'] as const)('switch labels on %s', (platform) => {
  beforeEach(() => withPlatform(platform))

  it('words each switch the way this OS words switches', () => {
    const [networking, mcp, summary] = expected[platform]
    expect(getSettingDefinition('network.enabled')?.label).toBe(networking)
    expect(getSettingDefinition('developer.mcpEnabled')?.label).toBe(mcp)
    expect(tString('settings.summary.smbNetworkShares').startsWith(summary)).toBe(true)
  })
})
