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

/** One row of `listTrustedWebdavCertificates`. */
interface Certificate {
  host: string
  port: number
  fingerprint: string
  approvedAt: string
}

const listTrustedSftpHostKeys = vi.fn<() => Promise<HostKey[]>>()
const listTrustedWebdavCertificates = vi.fn<() => Promise<Certificate[]>>(() => Promise.resolve([]))
const forgetWebdavCertificate = vi.fn<(host: string, port: number) => Promise<boolean>>()
const listSmbLinkConsents = vi.fn<() => Promise<{ server: string; target: string; approvedAt: string }[]>>(() =>
  Promise.resolve([]),
)
// eslint-disable-next-line cmdr/no-confusable-callback-params -- mirrors the real `forgetSmbLinkTarget(server, target)`, whose shape the test exists to pin
const forgetSmbLinkTarget = vi.fn<(server: string, target: string) => Promise<boolean>>()
// eslint-disable-next-line cmdr/no-confusable-callback-params -- mirrors the real `forgetSftpHostKey(host, port, algorithm)`, whose shape the test exists to pin
const forgetSftpHostKey = vi.fn<(host: string, port: number, algorithm: string) => Promise<boolean>>()
// eslint-disable-next-line cmdr/no-confusable-callback-params -- mirrors the real `confirmDialog(message, title)`
const confirmDialog = vi.fn<(message: string, title: string) => Promise<boolean>>()

vi.mock('#lib/settings/settings-store.js', () => ({
  getSetting: vi.fn(() => undefined),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

vi.mock('#lib/tauri-commands/index.js', () => ({
  listTrustedSftpHostKeys: () => listTrustedSftpHostKeys(),
  forgetSftpHostKey: (host: string, port: number, algorithm: string) => forgetSftpHostKey(host, port, algorithm),
  listTrustedWebdavCertificates: () => listTrustedWebdavCertificates(),
  forgetWebdavCertificate: (host: string, port: number) => forgetWebdavCertificate(host, port),
  listSmbLinkConsents: () => listSmbLinkConsents(),
  forgetSmbLinkTarget: (server: string, target: string) => forgetSmbLinkTarget(server, target),
}))

vi.mock('#lib/utils/confirm-dialog.js', () => ({
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
  return Array.from(target.querySelectorAll('.key-list:not([data-testid="trusted-certificates"]) .key-row'))
}

function certificateRows(target: HTMLElement): HTMLElement[] {
  return Array.from(target.querySelectorAll('[data-testid="trusted-certificates"] .key-row'))
}

const synology: Certificate = {
  host: 'nas.local',
  port: 5006,
  fingerprint: 'CD:6F:DC:7C:4B:21:C9:B5:28:58:69:B3:4E:5C:68:B8:55:26:2C:0A:5C:06:7A:8A:2D:BF:BD:FA:AE:C2:9B:E7',
  approvedAt: '2026-10-20T10:00:00Z',
}

// `vi.clearAllMocks` keeps a resolved value, and only one describe wants these rows.
beforeEach(() => {
  listSmbLinkConsents.mockResolvedValue([])
})

describe('the SMB link sign-ins list', () => {
  const pair = { server: 'nas.local', target: 'fs01.example.com', approvedAt: '2026-10-31T10:00:00Z' }

  beforeEach(() => {
    vi.clearAllMocks()
    confirmDialog.mockResolvedValue(true)
    forgetSmbLinkTarget.mockResolvedValue(true)
    listTrustedSftpHostKeys.mockResolvedValue([])
    listSmbLinkConsents.mockResolvedValue([pair])
  })

  it('names both servers, and Forget takes that pair back after confirming', async () => {
    const target = await mountSection()
    const list = target.querySelector('[data-testid="smb-link-consents"]')
    expect(list?.textContent).toContain('nas.local')
    expect(list?.textContent).toContain('fs01.example.com')

    list?.querySelector('button')?.click()
    await vi.waitFor(() => {
      expect(forgetSmbLinkTarget).toHaveBeenCalledWith('nas.local', 'fs01.example.com')
    })
    target.remove()
  })

  it('keeps the pair when the confirmation is declined', async () => {
    confirmDialog.mockResolvedValue(false)
    const target = await mountSection()
    target.querySelector<HTMLButtonElement>('[data-testid="smb-link-consents"] button')?.click()
    await vi.waitFor(() => {
      expect(confirmDialog).toHaveBeenCalledOnce()
    })
    expect(forgetSmbLinkTarget).not.toHaveBeenCalled()
    target.remove()
  })
})

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

describe('the trusted certificates', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    confirmDialog.mockResolvedValue(true)
    listTrustedSftpHostKeys.mockResolvedValue([])
    listTrustedWebdavCertificates.mockResolvedValue([synology])
    forgetWebdavCertificate.mockResolvedValue(true)
  })

  it('shows one row per pinned certificate, with its fingerprint', async () => {
    const target = await mountSection()

    expect(certificateRows(target)).toHaveLength(1)
    const row = certificateRows(target)[0].textContent
    expect(row).toContain('nas.local:5006')
    expect(row).toContain(synology.fingerprint)
    target.remove()
  })

  it('forgets a pin by host and port after asking, then re-reads the list', async () => {
    const target = await mountSection()
    listTrustedWebdavCertificates.mockResolvedValue([])

    certificateRows(target)[0].querySelector('button')?.click()
    await tick()
    await tick()

    expect(confirmDialog).toHaveBeenCalled()
    expect(forgetWebdavCertificate).toHaveBeenCalledWith('nas.local', 5006)
    expect(listTrustedWebdavCertificates).toHaveBeenCalledTimes(2)
    target.remove()
  })

  it('says so plainly when no certificate is pinned', async () => {
    listTrustedWebdavCertificates.mockResolvedValue([])
    const target = await mountSection()

    expect(certificateRows(target)).toHaveLength(0)
    expect(target.textContent).toContain('When you connect to a WebDAV server macOS doesn’t trust')
    target.remove()
  })
})
