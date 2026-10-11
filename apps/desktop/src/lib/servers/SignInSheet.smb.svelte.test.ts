/**
 * The sheet editing an SMB host: a manual-server entry, not an account with a place.
 * Its edit renames it, sets its account, and a new address MOVES it
 * (`src-tauri/src/server_move_smb.rs`).
 */

import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import SignInSheet from './SignInSheet.svelte'
import type { SignInSheetRequest } from './sign-in-contract'

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  updateSavedServer: vi.fn(() => Promise.resolve({ outcome: 'saved' })),
  updateSavedSmbHost: vi.fn(() => Promise.resolve({ outcome: 'saved' })),
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
  beforeEach(async () => {
    const commands = await import('$lib/tauri-commands')
    vi.mocked(commands.updateSavedSmbHost).mockClear()
    vi.mocked(commands.updateSavedServer).mockClear()
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
      'Did the server move? Type its new address. Its shares and saved password come along, and each share’s favorites and open tabs follow it the first time it opens there.',
    )
    expect(field('server-address').getAttribute('aria-describedby')).toBe('server-address-help')
  })

  it('sends a new address to the SMB host writer, which moves the host', async () => {
    const commands = await import('$lib/tauri-commands')
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
    const commands = await import('$lib/tauri-commands')
    vi.mocked(commands.updateSavedSmbHost).mockResolvedValueOnce({ outcome: 'share_mounted', name: 'Photos' })
    const { done } = await renderSheet({ mode: 'edit', server: SMB_HOST })
    typeInto(field('server-address'), '192.168.0.200')
    await tick()

    buttonSaying('Save').click()
    await flush()

    expect(document.body.querySelector('#server-address-refusal')?.textContent.trim()).toBe(
      'Photos is still mounted from the old address. Eject it, then save again.',
    )
    expect(done).toEqual([])
  })

  it('opens an unnamed host with an empty name and the address as its placeholder', async () => {
    await renderSheet({ mode: 'edit', server: { ...SMB_HOST, displayName: '192.168.0.153', nameSource: 'fallback' } })
    expect(field('server-name').value).toBe('')
    expect(field('server-name').placeholder).toBe('Leave empty to use 192.168.0.153')
  })

  it('saves a rename and a changed account through the SMB host writer and closes', async () => {
    const commands = await import('$lib/tauri-commands')
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
