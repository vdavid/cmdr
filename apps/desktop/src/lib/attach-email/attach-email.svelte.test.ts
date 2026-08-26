/**
 * The attach-email opt-in shared by the crash-report, error-report, and feedback dialogs.
 *
 * The load-bearing behaviors: the control is offered whether or not a contact email is on
 * file, it FOLLOWS `analytics.email` live (the label's "change" link opens Settings in a
 * window of its own, so the address can move while the dialog stays up), `emailToAttach`
 * is `undefined` unless the box is ticked, a typed address rides along only once it clears
 * the loose shape check, and `persist()` (called after a successful send) writes the
 * sticky choice plus a newly typed address, so nothing lands in settings from a half-typed
 * field.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest'
import { flushSync } from 'svelte'
import { createAttachEmail, type AttachEmail } from './attach-email.svelte'
import { getSetting, setSetting } from '$lib/settings'

/** Live listeners on `analytics.email`, so a test can play the Settings window's part. */
const emailListeners = new Set<(id: string, value: string) => void>()

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(),
  setSetting: vi.fn(),
  onSpecificSettingChange: vi.fn((id: string, listener: (id: string, value: string) => void) => {
    if (id !== 'analytics.email') throw new Error(`unexpected subscription: ${id}`)
    emailListeners.add(listener)
    return () => emailListeners.delete(listener)
  }),
}))

const mockGetSetting = vi.mocked(getSetting)
const mockSetSetting = vi.mocked(setSetting)

/** Point the mocked store at a contact email and a sticky attach-by-default choice. */
function withSettings(contactEmail: string, attachByDefault: boolean) {
  mockGetSetting.mockImplementation((id) => {
    if (id === 'analytics.email') return contactEmail
    if (id === 'updates.attachEmailToReports') return attachByDefault
    throw new Error(`unexpected setting read: ${id}`)
  })
}

/** The dialogs that own the states, stood in for by effect roots the test can close. */
const owners: (() => void)[] = []

/**
 * Build the state the way a dialog does: inside an owner that runs its `$effect`, so the
 * live subscription is really installed and closing the owner really tears it down.
 */
function create(): AttachEmail {
  let email: AttachEmail | undefined
  owners.push(
    $effect.root(() => {
      email = createAttachEmail()
    }),
  )
  flushSync()
  if (!email) throw new Error('state was not created')
  return email
}

/** Close every owner, the way closing the dialogs would. */
function closeOwners() {
  for (const dispose of owners.splice(0)) dispose()
}

/** Play the Settings window: write `analytics.email` and tell the subscribers. */
function setContactEmailFromSettings(value: string) {
  for (const listener of [...emailListeners]) listener('analytics.email', value)
  flushSync()
}

beforeEach(() => {
  vi.clearAllMocks()
  emailListeners.clear()
})

afterEach(() => {
  closeOwners()
})

describe('createAttachEmail', () => {
  it('reports no contact email on file', () => {
    withSettings('', false)
    const email = create()
    expect(email.hasContactEmail).toBe(false)
    expect(email.contactEmail).toBe('')
  })

  it('treats a whitespace-only email as none on file', () => {
    withSettings('   ', false)
    expect(create().hasContactEmail).toBe(false)
  })

  it('trims the contact email it exposes', () => {
    withSettings('  alex@example.com  ', false)
    const email = create()
    expect(email.hasContactEmail).toBe(true)
    expect(email.contactEmail).toBe('alex@example.com')
  })

  it('seeds the checkbox from the sticky setting', () => {
    withSettings('alex@example.com', true)
    expect(create().attach).toBe(true)
  })

  it('starts unticked with an empty field when nothing is on file', () => {
    withSettings('', false)
    const email = create()
    expect(email.attach).toBe(false)
    expect(email.typedEmail).toBe('')
  })

  it('withholds the email until the box is ticked', () => {
    withSettings('alex@example.com', false)
    const email = create()
    expect(email.emailToAttach).toBeUndefined()
    email.attach = true
    expect(email.emailToAttach).toBe('alex@example.com')
  })

  it('attaches a typed address when none is on file', () => {
    withSettings('', false)
    const email = create()
    email.attach = true
    email.typedEmail = '  alex@example.com  '
    expect(email.emailToAttach).toBe('alex@example.com')
    expect(email.typedEmailInvalid).toBe(false)
    expect(email.blocksSend).toBe(false)
  })

  it('accepts odd-but-valid addresses, matching the loose server-side shape check', () => {
    withSettings('', false)
    for (const address of ['a@b', "o'hara+tag@sub.example.museum", 'アレックス@例え.テスト', '"quoted"@host']) {
      const email = create()
      email.attach = true
      email.typedEmail = address
      expect(email.typedEmailInvalid, address).toBe(false)
      expect(email.emailToAttach, address).toBe(address)
    }
  })

  it('rejects an address with no @, and blocks the send', () => {
    withSettings('', false)
    const email = create()
    email.attach = true
    email.typedEmail = 'foo'
    expect(email.typedEmailInvalid).toBe(true)
    expect(email.blocksSend).toBe(true)
    expect(email.emailToAttach).toBeUndefined()
  })

  it('rejects an address with a space in it', () => {
    withSettings('', false)
    const email = create()
    email.attach = true
    email.typedEmail = 'alex smith@example.com'
    expect(email.typedEmailInvalid).toBe(true)
  })

  it('neither attaches nor blocks when ticked with an empty field', () => {
    withSettings('', false)
    const email = create()
    email.attach = true
    expect(email.emailToAttach).toBeUndefined()
    expect(email.typedEmailInvalid).toBe(false)
    expect(email.blocksSend).toBe(false)
  })

  it('never blocks the send while the box is unticked', () => {
    withSettings('', false)
    const email = create()
    email.typedEmail = 'foo'
    expect(email.typedEmailInvalid).toBe(false)
    expect(email.blocksSend).toBe(false)
  })

  it('prefers the address on file over anything typed', () => {
    withSettings('alex@example.com', false)
    const email = create()
    email.attach = true
    email.typedEmail = 'foo'
    expect(email.emailToAttach).toBe('alex@example.com')
    expect(email.blocksSend).toBe(false)
  })

  it('writes nothing to settings while the user types', () => {
    withSettings('', false)
    const email = create()
    email.attach = true
    email.typedEmail = 'a'
    email.typedEmail = 'alex@example.com'
    expect(mockSetSetting).not.toHaveBeenCalled()
  })

  it('persists the sticky choice', () => {
    withSettings('alex@example.com', false)
    const email = create()
    email.attach = true
    email.persist()
    expect(mockSetSetting).toHaveBeenCalledWith('updates.attachEmailToReports', true)
  })

  it('persists a typed address so the next report reuses it', () => {
    withSettings('', false)
    const email = create()
    email.attach = true
    email.typedEmail = '  alex@example.com '
    email.persist()
    expect(mockSetSetting).toHaveBeenCalledWith('analytics.email', 'alex@example.com')
    expect(mockSetSetting).toHaveBeenCalledWith('updates.attachEmailToReports', true)
  })

  it('persists no address when the typed one never rode along', () => {
    withSettings('', false)
    const email = create()
    email.attach = true
    email.typedEmail = 'foo'
    email.persist()
    expect(mockSetSetting).not.toHaveBeenCalledWith('analytics.email', expect.anything())
  })

  it('persists no address when the box is unticked', () => {
    withSettings('', false)
    const email = create()
    email.typedEmail = 'alex@example.com'
    email.persist()
    expect(mockSetSetting).not.toHaveBeenCalledWith('analytics.email', expect.anything())
    expect(mockSetSetting).toHaveBeenCalledWith('updates.attachEmailToReports', false)
  })

  it('leaves the address on file alone', () => {
    withSettings('alex@example.com', false)
    const email = create()
    email.attach = true
    email.persist()
    expect(mockSetSetting).not.toHaveBeenCalledWith('analytics.email', expect.anything())
  })
})

describe('createAttachEmail following analytics.email live', () => {
  it('switches to the on-file shape when Settings gains an address mid-dialog', () => {
    withSettings('', false)
    const email = create()
    expect(email.hasContactEmail).toBe(false)

    setContactEmailFromSettings('alex@example.com')

    expect(email.hasContactEmail).toBe(true)
    expect(email.contactEmail).toBe('alex@example.com')
  })

  it('switches back to collecting when the address on file is cleared mid-dialog', () => {
    withSettings('alex@example.com', false)
    const email = create()

    setContactEmailFromSettings('')

    expect(email.hasContactEmail).toBe(false)
    expect(email.contactEmail).toBe('')
  })

  it('treats an address cleared to whitespace as none on file', () => {
    withSettings('alex@example.com', false)
    const email = create()
    setContactEmailFromSettings('   ')
    expect(email.hasContactEmail).toBe(false)
  })

  it('attaches the new address, not the old one, after the user edits it in Settings', () => {
    withSettings('old@example.com', false)
    const email = create()
    email.attach = true
    expect(email.emailToAttach).toBe('old@example.com')

    setContactEmailFromSettings('new@example.com')

    // The tick survives: it means "I want a reply", and the label names the address it
    // will carry, so it can't quietly come to mean something the user can't see.
    expect(email.attach).toBe(true)
    expect(email.emailToAttach).toBe('new@example.com')
  })

  it('attaches nothing when the address is cleared out from under a ticked box', () => {
    withSettings('alex@example.com', false)
    const email = create()
    email.attach = true

    setContactEmailFromSettings('')

    expect(email.attach).toBe(true)
    expect(email.emailToAttach).toBeUndefined()
    expect(email.blocksSend).toBe(false)
  })

  it('keeps the typed draft while an address on file hides the field', () => {
    withSettings('', false)
    const email = create()
    email.attach = true
    email.typedEmail = 'typed@example.com'

    setContactEmailFromSettings('onfile@example.com')
    expect(email.emailToAttach).toBe('onfile@example.com')

    setContactEmailFromSettings('')
    // The field comes back showing what the user typed, so nothing rides along unseen.
    expect(email.typedEmail).toBe('typed@example.com')
    expect(email.emailToAttach).toBe('typed@example.com')
  })

  it('stops flagging a typed address once one is on file', () => {
    withSettings('', false)
    const email = create()
    email.attach = true
    email.typedEmail = 'foo'
    expect(email.blocksSend).toBe(true)

    setContactEmailFromSettings('alex@example.com')

    expect(email.typedEmailInvalid).toBe(false)
    expect(email.blocksSend).toBe(false)
  })

  it('persists a typed address after a live switch back to collecting', () => {
    withSettings('alex@example.com', false)
    const email = create()
    email.attach = true
    setContactEmailFromSettings('')
    email.typedEmail = 'typed@example.com'

    email.persist()

    expect(mockSetSetting).toHaveBeenCalledWith('analytics.email', 'typed@example.com')
  })

  it('leaves a live-arrived address on file alone on persist', () => {
    withSettings('', false)
    const email = create()
    email.attach = true
    email.typedEmail = 'typed@example.com'
    setContactEmailFromSettings('onfile@example.com')

    email.persist()

    expect(mockSetSetting).not.toHaveBeenCalledWith('analytics.email', expect.anything())
  })

  it('stops listening once the dialog that owns it goes away', () => {
    withSettings('', false)
    const email = create()
    expect(emailListeners.size).toBe(1)

    closeOwners()

    expect(emailListeners.size).toBe(0)
    expect(email.hasContactEmail).toBe(false)
  })
})
