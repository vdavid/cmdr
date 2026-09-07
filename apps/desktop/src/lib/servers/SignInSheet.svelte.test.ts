/**
 * What the sheet must get right about the two moments a person is most likely
 * to be in it: a password that didn't work, and reaching for the next field.
 *
 * ❗ The Tab cell is not a formality. In Cmdr's own model Tab switches PANES, and
 * an in-pane sign-in form spent its whole life fighting that (the old SMB form
 * carries a hand-rolled `stopPropagation` and a focus-trap opt-out for exactly
 * this). A modal is what settles it, and this is the cell that says so.
 */

import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import { mount, tick } from 'svelte'
import SignInSheet from './SignInSheet.svelte'
import type { SignInAttemptOutcome, SignInSheetRequest, SignInSubmission } from './sign-in-contract'

vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  hasServerSecret: vi.fn(() => Promise.resolve(false)),
  getKnownSftpServers: vi.fn(() => Promise.resolve([])),
  getKnownWebdavServers: vi.fn(() => Promise.resolve([])),
  getSftpUnattendedReconnect: vi.fn(() => Promise.resolve('ready')),
  getWebdavUnattendedReconnect: vi.fn(() => Promise.resolve('possible')),
  forgetServerSecret: vi.fn(() => Promise.resolve(true)),
  updateSavedServer: vi.fn(() => Promise.resolve()),
  saveSftpCredentials: vi.fn(() => Promise.resolve()),
  saveWebdavCredentials: vi.fn(() => Promise.resolve()),
  approveSftpHostKey: vi.fn(() => Promise.resolve({ outcome: 'recorded' })),
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(() => Promise.resolve(null)) }))

afterEach(() => {
  document.body.innerHTML = ''
})

const endpoint = {
  protocol: 'sftp' as const,
  displayName: 'Naspolya',
  address: 'ada@nas.local:22',
  host: 'nas.local',
  username: 'ada',
}

let submissions: SignInSubmission[] = []

beforeEach(() => {
  submissions = []
})

/** Drains the sheet's `onMount` seeding and any attempt in flight. */
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
  return { target, done }
}

function typeInto(input: HTMLInputElement, value: string) {
  input.value = value
  input.dispatchEvent(new Event('input', { bubbles: true }))
}

/** The sheet's own buttons live in the portalled dialog, not under `target`. */
function buttonSaying(text: string): HTMLButtonElement {
  const match = [...document.body.querySelectorAll('button')].find((b) => b.textContent.includes(text))
  if (!match) throw new Error(`no button saying ${text}`)
  return match
}

describe('SignInSheet: a refusal', () => {
  it('renders the sentence under the password field and leaves the focus there', async () => {
    const attempt = (submission: SignInSubmission): Promise<SignInAttemptOutcome> => {
      submissions.push(submission)
      return Promise.resolve({ kind: 'refused', refusal: 'authentication_rejected' })
    }
    await renderSheet({ mode: 'sign-in', remembered: false, endpoint, shape: { kind: 'password' }, attempt })

    const secret = document.body.querySelector<HTMLInputElement>('#sign-in-secret')
    expect(secret).not.toBeNull()
    typeInto(secret as HTMLInputElement, 'hunter2')
    await tick()

    buttonSaying('Sign in').click()
    await flush()

    // ❗ Under the field the sentence is about. The same words above the form
    // read as being about the whole form, which is a puzzle rather than an
    // instruction.
    const inline = document.body.querySelector('#sign-in-secret-refusal')
    expect(inline?.textContent).toContain("That password didn't work for ada")
    expect(secret?.getAttribute('aria-invalid')).toBe('true')
    expect(secret?.getAttribute('aria-describedby')).toBe('sign-in-secret-refusal')

    // And the caret is where the retry happens, so trying again is one keystroke
    // rather than a hunt for the field.
    expect(document.activeElement).toBe(secret)
  })

  it('offers the secret exactly once per press, and never a username the shape calls read-only', async () => {
    const attempt = (submission: SignInSubmission): Promise<SignInAttemptOutcome> => {
      submissions.push(submission)
      return Promise.resolve({ kind: 'refused', refusal: 'authentication_rejected' })
    }
    await renderSheet({ mode: 'sign-in', remembered: false, endpoint, shape: { kind: 'password' }, attempt })

    typeInto(document.body.querySelector<HTMLInputElement>('#sign-in-secret') as HTMLInputElement, 'hunter2')
    await tick()
    buttonSaying('Sign in').click()
    await flush()

    expect(submissions).toHaveLength(1)
    const only = submissions[0]
    expect(only.mode).toBe('sign-in')
    if (only.mode !== 'sign-in') throw new Error('unreachable')
    expect(only.secret).toEqual({ secret: 'hunter2', remember: false })
    // ❗ `password` renders the account read-only because SFTP's reconnect
    // refuses a changed one: the volume id IS the account.
    expect(only.username).toBeNull()
  })
})

describe('SignInSheet: the host-key step', () => {
  it('starts over on the key the server really presents when the approval is superseded', async () => {
    const { approveSftpHostKey } = await import('$lib/tauri-commands')
    const approve = vi.mocked(approveSftpHostKey)
    approve.mockResolvedValueOnce({
      outcome: 'superseded',
      host: 'nas.local',
      port: 22,
      algorithm: 'ssh-ed25519',
      fingerprint: 'SHA256:THE-REAL-ONE',
      kind: 'changed',
    })
    const attempt = (): Promise<SignInAttemptOutcome> => Promise.resolve({ kind: 'connected', volumeId: 'v' })

    await renderSheet({
      mode: 'sign-in',
      remembered: false,
      endpoint,
      shape: { kind: 'password' },
      hostKey: {
        host: 'nas.local',
        port: 22,
        algorithm: 'ssh-ed25519',
        fingerprint: 'SHA256:THE-ONE-ON-SCREEN',
        kind: 'unknown',
      },
      attempt,
    })
    expect(document.body.textContent).toContain('SHA256:THE-ONE-ON-SCREEN')

    buttonSaying('Trust and connect').click()
    await flush()

    // ❗ Nothing was recorded, so the step starts over on the REAL key rather
    // than silently trusting the one the user was just looking at. And a key
    // that CHANGED never wears first contact's plain primary button.
    expect(document.body.textContent).toContain('SHA256:THE-REAL-ONE')
    expect(document.body.textContent).not.toContain('SHA256:THE-ONE-ON-SCREEN')
    expect(document.body.textContent).toContain("nas.local's key changed")
  })
})

describe('SignInSheet: the keyboard', () => {
  it('keeps Tab inside the sheet, so it never switches panes', async () => {
    const attempt = (): Promise<SignInAttemptOutcome> => Promise.resolve({ kind: 'cancelled' })
    await renderSheet({
      mode: 'sign-in',
      remembered: false,
      endpoint,
      shape: { kind: 'username_password', guestAllowed: false },
      attempt,
    })

    const username = document.body.querySelector<HTMLInputElement>('#sign-in-username')
    const secret = document.body.querySelector<HTMLInputElement>('#sign-in-secret')
    expect(username).not.toBeNull()
    expect(secret).not.toBeNull()

    // The pane switcher listens on the window. A Tab that reaches it would move
    // the user out of a form they are halfway through typing into.
    let reachedTheApp = false
    const spy = () => {
      reachedTheApp = true
    }
    window.addEventListener('keydown', spy)
    try {
      ;(username as HTMLInputElement).focus()
      ;(username as HTMLInputElement).dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }))
      await tick()
    } finally {
      window.removeEventListener('keydown', spy)
    }
    expect(reachedTheApp).toBe(false)

    // And both fields are reachable: the dialog's focus trap owns the order.
    ;(secret as HTMLInputElement).focus()
    expect(document.activeElement).toBe(secret)
  })
})

describe('SignInSheet: add mode', () => {
  it('flips the protocol to what the address says, and sends the target that names', async () => {
    const attempt = (submission: SignInSubmission): Promise<SignInAttemptOutcome> => {
      submissions.push(submission)
      return Promise.resolve({ kind: 'refused', refusal: 'unreachable' })
    }
    await renderSheet({ mode: 'add', attempt })

    typeInto(document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement, 'ada@nas.local:2222')
    await tick()
    // The username field only exists once the address stopped reading as SMB,
    // which is the toggle having flipped.
    const username = document.body.querySelector<HTMLInputElement>('#server-username')
    expect(username?.value).toBe('ada')

    buttonSaying('Connect').click()
    await flush()

    expect(submissions).toHaveLength(1)
    const only = submissions[0]
    if (only.mode !== 'add') throw new Error('expected an add submission')
    expect(only.target).toMatchObject({ protocol: 'sftp', host: 'nas.local', port: 2222, username: 'ada' })
  })

  it('hands an SMB address off rather than dialing it, and asks for no password', async () => {
    const attempt = (submission: SignInSubmission): Promise<SignInAttemptOutcome> => {
      submissions.push(submission)
      return Promise.resolve({ kind: 'handed_off' })
    }
    const { done } = await renderSheet({ mode: 'add', attempt })

    typeInto(document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement, 'naspolya')
    await tick()
    // ❗ SMB's connect is a share MOUNT: nothing is asked until a listing or a
    // mount refuses, so there is no password field to put in front of anyone.
    expect(document.body.querySelector('#server-secret')).toBeNull()

    buttonSaying('Connect').click()
    await flush()

    expect(submissions).toEqual([{ mode: 'add_smb', address: 'naspolya' }])
    expect(done).toEqual([{ kind: 'handed_off' }])
  })
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
    places: [],
  }

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
    const commands = await import('$lib/tauri-commands')
    vi.mocked(commands.getKnownSftpServers).mockResolvedValue([KNOWN_SFTP])
    vi.mocked(commands.saveSftpCredentials).mockClear()
    vi.mocked(commands.forgetServerSecret).mockClear()
  })

  it('locks the identity and points at the honest way to change it', async () => {
    await renderSheet({ mode: 'edit', server: SAVED })

    const address = document.body.querySelector<HTMLInputElement>('#server-address')
    const username = document.body.querySelector<HTMLInputElement>('#server-username')
    expect(address?.value).toBe('ada@nas.local:22')
    expect(address?.disabled).toBe(true)
    expect(username?.disabled).toBe(true)
    // ❗ An inert-looking field with no explanation is worse than no field. The
    // hint names the path that actually works.
    expect(document.body.textContent).toContain('Forget this server and add it again')
  })

  it('writes a typed password through the Keychain, so the field does what it shows', async () => {
    const commands = await import('$lib/tauri-commands')
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

  it('leaves the store alone when the password field is left empty', async () => {
    const commands = await import('$lib/tauri-commands')
    await renderSheet({ mode: 'edit', server: SAVED })

    buttonSaying('Save').click()
    await flush()

    // An empty box means "I didn't come here to change the password", ❌ never
    // "store an empty one".
    expect(vi.mocked(commands.saveSftpCredentials)).not.toHaveBeenCalled()
    expect(vi.mocked(commands.forgetServerSecret)).not.toHaveBeenCalled()
  })
})

describe('SignInSheet: a refusal and the address that earned it', () => {
  it('stops accusing the old host once the address is corrected', async () => {
    const attempt = (submission: SignInSubmission): Promise<SignInAttemptOutcome> => {
      submissions.push(submission)
      return Promise.resolve({ kind: 'refused', refusal: 'unreachable' })
    }
    await renderSheet({ mode: 'add', attempt })

    const address = document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement
    typeInto(address, 'ada@typo.local:22')
    await tick()
    buttonSaying('Connect').click()
    await flush()

    // The refusal names the host that was actually dialed.
    expect(document.body.textContent).toContain('typo.local')

    // Correcting the address retires it. Pre-fix the sentence survived AND
    // re-interpolated the live field, so a host Cmdr never contacted was on
    // screen being called unreachable.
    typeInto(address, 'ada@nas.local:22')
    await tick()
    expect(document.body.textContent).not.toContain('nas.local.')
    expect(document.body.textContent).not.toContain('typo.local')
  })
})
