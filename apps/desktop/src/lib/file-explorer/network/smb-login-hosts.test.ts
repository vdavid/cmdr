/**
 * Tests for the credential-form host registry: which pane gets asked to render
 * the inline SMB login form for a given volume.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { registerSmbLoginHost, promptForSmbCredentials, _clearSmbLoginHostsForTests } from './smb-login-hosts'
import type { UpgradeResult } from '$lib/tauri-commands'

const info: UpgradeResult & { status: 'credentialsNeeded' } = {
  status: 'credentialsNeeded',
  server: 'naspolya',
  share: 'archive',
  port: 445,
  displayName: 'Naspolya',
  usernameHint: null,
  message: null,
}

beforeEach(() => {
  _clearSmbLoginHostsForTests()
})

describe('promptForSmbCredentials', () => {
  it('prefers the pane already showing that volume, so the form opens where the user is looking', () => {
    const left = vi.fn()
    const right = vi.fn()
    registerSmbLoginHost({ getVolumeId: () => 'smb-photos', open: left })
    registerSmbLoginHost({ getVolumeId: () => 'smb-archive', open: right })

    expect(promptForSmbCredentials(info, 'smb-archive')).toBe(true)

    expect(right).toHaveBeenCalledWith(info, 'smb-archive')
    expect(left).not.toHaveBeenCalled()
  })

  it('falls back to any mounted pane, since a pane can host a form for a volume it is not showing', () => {
    const somewhereElse = vi.fn()
    registerSmbLoginHost({ getVolumeId: () => 'root', open: somewhereElse })

    expect(promptForSmbCredentials(info, 'smb-archive')).toBe(true)

    expect(somewhereElse).toHaveBeenCalledWith(info, 'smb-archive')
  })

  it('reads the volume live, so a pane that navigated is matched on where it is now', () => {
    let showing = 'root'
    const open = vi.fn()
    registerSmbLoginHost({ getVolumeId: () => showing, open })
    registerSmbLoginHost({ getVolumeId: () => 'smb-photos', open: vi.fn() })

    showing = 'smb-archive'
    promptForSmbCredentials(info, 'smb-archive')

    expect(open).toHaveBeenCalledWith(info, 'smb-archive')
  })

  it('reports failure when nothing can host the form, so the caller says so instead of looking inert', () => {
    expect(promptForSmbCredentials(info, 'smb-archive')).toBe(false)
  })

  it('drops an unregistered pane, so a destroyed pane is never asked to render', () => {
    const open = vi.fn()
    const unregister = registerSmbLoginHost({ getVolumeId: () => 'smb-archive', open })

    unregister()

    expect(promptForSmbCredentials(info, 'smb-archive')).toBe(false)
    expect(open).not.toHaveBeenCalled()
  })
})
