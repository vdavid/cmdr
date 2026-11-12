/**
 * The sheet editing an SMB host: a manual-server entry, not an account with a place.
 * Its edit renames it, sets its account, and a new address MOVES it
 * (`src-tauri/src/server_move_smb.rs`).
 */

import { describe, expect, it, vi, beforeEach, afterEach, onTestFinished } from 'vitest'
import { mount, tick } from 'svelte'
import SignInSheet from './SignInSheet.svelte'
import type { SignInSheetRequest } from './sign-in-contract'
import { withPlatform } from '#lib/platform/test-platform.js'

vi.mock('#lib/tauri-commands/index.js', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  updateSavedServer: vi.fn(() => Promise.resolve({ outcome: 'saved' })),
  updateSavedSmbHost: vi.fn(() => Promise.resolve({ outcome: 'saved' })),
  ejectAndUpdateSavedSmbHost: vi.fn(() => Promise.resolve({ outcome: 'settled', answer: { outcome: 'saved' } })),
  cancelServerConnect: vi.fn(() => Promise.resolve(true)),
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(() => Promise.resolve(null)) }))

afterEach(() => {
  document.body.innerHTML = ''
})

/** Drains the sheet's `onMount` seeding and any save in flight. */
async function flush(times = 8) {
  for (let i = 0; i < times; i++) {
    await Promise.resolve()
    await tick()
  }
}

async function renderSheet(request: SignInSheetRequest) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const done: unknown[] = []
  mount(SignInSheet, {
    target,
    props: {
      request,
      onDone: (result) => {
        done.push(result)
      },
    },
  })
  await flush()
  return { done }
}

function typeInto(input: HTMLInputElement, value: string) {
  input.value = value
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

function field(id: string): HTMLInputElement {
  const input = document.body.querySelector<HTMLInputElement>(`#${id}`)
  if (!input) throw new Error(`no field #${id}`)
  return input
}

/** The sheet's own buttons live in the portalled dialog. */
function buttonSaying(text: string): HTMLButtonElement {
  const match = [...document.body.querySelectorAll('button')].find((b) => b.textContent.includes(text))
  if (!match) throw new Error(`no button saying ${text}`)
  return match
}

const SMB_HOST = {
  id: 'manual-192-168-0-153-445',
  protocol: 'smb' as const,
  displayName: "Sven's NAS",
  nameSource: 'user' as const,
  address: '192.168.0.153',
  username: null,
  pinned: false,
  lastConnectedAt: null,
  autoReconnect: null,
  places: [],
}

describe('SignInSheet: editing an SMB host', () => {
  // These pin the words a Mac reads; the Windows and Linux ones are pinned by name below.
  beforeEach(() => withPlatform('macos'))
  beforeEach(async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    vi.mocked(commands.updateSavedSmbHost).mockClear()
    vi.mocked(commands.updateSavedServer).mockClear()
    vi.mocked(commands.ejectAndUpdateSavedSmbHost).mockClear()
    vi.mocked(commands.cancelServerConnect).mockClear()
  })

  it('opens on the name a person typed, with the address open to a move', async () => {
    await renderSheet({ mode: 'edit', server: SMB_HOST })

    expect(field('server-name').value).toBe("Sven's NAS")
    expect(field('server-address').value).toBe('192.168.0.153')
    expect(field('server-address').disabled).toBe(false)
    expect(document.body.querySelector('#server-secret')).toBeNull()
  })

  /**
   * ❗ "Edit server…" from a row menu has to be typeable at once, on the first field Edit
   * lets a person change: the address, the likely reason to open Edit now that a host
   * can move (`DETAILS.md` § "Edit mode opens on the first field it lets a person change").
   */
  it('opens with the keyboard on the address, the first field Edit lets a person change', async () => {
    await renderSheet({ mode: 'edit', server: SMB_HOST })
    expect(document.activeElement).toBe(field('server-address'))
  })

  /** The address says what typing a new one does, SMB's own way: each share follows at its first open there. */
  it('says under the address that typing a new one moves the server', async () => {
    await renderSheet({ mode: 'edit', server: SMB_HOST })
    expect(document.body.querySelector('#server-address-help')?.textContent.trim()).toBe(
      'Did the server move? Type its new address. Its shares and saved password come along, and each share’s favorites and open tabs follow once that share opens at the new address.',
    )
    expect(field('server-address').getAttribute('aria-describedby')).toBe('server-address-help')
  })

  it('sends a new address to the SMB host writer, which moves the host', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    const { done } = await renderSheet({ mode: 'edit', server: SMB_HOST })
    typeInto(field('server-address'), 'nas.tail1234.ts.net')
    await tick()

    buttonSaying('Save').click()
    await flush()

    expect(commands.updateSavedSmbHost).toHaveBeenCalledWith(
      'manual-192-168-0-153-445',
      "Sven's NAS",
      null,
      'smb://nas.tail1234.ts.net',
    )
    expect(done).toEqual([{ kind: 'saved' }])
  })

  /** ❗ A share still mounted from the old address refuses the move, naming the share, and nothing closes. */
  it('names the share to eject when one is still mounted from the old address', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    vi.mocked(commands.updateSavedSmbHost).mockResolvedValueOnce({ outcome: 'share_mounted', name: 'Photos' })
    const { done } = await renderSheet({ mode: 'edit', server: SMB_HOST })
    typeInto(field('server-address'), '192.168.0.200')
    await tick()

    buttonSaying('Save').click()
    await flush()

    expect(document.body.querySelector('#server-address-refusal')?.textContent.trim()).toBe(
      'The share Photos is still mounted from the old address. Eject it, then save again.',
    )
    expect(done).toEqual([])
  })

  /** Opens the sheet, types a new address, and presses Save onto the `share_mounted` refusal. */
  async function refusedOverAMountedShare() {
    const commands = await import('#lib/tauri-commands/index.js')
    vi.mocked(commands.updateSavedSmbHost).mockResolvedValueOnce({ outcome: 'share_mounted', name: 'Photos' })
    const sheet = await renderSheet({ mode: 'edit', server: SMB_HOST })
    typeInto(field('server-address'), '192.168.0.200')
    await tick()
    buttonSaying('Save').click()
    await flush()
    return sheet
  }

  /**
   * ❗ The NAS got a new IP, so the old mount is dead and ejecting it by hand hangs. The
   * refusal offers the way past it, and the person's click is what ejects.
   */
  it('offers Eject and save on a share still mounted from the old address, which ejects and saves', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    const { done } = await refusedOverAMountedShare()
    expect(commands.ejectAndUpdateSavedSmbHost).not.toHaveBeenCalled()
    expect(document.body.querySelector('#server-eject-and-save-help')?.textContent.trim()).toBe(
      'Cmdr ejects every share still mounted from the old address, then saves the new one.',
    )

    buttonSaying('Eject and save').click()
    await flush()

    expect(commands.ejectAndUpdateSavedSmbHost).toHaveBeenCalledWith(
      'manual-192-168-0-153-445',
      "Sven's NAS",
      null,
      'smb://192.168.0.200',
      expect.any(String),
    )
    expect(done).toEqual([{ kind: 'saved' }])
  })

  /** ❗ A refused eject is a live mount someone is using: it says who, saves nothing, and offers the button again. */
  it('says who is holding a share whose eject was refused, and saves nothing', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    vi.mocked(commands.ejectAndUpdateSavedSmbHost).mockResolvedValueOnce({
      outcome: 'eject_refused',
      name: 'Photos',
      error: {
        type: 'unmountRefused',
        holders: { type: 'complete', named: [{ pid: 4242, name: 'Preview', bundleId: null, kind: 'app' }] },
        detail: 'diskutil unmount: dissented by PID 4242',
      },
    })
    const { done } = await refusedOverAMountedShare()

    buttonSaying('Eject and save').click()
    await flush()

    expect(document.body.querySelector('#server-address-refusal')?.textContent.trim()).toBe(
      'Cmdr couldn’t eject Photos, so nothing was saved. Preview is still using something there. Close that app, then try again.',
    )
    expect(buttonSaying('Eject and save')).toBeTruthy()
    expect(done).toEqual([])
  })

  /** Cancel while it runs calls the run off, so a closed sheet never saves the edit. */
  it('calls a running Eject and save off when the sheet is canceled', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    vi.mocked(commands.ejectAndUpdateSavedSmbHost).mockReturnValueOnce(new Promise(() => {}))
    const { done } = await refusedOverAMountedShare()

    buttonSaying('Eject and save').click()
    await flush()
    expect(buttonSaying('Ejecting…').getAttribute('aria-disabled')).toBe('true')
    const attemptId = vi.mocked(commands.ejectAndUpdateSavedSmbHost).mock.calls[0][4]

    buttonSaying('Cancel').click()
    await flush()

    expect(commands.cancelServerConnect).toHaveBeenCalledWith(attemptId)
    expect(done).toEqual([{ kind: 'cancelled' }])
  })

  /** A network share is disconnected on Windows and unmounted on Linux, never ejected. */
  it.each([
    [
      'windows',
      'The share Photos is still connected from the old address. Disconnect it, then save again.',
      'Disconnect and save',
      'Cmdr disconnects every share still connected from the old address, then saves the new one.',
    ],
    [
      'linux',
      'The share Photos is still mounted from the old address. Unmount it, then save again.',
      'Unmount and save',
      'Cmdr unmounts every share still mounted from the old address, then saves the new one.',
    ],
  ] as const)(
    'words a share still mounted from the old address in %s’s words',
    async (platform, refusal, button, help) => {
      onTestFinished(withPlatform(platform))
      await refusedOverAMountedShare()
      expect(document.body.querySelector('#server-address-refusal')?.textContent.trim()).toBe(refusal)
      expect(document.body.querySelector('#server-eject-and-save-help')?.textContent.trim()).toBe(help)
      expect(buttonSaying(button)).toBeTruthy()
    },
  )

  it('offers no Eject and save on a refusal that no eject fixes', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    vi.mocked(commands.updateSavedSmbHost).mockResolvedValueOnce({ outcome: 'address_taken', name: 'Office NAS' })
    await renderSheet({ mode: 'edit', server: SMB_HOST })
    typeInto(field('server-address'), '192.168.0.200')
    await tick()
    buttonSaying('Save').click()
    await flush()

    expect([...document.body.querySelectorAll('button')].some((b) => b.textContent.includes('Eject and save'))).toBe(
      false,
    )
  })

  it('opens an unnamed host with an empty name and the address as its placeholder', async () => {
    await renderSheet({ mode: 'edit', server: { ...SMB_HOST, displayName: '192.168.0.153', nameSource: 'fallback' } })
    expect(field('server-name').value).toBe('')
    expect(field('server-name').placeholder).toBe('Leave empty to use 192.168.0.153')
  })

  it('saves a rename and a changed account through the SMB host writer and closes', async () => {
    const commands = await import('#lib/tauri-commands/index.js')
    const { done } = await renderSheet({ mode: 'edit', server: { ...SMB_HOST, username: 'sven' } })
    const username = field('server-username')
    // ❗ Editable: for SMB the account is a preference, not the server's identity.
    expect(username.value).toBe('sven')
    expect(username.disabled).toBe(false)
    typeInto(field('server-name'), 'Attic NAS')
    typeInto(username, 'bob')
    await tick()

    buttonSaying('Save').click()
    await flush()

    expect(commands.updateSavedSmbHost).toHaveBeenCalledWith(
      'manual-192-168-0-153-445',
      'Attic NAS',
      'bob',
      'smb://192.168.0.153',
    )
    expect(commands.updateSavedServer).not.toHaveBeenCalled()
    expect(done).toEqual([{ kind: 'saved' }])
  })
})
