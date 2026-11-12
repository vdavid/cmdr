/**
 * The sheet editing a saved SFTP server: what's locked, where a refusal lands, and what a
 * Save writes to the password store. SMB's edit lives in `SignInSheet.smb.svelte.test.ts`.
 */

import { describe, expect, it, vi, beforeEach, afterEach, onTestFinished } from 'vitest'
import { tick } from 'svelte'
import { withPlatform } from '#lib/platform/test-platform.js'
import { buttonSaying, flush, protocolTab, renderSheet, typeInto } from './test-helpers'

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  hasServerSecret: vi.fn(() => Promise.resolve(false)),
  getKnownSftpServers: vi.fn(() => Promise.resolve([])),
  getKnownWebdavServers: vi.fn(() => Promise.resolve([])),
  getSftpUnattendedReconnect: vi.fn(() => Promise.resolve('ready')),
  getWebdavUnattendedReconnect: vi.fn(() => Promise.resolve('possible')),
  forgetServerSecret: vi.fn(() => Promise.resolve(true)),
  updateSavedServer: vi.fn(() => Promise.resolve({ outcome: 'saved' })),
  savedServerId: vi.fn(() => Promise.resolve('sftp-nas-local-22-ada')),
  updateSavedSmbHost: vi.fn(() => Promise.resolve({ outcome: 'saved' })),
  saveSftpCredentials: vi.fn(() => Promise.resolve()),
  saveWebdavCredentials: vi.fn(() => Promise.resolve()),
  approveSftpHostKey: vi.fn(() => Promise.resolve({ outcome: 'recorded' })),
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(() => Promise.resolve(null)) }))

afterEach(() => {
  document.body.innerHTML = ''
})

/**
 * ❗ Edit mode changes a server's SETTINGS. The address and the account are its
 * IDENTITY: Rust mints the volume id from `(host, port, username)` and
 * `sftp_known_servers::remember` is keyed on the same tuple, so an edited one
 * upserts a SECOND saved entry beside the first rather than moving anything.
 * The fields say so by being locked, and the hint says what to do instead.
 */
describe('SignInSheet: edit mode', () => {
  const SAVED = {
    id: 'sftp-nas-local-22-ada',
    protocol: 'sftp' as const,
    displayName: 'Naspolya',
    nameSource: 'user' as const,
    address: 'nas.local:22',
    username: 'ada',
    pinned: true,
    lastConnectedAt: null,
    autoReconnect: true,
    places: [],
  }

  // These pin the words a Mac reads; the Windows and Linux ones are pinned by name below.
  beforeEach(() => withPlatform('macos'))

  const KNOWN_SFTP = {
    host: 'nas.local',
    port: 22,
    username: 'ada',
    displayName: 'Naspolya',
    remoteRoot: '/srv/data',
    keyFile: null,
    useAgent: true,
    autoReconnect: true,
    pinned: true,
    lastConnectedAt: '2026-09-06T00:00:00Z',
  }

  beforeEach(async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    vi.mocked(commands.getKnownSftpServers).mockResolvedValue([KNOWN_SFTP])
    vi.mocked(commands.saveSftpCredentials).mockClear()
    vi.mocked(commands.forgetServerSecret).mockClear()
    vi.mocked(commands.updateSavedServer).mockClear()
  })

  it('opens an unnamed server with an empty name, its label as the placeholder, and the label in the title', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    vi.mocked(commands.getKnownSftpServers).mockResolvedValue([{ ...KNOWN_SFTP, displayName: '' }])
    await renderSheet({ mode: 'edit', server: { ...SAVED, displayName: 'ada@nas.local', nameSource: 'fallback' } })

    // ❗ The field holds what the user TYPED, which for an unnamed server is
    // nothing. A name that looked like the address is what sent a person to
    // widen the root through the wrong field.
    const name = document.body.querySelector<HTMLInputElement>('#server-name')
    expect(name?.value).toBe('')
    expect(name?.placeholder).toBe('Leave empty to use ada@nas.local')
    expect(document.body.textContent).toContain('Edit ada@nas.local')
  })

  it('shows the saved root folder and start folder', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    vi.mocked(commands.getKnownSftpServers).mockResolvedValue([{ ...KNOWN_SFTP, startFolder: '/srv/data/photos' }])
    await renderSheet({ mode: 'edit', server: SAVED })

    expect(document.body.querySelector<HTMLInputElement>('#server-remote-root')?.value).toBe('/srv/data')
    expect(document.body.querySelector<HTMLInputElement>('#server-start-folder')?.value).toBe('/srv/data/photos')
  })

  it('closes as saved when the save lands', async () => {
    const { done } = await renderSheet({ mode: 'edit', server: SAVED })

    buttonSaying('Save').click()
    await flush()

    expect(done).toEqual([{ kind: 'saved' }])
  })

  it('explains "Reconnect automatically" beside its checkbox, since the label alone reads as "connect at startup"', async () => {
    await renderSheet({ mode: 'edit', server: SAVED })

    const tip = document.body.querySelector('button[aria-label="More about “Reconnect automatically”"]')
    expect(tip).not.toBeNull()
    // Beside the checkbox, ❌ never inside its `<label>`: a click on the glyph would flip the box.
    expect(tip?.closest('label')).toBeNull()
    expect(tip?.closest('.field')?.textContent).toContain('Reconnect automatically')
  })

  it('keeps the sheet open on a root the server lacks, under the root folder, and writes no password', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    vi.mocked(commands.updateSavedServer).mockResolvedValueOnce({ outcome: 'root_not_found' })
    const { done } = await renderSheet({ mode: 'edit', server: SAVED })

    typeInto(document.body.querySelector<HTMLInputElement>('#server-secret') as HTMLInputElement, 'hunter2')
    await tick()
    buttonSaying('Save').click()
    await flush()

    const root = document.body.querySelector<HTMLInputElement>('#server-remote-root') as HTMLInputElement
    expect(done).toEqual([])
    expect(document.body.querySelector('#server-remote-root-refusal')?.textContent).toContain('nas.local')
    expect(root.getAttribute('aria-invalid')).toBe('true')
    expect(document.activeElement).toBe(root)
    // ❗ A refusal saved nothing, so the password typed beside it isn't filed
    // either: half an edit is a server that dials one way and lists another.
    expect(vi.mocked(commands.saveSftpCredentials)).not.toHaveBeenCalled()
  })

  it('puts a start folder the server lacks under the start folder', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    vi.mocked(commands.updateSavedServer).mockResolvedValueOnce({ outcome: 'start_folder_not_found' })
    const { done } = await renderSheet({ mode: 'edit', server: SAVED })

    buttonSaying('Save').click()
    await flush()

    const startFolder = document.body.querySelector<HTMLInputElement>('#server-start-folder')
    expect(done).toEqual([])
    expect(document.body.querySelector('#server-start-folder-refusal')).not.toBeNull()
    expect(document.body.querySelector('#server-remote-root-refusal')).toBeNull()
    expect(document.activeElement).toBe(startFolder)
  })

  it('refuses a start folder outside the root before asking the backend', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    const { done } = await renderSheet({ mode: 'edit', server: SAVED })

    const startFolder = document.body.querySelector<HTMLInputElement>('#server-start-folder') as HTMLInputElement
    typeInto(startFolder, '/srv/database')
    await tick()
    buttonSaying('Save').click()
    await flush()

    expect(vi.mocked(commands.updateSavedServer)).not.toHaveBeenCalled()
    expect(done).toEqual([])
    expect(document.body.querySelector('#server-start-folder-refusal')).not.toBeNull()
    expect(document.activeElement).toBe(startFolder)
  })

  it('stays open and says so when the connected server did not answer in time', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    vi.mocked(commands.updateSavedServer).mockResolvedValueOnce({ outcome: 'unreachable' })
    const { done } = await renderSheet({ mode: 'edit', server: SAVED })

    buttonSaying('Save').click()
    await flush()

    expect(done).toEqual([])
    const formRefusal = document.body.querySelector('.form-refusal')
    expect(formRefusal?.getAttribute('role')).toBe('alert')
    expect(formRefusal?.textContent).toContain('nas.local')
  })

  it('lets the address move, locks the account and the protocol, and says what each does', async () => {
    await renderSheet({ mode: 'edit', server: SAVED })

    const address = document.body.querySelector<HTMLInputElement>('#server-address')
    const username = document.body.querySelector<HTMLInputElement>('#server-username')
    expect(address?.value).toBe('ada@nas.local:22')
    // ❗ A server that moved (a new IP, a Tailscale name) keeps everything: the
    // backend moves it on Save.
    expect(address?.disabled).toBe(false)
    expect(document.body.querySelector('#server-address-help')?.textContent).toContain('come along')
    // Another account is another place, so that one stays an Add.
    expect(username?.disabled).toBe(true)
    expect(protocolTab('SFTP').disabled).toBe(true)
    // ❗ An inert-looking field with no explanation is worse than no field. The
    // hint names the path that actually works.
    expect(document.body.querySelector('#server-identity-hint')?.textContent).toContain('add it as a new server')
  })

  it('saves as the place it was raised on, so the backend decides whether the address moved', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    await renderSheet({ mode: 'edit', server: SAVED })

    typeInto(document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement, '10.0.0.5')
    await tick()
    buttonSaying('Save').click()
    await flush()

    expect(vi.mocked(commands.updateSavedServer)).toHaveBeenCalledWith(
      expect.objectContaining({ protocol: 'sftp', host: '10.0.0.5', username: 'ada' }),
      'sftp-nas-local-22-ada',
    )
  })

  it('puts an address another saved server holds under the address, naming that server', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    vi.mocked(commands.updateSavedServer).mockResolvedValueOnce({ outcome: 'address_taken', name: 'Old NAS' })
    const { done } = await renderSheet({ mode: 'edit', server: SAVED })

    const address = document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement
    typeInto(address, 'old-nas.local')
    await tick()
    buttonSaying('Save').click()
    await flush()

    expect(done).toEqual([])
    expect(document.body.querySelector('#server-address-refusal')?.textContent).toContain('Old NAS')
    expect(document.activeElement).toBe(address)
  })

  it('forgets the password at the NEW address when Remember went off in the same save that moved it', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    vi.mocked(commands.hasServerSecret).mockResolvedValueOnce(true)
    vi.mocked(commands.savedServerId).mockResolvedValueOnce('sftp-10-0-0-5-22-ada')
    await renderSheet({ mode: 'edit', server: SAVED })

    typeInto(document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement, '10.0.0.5')
    const remember = [...document.body.querySelectorAll('label')].find((l) =>
      l.textContent.includes('Remember in Keychain'),
    )
    remember?.click()
    await flush()
    buttonSaying('Save').click()
    await flush()

    // ❗ The old id names nothing after a move, so a forget aimed there would leave
    // the moved password filed.
    expect(vi.mocked(commands.forgetServerSecret)).toHaveBeenCalledWith('sftp-10-0-0-5-22-ada')
  })

  it('writes a typed password through the Keychain, so the field does what it shows', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    await renderSheet({ mode: 'edit', server: SAVED })

    typeInto(document.body.querySelector<HTMLInputElement>('#server-secret') as HTMLInputElement, 'hunter2')
    await tick()
    buttonSaying('Save').click()
    await flush()

    // The tuple the volume id is minted from, so the entry the next dial reads
    // is the one this writes.
    expect(vi.mocked(commands.saveSftpCredentials)).toHaveBeenCalledWith('nas.local', 22, 'ada', 'hunter2')
    expect(vi.mocked(commands.forgetServerSecret)).not.toHaveBeenCalled()
  })

  it('says the edit landed but the password did not when the Keychain refuses it', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    vi.mocked(commands.saveSftpCredentials).mockRejectedValueOnce(new Error('keychain refused: access_denied'))
    const { done } = await renderSheet({ mode: 'edit', server: SAVED })

    const secret = document.body.querySelector<HTMLInputElement>('#server-secret') as HTMLInputElement
    typeInto(secret, 'hunter2')
    await tick()
    buttonSaying('Save').click()
    await flush()

    // ❗ The edit WAS saved and no server was contacted. "Couldn't reach" under
    // the address sent people to save again over an edit that had landed.
    expect(done).toEqual([])
    expect(document.body.textContent).toContain('Cmdr saved your changes')
    expect(document.body.textContent).not.toContain('couldn’t reach')
    expect(secret.getAttribute('aria-invalid')).toBe('true')
    expect(document.activeElement).toBe(secret)
  })

  it('leaves the store alone when the password field is left empty', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    await renderSheet({ mode: 'edit', server: SAVED })

    buttonSaying('Save').click()
    await flush()

    // An empty box means "I didn't come here to change the password", ❌ never
    // "store an empty one".
    expect(vi.mocked(commands.saveSftpCredentials)).not.toHaveBeenCalled()
    expect(vi.mocked(commands.forgetServerSecret)).not.toHaveBeenCalled()
  })

  it('says a stored password is kept when its field stays empty, and keeps it on Save', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    vi.mocked(commands.hasServerSecret).mockResolvedValueOnce(true)
    const { done } = await renderSheet({ mode: 'edit', server: SAVED })

    // ❗ A blank field over a stored secret read as "no password saved".
    const secret = document.body.querySelector<HTMLInputElement>('#server-secret')
    expect(secret?.value).toBe('')
    expect(secret?.placeholder).toBe('Saved in Keychain. Leave empty to keep it.')

    buttonSaying('Save').click()
    await flush()

    expect(done).toEqual([{ kind: 'saved' }])
    expect(vi.mocked(commands.saveSftpCredentials)).not.toHaveBeenCalled()
    expect(vi.mocked(commands.forgetServerSecret)).not.toHaveBeenCalled()
  })

  it('drops the "saved" placeholder once Remember goes off, since Save then forgets the password', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    vi.mocked(commands.hasServerSecret).mockResolvedValueOnce(true)
    await renderSheet({ mode: 'edit', server: SAVED })
    expect(document.body.querySelector<HTMLInputElement>('#server-secret')?.placeholder).not.toBe('')

    const remember = [...document.body.querySelectorAll('label')].find((l) =>
      l.textContent.includes('Remember in Keychain'),
    )
    remember?.click()
    await flush()

    expect(document.body.querySelector<HTMLInputElement>('#server-secret')?.placeholder ?? '').toBe('')
  })

  it('shows no "saved" placeholder when nothing is stored', async () => {
    await renderSheet({ mode: 'edit', server: SAVED })

    expect(document.body.querySelector<HTMLInputElement>('#server-secret')?.placeholder ?? '').toBe('')
  })

  /** Windows names no store on the checkbox or in the field; Linux names the keyring. */
  it.each([
    ['windows', 'Remember password', 'Saved. Leave empty to keep it.'],
    ['linux', 'Remember in keyring', 'Saved in your keyring. Leave empty to keep it.'],
  ] as const)('names the password store in %s’s words', async (platform, checkbox, placeholder) => {
    onTestFinished(withPlatform(platform))
    const commands = await import('#lib/tauri-commands/index.js')
    vi.mocked(commands.hasServerSecret).mockResolvedValueOnce(true)
    await renderSheet({ mode: 'edit', server: SAVED })

    expect(document.body.querySelector<HTMLInputElement>('#server-secret')?.placeholder).toBe(placeholder)
    expect([...document.body.querySelectorAll('label')].some((l) => l.textContent.includes(checkbox))).toBe(true)
  })
})
