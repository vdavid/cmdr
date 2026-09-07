/**
 * Tier-3 tests for `ServersSection.svelte` (File systems › Servers).
 *
 * The page is a LIST with a destructive button per row, so what is pinned here
 * is what the list says and what the button does: one row per trusted key, a
 * Forget that asks first and drops the whole identity (host, port, AND
 * algorithm, since a server can hold several key types), and the empty sentence
 * only when the store is genuinely empty.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import ServersSection from './ServersSection.svelte'

/** One row of `listTrustedSftpHostKeys`, as the settings page reads it. */
interface HostKey {
  host: string
  port: number
  algorithm: string
  fingerprint: string
  approvedAt: string
}

const listTrustedSftpHostKeys = vi.fn<() => Promise<HostKey[]>>()
// eslint-disable-next-line cmdr/no-confusable-callback-params -- mirrors the real `forgetSftpHostKey(host, port, algorithm)`, whose shape the test exists to pin
const forgetSftpHostKey = vi.fn<(host: string, port: number, algorithm: string) => Promise<boolean>>()
// eslint-disable-next-line cmdr/no-confusable-callback-params -- mirrors the real `confirmDialog(message, title)`
const confirmDialog = vi.fn<(message: string, title: string) => Promise<boolean>>()

vi.mock('$lib/settings/settings-store', () => ({
  getSetting: vi.fn(() => undefined),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/tauri-commands', () => ({
  listTrustedSftpHostKeys: () => listTrustedSftpHostKeys(),
  forgetSftpHostKey: (host: string, port: number, algorithm: string) => forgetSftpHostKey(host, port, algorithm),
}))

vi.mock('$lib/utils/confirm-dialog', () => ({
  confirmDialog: (message: string, title: string) => confirmDialog(message, title),
}))

const naspolya: HostKey = {
  host: 'nas.local',
  port: 2222,
  algorithm: 'ssh-ed25519',
  fingerprint: 'SHA256:9GbJ2r0Ck9tqZ6y5rQ0m8kFq1sWl4pS2vNbXcYdEfGh',
  approvedAt: '2026-09-01T10:00:00Z',
}

const jumpBox: HostKey = {
  host: 'jump.example.com',
  port: 22,
  algorithm: 'rsa-sha2-512',
  fingerprint: 'SHA256:aB3dE5fG7hI9jK1lM3nO5pQ7rS9tU1vW3xY5zA7bC9d',
  approvedAt: '2026-08-14T08:30:00Z',
}

async function mountSection(): Promise<HTMLDivElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(ServersSection, { target, props: { searchQuery: '' } })
  await tick()
  await tick()
  return target
}

function rows(target: HTMLElement): HTMLElement[] {
  return Array.from(target.querySelectorAll('.key-row'))
}

describe('the trusted-key list', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    confirmDialog.mockResolvedValue(true)
    forgetSftpHostKey.mockResolvedValue(true)
    listTrustedSftpHostKeys.mockResolvedValue([naspolya, jumpBox])
  })

  it('shows one row per key, with the fingerprint a person compares against', async () => {
    const target = await mountSection()

    expect(rows(target)).toHaveLength(2)
    const first = rows(target)[0].textContent
    expect(first).toContain('nas.local:2222')
    expect(first).toContain('ssh-ed25519')
    expect(first).toContain(naspolya.fingerprint)
    target.remove()
  })

  it('says so plainly when nothing has been trusted', async () => {
    listTrustedSftpHostKeys.mockResolvedValue([])
    const target = await mountSection()

    expect(rows(target)).toHaveLength(0)
    expect(target.querySelector('.empty')?.textContent).toContain('Nothing trusted yet')
    target.remove()
  })

  /** A read that throws leaves the page usable rather than blank forever. */
  it('falls back to the empty state when the store cannot be read', async () => {
    listTrustedSftpHostKeys.mockRejectedValue(new Error('no store'))
    const target = await mountSection()

    expect(target.querySelector('.empty')).not.toBeNull()
    target.remove()
  })
})

describe('forgetting a key', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    confirmDialog.mockResolvedValue(true)
    forgetSftpHostKey.mockResolvedValue(true)
    listTrustedSftpHostKeys.mockResolvedValue([naspolya, jumpBox])
  })

  /**
   * ❗ Host, port, AND algorithm: one server can hold several key types, and
   * forgetting by host alone would drop the wrong one (or all of them).
   */
  it('drops the whole identity, then re-reads the list', async () => {
    const target = await mountSection()
    listTrustedSftpHostKeys.mockResolvedValue([jumpBox])

    rows(target)[0].querySelector('button')?.click()
    await tick()
    await tick()

    expect(forgetSftpHostKey).toHaveBeenCalledWith('nas.local', 2222, 'ssh-ed25519')
    expect(listTrustedSftpHostKeys).toHaveBeenCalledTimes(2)
    target.remove()
  })

  it('asks first, and does nothing when the answer is no', async () => {
    confirmDialog.mockResolvedValue(false)
    const target = await mountSection()

    rows(target)[0].querySelector('button')?.click()
    await tick()
    await tick()

    expect(confirmDialog).toHaveBeenCalled()
    expect(forgetSftpHostKey).not.toHaveBeenCalled()
    target.remove()
  })
})
