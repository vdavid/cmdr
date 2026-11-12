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
import { tick } from 'svelte'
import type { SignInAttemptOutcome, SignInSubmission } from './sign-in-contract'
import { systemStrings } from '#lib/system-strings.svelte.js'
import { buttonSaying, exactButton, flush, protocolTab, renderSheet, typeInto } from './test-helpers'

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
  savedServerId: vi.fn(() => Promise.resolve('sftp-nas-local-22-test_user_1')),
  updateSavedSmbHost: vi.fn(() => Promise.resolve({ outcome: 'saved' })),
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
  displayName: 'Testnas',
  address: 'test_user_1@testnas.local:22',
  host: 'testnas.local',
  username: 'test_user_1',
}

let submissions: SignInSubmission[] = []

beforeEach(() => {
  submissions = []
})

async function pickProtocol(label: string) {
  protocolTab(label).click()
  await tick()
}

/** ❗ The caret goes where the typing starts: Username when it's empty and editable, Password otherwise. */
describe('SignInSheet: where the caret starts', () => {
  const attempt = (): Promise<SignInAttemptOutcome> => Promise.resolve({ kind: 'handed_off' })

  it('starts on Username when it is editable and empty', async () => {
    await renderSheet({
      mode: 'sign-in',
      remembered: true,
      endpoint: { ...endpoint, username: undefined },
      shape: { kind: 'username_password', guestAllowed: false },
      attempt,
    })
    expect(document.activeElement).toBe(document.body.querySelector('#sign-in-username'))
  })

  it('starts on Password when the username is already there', async () => {
    await renderSheet({
      mode: 'sign-in',
      remembered: true,
      endpoint: { ...endpoint, username: 'test_user_1' },
      shape: { kind: 'username_password', guestAllowed: false },
      attempt,
    })
    expect(document.activeElement).toBe(document.body.querySelector('#sign-in-secret'))
  })
})

/**
 * ❗ Red is feedback on something the person did. A server asking who they are is
 * the reason the sheet opened, which the title already says, so a sheet that opens
 * on `needs_credentials` opens clean (cmdr-reports#10).
 */
describe('SignInSheet: the refusal it opens with', () => {
  const attempt = (): Promise<SignInAttemptOutcome> => Promise.resolve({ kind: 'handed_off' })
  const smbShape = { kind: 'username_password', guestAllowed: false } as const

  it('opens clean when the server only asked for an account', async () => {
    await renderSheet({
      mode: 'sign-in',
      remembered: true,
      endpoint,
      shape: smbShape,
      refusal: 'needs_credentials',
      attempt,
    })
    expect(document.body.querySelector('#sign-in-secret-refusal')).toBeNull()
    expect(document.body.querySelector('.form-refusal')).toBeNull()
    expect(document.body.querySelector('#sign-in-secret')?.getAttribute('aria-invalid')).not.toBe('true')
  })

  it('still says so when a password the person had already given was turned away', async () => {
    await renderSheet({
      mode: 'sign-in',
      remembered: true,
      endpoint,
      shape: smbShape,
      refusal: 'authentication_rejected',
      attempt,
    })
    expect(document.body.querySelector('#sign-in-secret-refusal')?.textContent).toContain(
      'That password didn’t work for test_user_1',
    )
  })
})

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
    expect(inline?.textContent).toContain('That password didn’t work for test_user_1')
    expect(secret?.getAttribute('aria-invalid')).toBe('true')
    expect(secret?.getAttribute('aria-describedby')).toBe('sign-in-secret-refusal')

    // And the caret is where the retry happens, so trying again is one keystroke
    // rather than a hunt for the field.
    expect(document.activeElement).toBe(secret)
  })

  it('says a refusal about the SERVER in sign-in mode too, where there is no address field to put it under', async () => {
    // ❗ `unreachable` and `timed_out` belong under the address, which only add
    // and edit mode render. A sign-in round refused that way showed nothing,
    // and the person was left pressing Sign in at a sheet that said no word.
    const attempt = (): Promise<SignInAttemptOutcome> => Promise.resolve({ kind: 'refused', refusal: 'unreachable' })
    await renderSheet({ mode: 'sign-in', remembered: false, endpoint, shape: { kind: 'password' }, attempt })

    typeInto(document.body.querySelector<HTMLInputElement>('#sign-in-secret') as HTMLInputElement, 'hunter2')
    await tick()
    buttonSaying('Sign in').click()
    await flush()

    const alert = document.body.querySelector('.form-refusal')
    expect(alert?.getAttribute('role')).toBe('alert')
    expect(alert?.textContent).toContain('Cmdr couldn’t reach testnas.local.')
  })

  it('lets the person try again when an attempt breaks down instead of answering', async () => {
    // ❗ An attempt promises an outcome. One that throws anyway must not leave
    // the sheet disabled behind a spinner nothing will ever stop.
    const attempt = (): Promise<SignInAttemptOutcome> => Promise.reject(new Error('the bridge is gone'))
    await renderSheet({ mode: 'sign-in', remembered: false, endpoint, shape: { kind: 'password' }, attempt })

    typeInto(document.body.querySelector<HTMLInputElement>('#sign-in-secret') as HTMLInputElement, 'hunter2')
    await tick()
    buttonSaying('Sign in').click()
    await flush()

    expect(buttonSaying('Sign in').disabled).toBe(false)
    expect(document.body.querySelector('.form-refusal')?.textContent).toContain('Cmdr couldn’t reach testnas.local.')
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

/** ❗ #530: a connection that came up while the Keychain refused the checked box is said out loud. */
describe('SignInSheet: connected, but the password was not kept', () => {
  it('stays open on the notice under the password, and closes as connected', async () => {
    const attempt = (): Promise<SignInAttemptOutcome> =>
      Promise.resolve({ kind: 'connected', volumeId: 'v', rememberRefused: true })
    const { done } = await renderSheet({
      mode: 'sign-in',
      remembered: true,
      endpoint,
      shape: { kind: 'password' },
      attempt,
    })

    typeInto(document.body.querySelector<HTMLInputElement>('#sign-in-secret') as HTMLInputElement, 'hunter2')
    await tick()
    buttonSaying('Sign in').click()
    await flush()

    expect(done).toEqual([])
    expect(document.body.querySelector('#sign-in-secret-refusal')?.textContent).toContain(
      'couldn’t store this password',
    )

    // ❗ The volume IS connected, so leaving the sheet says so rather than "cancelled".
    buttonSaying('Cancel').click()
    await flush()
    expect(done).toEqual([{ kind: 'connected', volumeId: 'v' }])
  })
})

/** A first-contact SSH host key for `testnas.local`, as the sheet's trust step takes it. */
const hostKey = (fingerprint: string) =>
  ({
    subject: 'host_key',
    prompt: { host: 'testnas.local', port: 22, algorithm: 'ssh-ed25519', fingerprint, kind: 'unknown' },
  }) as const

describe('SignInSheet: the host-key step', () => {
  it('starts over on the key the server really presents when the approval is superseded', async () => {
    const { approveSftpHostKey } = await import('#lib/tauri-commands/index.js')
    const approve = vi.mocked(approveSftpHostKey)
    approve.mockResolvedValueOnce({
      outcome: 'superseded',
      host: 'testnas.local',
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
      trust: hostKey('SHA256:THE-ONE-ON-SCREEN'),
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
    expect(document.body.textContent).toContain('The key for testnas.local changed')
  })

  /**
   * ❗ An offered secret wins over the stored one for its dial, so an EMPTY one offered
   * after a trusted key turned a remembered password into "That password didn't work"
   * (first seen after moving a saved server to a new address, whose key is new).
   */
  it('dials past a freshly trusted key with no secret when nothing was typed, so a stored one answers', async () => {
    const attempt = (submission: SignInSubmission): Promise<SignInAttemptOutcome> => {
      submissions.push(submission)
      return Promise.resolve({ kind: 'connected', volumeId: 'v' })
    }
    await renderSheet({
      mode: 'sign-in',
      remembered: true,
      endpoint,
      shape: { kind: 'password' },
      trust: hostKey('SHA256:NEW'),
      attempt,
    })

    buttonSaying('Trust and connect').click()
    await flush()

    expect(submissions).toHaveLength(1)
    expect(submissions[0]).toMatchObject({ mode: 'sign-in', secret: null })
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

/**
 * cmdr-reports#6: the title says "Add server" and the buttons say what each one
 * does. Both verify before saving; only "Add and open" moves a pane.
 */
describe('SignInSheet: Add and Add and open', () => {
  const recording = (outcome: SignInAttemptOutcome) => (submission: SignInSubmission) => {
    submissions.push(submission)
    return Promise.resolve(outcome)
  }

  it('offers Add beside the primary Add and open, and Enter means Add and open', async () => {
    await renderSheet({ mode: 'add', attempt: recording({ kind: 'handed_off' }) })
    const address = document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement
    typeInto(address, 'testnas')
    await tick()

    expect(buttonSaying('Add and open').className).toContain('primary')
    expect(buttonSaying('Add').textContent.trim()).toBe('Add')

    address.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
    await flush()
    expect(submissions).toEqual([
      { mode: 'add_smb', address: 'smb://testnas', name: '', username: null, intent: 'open' },
    ])
  })

  it('saves without opening on Add, and closes as added', async () => {
    const { done } = await renderSheet({
      mode: 'add',
      attempt: recording({ kind: 'added', serverId: 'manual-testnas-445' }),
    })
    typeInto(document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement, 'testnas')
    await tick()

    exactButton('Add').click()
    await flush()

    expect(submissions).toEqual([
      { mode: 'add_smb', address: 'smb://testnas', name: '', username: null, intent: 'save' },
    ])
    expect(done).toEqual([{ kind: 'added', serverId: 'manual-testnas-445' }])
  })

  it('offers Add anyway once the server could not be reached, saying nothing was checked', async () => {
    await renderSheet({ mode: 'add', attempt: recording({ kind: 'refused', refusal: 'unreachable' }) })
    typeInto(document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement, 'testnas')
    await tick()
    expect(document.body.querySelector('#server-add-anyway-help')).toBeNull()

    buttonSaying('Add and open').click()
    await flush()
    expect(document.body.querySelector('#server-add-anyway-help')?.textContent).toContain('couldn’t check')

    buttonSaying('Add anyway').click()
    await flush()
    expect(submissions.at(-1)).toMatchObject({ mode: 'add_smb', intent: 'save_unchecked' })
  })

  /**
   * ERR-XGS9X: a stuck Local Network permission looks like a server that's off, so
   * the hint only suggests, under the same sentence, and Add anyway stays.
   */
  it('hints at the Local Network permission under the unreachable sentence when the probe points at it', async () => {
    await renderSheet({
      mode: 'add',
      attempt: recording({ kind: 'refused', refusal: 'unreachable', hint: 'local_network_permission' }),
    })
    typeInto(document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement, '192.168.50.20')
    await tick()
    buttonSaying('Add and open').click()
    await flush()

    const refusal = document.body.querySelector('#server-address-refusal')?.textContent ?? ''
    expect(refusal).toContain('reach 192.168.50.20.')
    expect(document.body.querySelector('#server-address-hint')?.textContent).toContain(systemStrings.localNetwork)
    expect(buttonSaying('Add anyway')).toBeTruthy()

    // Editing the address retires the refusal, and its hint with it.
    typeInto(document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement, '192.168.0.154')
    await tick()
    expect(document.body.querySelector('#server-address-hint')).toBeNull()
  })

  it('shows no Local Network hint for a server that just did not answer', async () => {
    await renderSheet({ mode: 'add', attempt: recording({ kind: 'refused', refusal: 'unreachable' }) })
    typeInto(document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement, 'testnas')
    await tick()
    buttonSaying('Add and open').click()
    await flush()

    expect(document.body.querySelector('#server-address-refusal')).not.toBeNull()
    expect(document.body.querySelector('#server-address-hint')).toBeNull()
  })

  /** ❗ "Couldn't reach localhost" named a different server than `localhost:11499`: the port is part of which one. */
  it('names the port in the unreachable sentence when it is not the default', async () => {
    await renderSheet({ mode: 'add', attempt: recording({ kind: 'refused', refusal: 'unreachable' }) })
    typeInto(document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement, 'localhost:11499')
    await tick()
    buttonSaying('Add and open').click()
    await flush()
    expect(document.body.textContent).toContain('localhost:11499')
  })

  it('leaves the default port unsaid', async () => {
    await renderSheet({ mode: 'add', attempt: recording({ kind: 'refused', refusal: 'unreachable' }) })
    typeInto(document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement, 'testnas:445')
    await tick()
    buttonSaying('Add and open').click()
    await flush()
    expect(document.body.textContent).toContain('reach testnas.')
  })

  it('offers no Add anyway for an address that does not parse: that is a typo, not a sleeping server', async () => {
    await renderSheet({ mode: 'add', attempt: recording({ kind: 'refused', refusal: 'invalid_url' }) })
    typeInto(document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement, 'testnas')
    await tick()
    buttonSaying('Add and open').click()
    await flush()
    expect(document.body.querySelector('#server-add-anyway-help')).toBeNull()
  })
})

describe('SignInSheet: add mode', () => {
  it('puts the protocol first, then the address, then the name, with the name outside Advanced', async () => {
    await renderSheet({ mode: 'add', attempt: () => Promise.resolve({ kind: 'cancelled' }) })
    await pickProtocol('SFTP')

    const tablist = document.body.querySelector('[role="tablist"]') as Element
    const address = document.body.querySelector('#server-address') as Element
    const name = document.body.querySelector('#server-name') as Element
    expect(tablist.compareDocumentPosition(address) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(address.compareDocumentPosition(name) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    // ❗ Not an advanced setting, so a closed disclosure must not hide it.
    expect(name.closest('details')).toBeNull()
  })

  it('gives SMB a name field right under the address, saying what an empty one falls back to', async () => {
    await renderSheet({ mode: 'add', attempt: () => Promise.resolve({ kind: 'cancelled' }) })

    const address = document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement
    const name = document.body.querySelector<HTMLInputElement>('#server-name') as HTMLInputElement
    expect(address.compareDocumentPosition(name) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    // SMB keeps nothing Advanced would hold, so there's still no disclosure.
    expect(document.body.querySelector('details')).toBeNull()

    typeInto(address, 'test_user_6@192.168.50.20')
    await tick()
    // ❗ The stand-in the hub would show, which for SMB is the host alone.
    expect(name.placeholder).toBe('Leave empty to use 192.168.50.20')
  })

  /**
   * ❗ The help line speaks for the protocol that's selected. It read "…or a whole
   * ssh line" with SMB selected, sending an SMB user to paste the wrong thing
   * (QA 2026-09-25).
   */
  it('says what the address field takes for the protocol that is selected', async () => {
    await renderSheet({ mode: 'add', attempt: () => Promise.resolve({ kind: 'cancelled' }) })
    const help = () => document.body.querySelector('#server-address-help')?.textContent.trim()

    expect(help()).toBe('A hostname, an IP address, or an smb:// address you copied.')
    await pickProtocol('SFTP')
    expect(help()).toBe('A hostname, an address you copied, or a whole ssh line.')
    await pickProtocol('WebDAV')
    expect(help()).toBe('A web address you copied, like https://nas.local/dav, or a hostname.')
  })

  /** Every protocol's Username field says what goes there, as SMB's "Optional" does (QA round 2). */
  it('gives SFTP and WebDAV usernames an example placeholder', async () => {
    await renderSheet({ mode: 'add', attempt: () => Promise.resolve({ kind: 'cancelled' }) })
    for (const protocol of ['SFTP', 'WebDAV']) {
      await pickProtocol(protocol)
      expect(document.body.querySelector<HTMLInputElement>('#server-username')?.placeholder).toBe('Example: barry')
    }
  })

  it('says an SFTP add falls back to the account and host', async () => {
    await renderSheet({ mode: 'add', attempt: () => Promise.resolve({ kind: 'cancelled' }) })
    await pickProtocol('SFTP')
    typeInto(
      document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement,
      'test_user_1@testnas.local:2222',
    )
    await tick()
    expect(document.body.querySelector<HTMLInputElement>('#server-name')?.placeholder).toBe(
      'Leave empty to use test_user_1@testnas.local',
    )
  })

  it('lets an SMB add name an account without putting it in the address', async () => {
    const attempt = (submission: SignInSubmission): Promise<SignInAttemptOutcome> => {
      submissions.push(submission)
      return Promise.resolve({ kind: 'handed_off' })
    }
    await renderSheet({ mode: 'add', attempt })
    typeInto(document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement, 'testnas')
    typeInto(document.body.querySelector<HTMLInputElement>('#server-username') as HTMLInputElement, ' test_user_6 ')
    await tick()

    buttonSaying('Add and open').click()
    await flush()

    expect(submissions).toEqual([
      { mode: 'add_smb', address: 'smb://testnas', name: '', username: 'test_user_6', intent: 'open' },
    ])
  })

  it('hands the typed name over with an SMB address', async () => {
    const attempt = (submission: SignInSubmission): Promise<SignInAttemptOutcome> => {
      submissions.push(submission)
      return Promise.resolve({ kind: 'handed_off' })
    }
    await renderSheet({ mode: 'add', attempt })
    typeInto(document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement, 'testnas')
    typeInto(document.body.querySelector<HTMLInputElement>('#server-name') as HTMLInputElement, '  Test NAS ')
    await tick()

    buttonSaying('Add and open').click()
    await flush()

    expect(submissions).toEqual([
      { mode: 'add_smb', address: 'smb://testnas', name: 'Test NAS', username: null, intent: 'open' },
    ])
  })

  /**
   * ❗ cmdr-reports#8: `test_user_6@192.168.50.20`, typed for an SMB NAS, flipped the
   * toggle to SFTP and dialed SSH without anyone clicking SFTP.
   */
  it('never moves the protocol while someone types, and dials the one they picked', async () => {
    const attempt = (submission: SignInSubmission): Promise<SignInAttemptOutcome> => {
      submissions.push(submission)
      return Promise.resolve({ kind: 'handed_off' })
    }
    await renderSheet({ mode: 'add', attempt })

    typeInto(
      document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement,
      'test_user_6@192.168.50.20',
    )
    await tick()
    expect(protocolTab('SMB').getAttribute('aria-selected')).toBe('true')
    // SMB asks for no password here; the account the address named is kept.
    expect(document.body.querySelector('#server-secret')).toBeNull()
    expect(document.body.querySelector<HTMLInputElement>('#server-username')?.value).toBe('test_user_6')
    // `user@host` is a fine SMB address, so nothing warns about it.
    expect(document.body.querySelector('#server-address-warning')).toBeNull()

    buttonSaying('Add and open').click()
    await flush()

    expect(submissions).toEqual([
      {
        mode: 'add_smb',
        address: 'smb://test_user_6@192.168.50.20',
        name: '',
        username: 'test_user_6',
        intent: 'open',
      },
    ])
  })

  it('warns when the address looks like another protocol, and leaves the toggle and the dial alone', async () => {
    const attempt = (submission: SignInSubmission): Promise<SignInAttemptOutcome> => {
      submissions.push(submission)
      return Promise.resolve({ kind: 'handed_off' })
    }
    await renderSheet({ mode: 'add', attempt })

    const address = document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement
    typeInto(address, 'sftp://test_user_1@testnas.local')
    await tick()

    const warning = document.body.querySelector('#server-address-warning')
    expect(warning?.textContent).toBe('This looks like an SFTP address. Pick SFTP above if that’s what you meant.')
    expect(address.getAttribute('aria-describedby')).toBe('server-address-warning')
    expect(protocolTab('SMB').getAttribute('aria-selected')).toBe('true')

    // Picking what it says retires the warning.
    await pickProtocol('SFTP')
    expect(document.body.querySelector('#server-address-warning')).toBeNull()
    expect(document.body.querySelector<HTMLInputElement>('#server-username')?.value).toBe('test_user_1')
  })

  it('sends an SFTP target once SFTP is picked, with the port and account the address carried', async () => {
    const attempt = (submission: SignInSubmission): Promise<SignInAttemptOutcome> => {
      submissions.push(submission)
      return Promise.resolve({ kind: 'refused', refusal: 'unreachable' })
    }
    await renderSheet({ mode: 'add', attempt })
    await pickProtocol('SFTP')

    typeInto(
      document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement,
      'test_user_1@testnas.local:2222',
    )
    await tick()
    expect(document.body.querySelector<HTMLInputElement>('#server-username')?.value).toBe('test_user_1')

    buttonSaying('Add and open').click()
    await flush()

    expect(submissions).toHaveLength(1)
    const only = submissions[0]
    if (only.mode !== 'add') throw new Error('expected an add submission')
    expect(only.target).toMatchObject({ protocol: 'sftp', host: 'testnas.local', port: 2222, username: 'test_user_1' })
  })

  it('opens a prefilled URL on the protocol its scheme spells out', async () => {
    await renderSheet({
      mode: 'add',
      prefill: 'https://cloud.example.com',
      attempt: () => Promise.resolve({ kind: 'cancelled' }),
    })
    expect(protocolTab('WebDAV').getAttribute('aria-selected')).toBe('true')
    expect(document.body.querySelector('#server-address-warning')).toBeNull()
  })

  it('hands an SMB address off rather than dialing it, and asks for no password', async () => {
    const attempt = (submission: SignInSubmission): Promise<SignInAttemptOutcome> => {
      submissions.push(submission)
      return Promise.resolve({ kind: 'handed_off' })
    }
    const { done } = await renderSheet({ mode: 'add', attempt })

    typeInto(document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement, 'testnas')
    await tick()
    // ❗ SMB's connect is a share MOUNT: nothing is asked until a listing or a
    // mount refuses, so there is no password field to put in front of anyone.
    expect(document.body.querySelector('#server-secret')).toBeNull()

    buttonSaying('Add and open').click()
    await flush()

    expect(submissions).toEqual([
      { mode: 'add_smb', address: 'smb://testnas', name: '', username: null, intent: 'open' },
    ])
    expect(done).toEqual([{ kind: 'handed_off' }])
  })

  it('refuses a start folder outside the root under the start folder, before dialing', async () => {
    const attempt = (submission: SignInSubmission): Promise<SignInAttemptOutcome> => {
      submissions.push(submission)
      return Promise.resolve({ kind: 'connected', volumeId: 'v' })
    }
    await renderSheet({ mode: 'add', attempt })
    await pickProtocol('SFTP')

    typeInto(
      document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement,
      'test_user_1@testnas.local/srv/data',
    )
    await tick()
    const startFolder = document.body.querySelector<HTMLInputElement>('#server-start-folder') as HTMLInputElement
    typeInto(startFolder, '/srv/data-1')
    await tick()

    buttonSaying('Add and open').click()
    await flush()

    // ❗ By whole components: `/srv/data-1` is a sibling of `/srv/data`, and the
    // sheet says so without a round-trip.
    expect(submissions).toEqual([])
    expect(document.body.querySelector('#server-start-folder-refusal')).not.toBeNull()
    expect(startFolder.getAttribute('aria-invalid')).toBe('true')
    expect(document.activeElement).toBe(startFolder)
  })

  it('puts a connect that refuses the start folder under the start folder', async () => {
    const attempt = (): Promise<SignInAttemptOutcome> =>
      Promise.resolve({ kind: 'refused', refusal: 'start_folder_outside_root' })
    await renderSheet({ mode: 'add', attempt })
    await pickProtocol('SFTP')

    typeInto(
      document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement,
      'test_user_1@testnas.local/srv/data',
    )
    await tick()
    buttonSaying('Add and open').click()
    await flush()

    expect(document.body.querySelector('#server-start-folder-refusal')).not.toBeNull()
    expect(document.body.querySelector('#server-address-refusal')).toBeNull()
    expect(document.activeElement).toBe(document.body.querySelector('#server-start-folder'))
  })
})

describe('SignInSheet: a refusal and the account that earned it', () => {
  it('names the account the refused round tried, not the one the sheet opened with', async () => {
    const attempt = (submission: SignInSubmission): Promise<SignInAttemptOutcome> => {
      submissions.push(submission)
      return Promise.resolve({ kind: 'refused', refusal: 'account_not_permitted' })
    }
    await renderSheet({
      mode: 'sign-in',
      remembered: false,
      endpoint: {
        protocol: 'smb',
        displayName: 'observermch/data',
        address: 'smb://observermch/data',
        host: 'observermch',
        username: 'test_user_1',
      },
      shape: { kind: 'username_password', guestAllowed: false },
      attempt,
    })

    typeInto(document.body.querySelector<HTMLInputElement>('#sign-in-username') as HTMLInputElement, 'test_user_5')
    typeInto(document.body.querySelector<HTMLInputElement>('#sign-in-secret') as HTMLInputElement, 'hunter2')
    await tick()
    buttonSaying('Sign in').click()
    await flush()

    // The share turned away `test_user_5`, the account that was sent. Naming `test_user_1`, who
    // the sheet happened to open with, blames an account nobody tried.
    expect(submissions[0]?.mode === 'sign-in' && submissions[0].username).toBe('test_user_5')
    expect(document.body.textContent).toContain('test_user_5 doesn’t have access here')
    expect(document.body.textContent).not.toContain('test_user_1 doesn’t have access here')
  })
})

describe('SignInSheet: a refusal and the address that earned it', () => {
  it('stops accusing the old host once the address is corrected', async () => {
    const attempt = (submission: SignInSubmission): Promise<SignInAttemptOutcome> => {
      submissions.push(submission)
      return Promise.resolve({ kind: 'refused', refusal: 'unreachable' })
    }
    await renderSheet({ mode: 'add', attempt })
    await pickProtocol('SFTP')

    const address = document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement
    typeInto(address, 'test_user_1@typo.local:22')
    await tick()
    buttonSaying('Add and open').click()
    await flush()

    // The refusal names the host that was actually dialed.
    expect(document.body.textContent).toContain('typo.local')

    // Correcting the address retires it. Pre-fix the sentence survived AND
    // re-interpolated the live field, so a host Cmdr never contacted was on
    // screen being called unreachable.
    typeInto(address, 'test_user_1@testnas.local:22')
    await tick()
    expect(document.body.textContent).not.toContain('testnas.local.')
    expect(document.body.textContent).not.toContain('typo.local')
  })
})

/**
 * ❗ A WebDAV address over plain `http` sends the password readable on every request (#540).
 * The sheet says so before the first secret leaves: when adding, and when signing back in.
 */
describe('SignInSheet: a WebDAV address without encryption', () => {
  const attempt = (): Promise<SignInAttemptOutcome> => Promise.resolve({ kind: 'handed_off' })
  const CLEARTEXT = 'isn’t encrypted'

  it('warns under the address while adding, and not for an https one', async () => {
    await renderSheet({ mode: 'add', attempt })
    await pickProtocol('WebDAV')
    const address = document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement

    typeInto(address, 'http://testnas.local:5005')
    await tick()
    expect(document.body.querySelector('#server-address-warning')?.textContent).toContain(CLEARTEXT)
    expect(address.getAttribute('aria-describedby')).toBe('server-address-warning')

    typeInto(address, 'https://testnas.local:5006')
    await tick()
    expect(document.body.querySelector('#server-address-warning')).toBeNull()

    // No scheme reads as https, so it needs no warning either.
    typeInto(address, 'testnas.local')
    await tick()
    expect(document.body.querySelector('#server-address-warning')).toBeNull()
  })

  it('says nothing about encryption for another protocol', async () => {
    await renderSheet({ mode: 'add', attempt })
    await pickProtocol('SFTP')
    typeInto(document.body.querySelector<HTMLInputElement>('#server-address') as HTMLInputElement, 'testnas.local')
    await tick()
    expect(document.body.textContent).not.toContain(CLEARTEXT)
  })

  it('warns before signing back in to an http server', async () => {
    await renderSheet({
      mode: 'sign-in',
      remembered: false,
      endpoint: { ...endpoint, protocol: 'webdav', address: 'http://testnas.local:5005' },
      shape: { kind: 'password' },
      attempt,
    })
    expect(document.body.textContent).toContain(CLEARTEXT)
  })

  it('stays quiet signing back in to an https server', async () => {
    await renderSheet({
      mode: 'sign-in',
      remembered: false,
      endpoint: { ...endpoint, protocol: 'webdav', address: 'https://testnas.local:5006' },
      shape: { kind: 'password' },
      attempt,
    })
    expect(document.body.textContent).not.toContain(CLEARTEXT)
  })
})
