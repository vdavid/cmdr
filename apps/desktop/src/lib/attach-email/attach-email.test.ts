/**
 * The attach-email opt-in shared by the crash-report, error-report, and feedback dialogs.
 *
 * The load-bearing behaviors: the control is offered whether or not a contact email is on
 * file, `emailToAttach` is `undefined` unless the box is ticked, a typed address rides
 * along only once it clears the loose shape check, and `persist()` (called after a
 * successful send) writes the sticky choice plus a newly typed address, so nothing lands
 * in settings from a half-typed field.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest'
import { createAttachEmail } from './attach-email.svelte'
import { getSetting, setSetting } from '$lib/settings'

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn(),
  setSetting: vi.fn(),
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

beforeEach(() => {
  vi.clearAllMocks()
})

describe('createAttachEmail', () => {
  it('reports no contact email on file', () => {
    withSettings('', false)
    const email = createAttachEmail()
    expect(email.hasContactEmail).toBe(false)
    expect(email.contactEmail).toBe('')
  })

  it('treats a whitespace-only email as none on file', () => {
    withSettings('   ', false)
    expect(createAttachEmail().hasContactEmail).toBe(false)
  })

  it('trims the contact email it exposes', () => {
    withSettings('  alex@example.com  ', false)
    const email = createAttachEmail()
    expect(email.hasContactEmail).toBe(true)
    expect(email.contactEmail).toBe('alex@example.com')
  })

  it('seeds the checkbox from the sticky setting', () => {
    withSettings('alex@example.com', true)
    expect(createAttachEmail().attach).toBe(true)
  })

  it('starts unticked with an empty field when nothing is on file', () => {
    withSettings('', false)
    const email = createAttachEmail()
    expect(email.attach).toBe(false)
    expect(email.typedEmail).toBe('')
  })

  it('withholds the email until the box is ticked', () => {
    withSettings('alex@example.com', false)
    const email = createAttachEmail()
    expect(email.emailToAttach).toBeUndefined()
    email.attach = true
    expect(email.emailToAttach).toBe('alex@example.com')
  })

  it('attaches a typed address when none is on file', () => {
    withSettings('', false)
    const email = createAttachEmail()
    email.attach = true
    email.typedEmail = '  alex@example.com  '
    expect(email.emailToAttach).toBe('alex@example.com')
    expect(email.typedEmailInvalid).toBe(false)
    expect(email.blocksSend).toBe(false)
  })

  it('accepts odd-but-valid addresses, matching the loose server-side shape check', () => {
    withSettings('', false)
    for (const address of ['a@b', "o'hara+tag@sub.example.museum", 'アレックス@例え.テスト', '"quoted"@host']) {
      const email = createAttachEmail()
      email.attach = true
      email.typedEmail = address
      expect(email.typedEmailInvalid, address).toBe(false)
      expect(email.emailToAttach, address).toBe(address)
    }
  })

  it('rejects an address with no @, and blocks the send', () => {
    withSettings('', false)
    const email = createAttachEmail()
    email.attach = true
    email.typedEmail = 'foo'
    expect(email.typedEmailInvalid).toBe(true)
    expect(email.blocksSend).toBe(true)
    expect(email.emailToAttach).toBeUndefined()
  })

  it('rejects an address with a space in it', () => {
    withSettings('', false)
    const email = createAttachEmail()
    email.attach = true
    email.typedEmail = 'alex smith@example.com'
    expect(email.typedEmailInvalid).toBe(true)
  })

  it('neither attaches nor blocks when ticked with an empty field', () => {
    withSettings('', false)
    const email = createAttachEmail()
    email.attach = true
    expect(email.emailToAttach).toBeUndefined()
    expect(email.typedEmailInvalid).toBe(false)
    expect(email.blocksSend).toBe(false)
  })

  it('never blocks the send while the box is unticked', () => {
    withSettings('', false)
    const email = createAttachEmail()
    email.typedEmail = 'foo'
    expect(email.typedEmailInvalid).toBe(false)
    expect(email.blocksSend).toBe(false)
  })

  it('prefers the address on file over anything typed', () => {
    withSettings('alex@example.com', false)
    const email = createAttachEmail()
    email.attach = true
    email.typedEmail = 'foo'
    expect(email.emailToAttach).toBe('alex@example.com')
    expect(email.blocksSend).toBe(false)
  })

  it('writes nothing to settings while the user types', () => {
    withSettings('', false)
    const email = createAttachEmail()
    email.attach = true
    email.typedEmail = 'a'
    email.typedEmail = 'alex@example.com'
    expect(mockSetSetting).not.toHaveBeenCalled()
  })

  it('persists the sticky choice', () => {
    withSettings('alex@example.com', false)
    const email = createAttachEmail()
    email.attach = true
    email.persist()
    expect(mockSetSetting).toHaveBeenCalledWith('updates.attachEmailToReports', true)
  })

  it('persists a typed address so the next report reuses it', () => {
    withSettings('', false)
    const email = createAttachEmail()
    email.attach = true
    email.typedEmail = '  alex@example.com '
    email.persist()
    expect(mockSetSetting).toHaveBeenCalledWith('analytics.email', 'alex@example.com')
    expect(mockSetSetting).toHaveBeenCalledWith('updates.attachEmailToReports', true)
  })

  it('persists no address when the typed one never rode along', () => {
    withSettings('', false)
    const email = createAttachEmail()
    email.attach = true
    email.typedEmail = 'foo'
    email.persist()
    expect(mockSetSetting).not.toHaveBeenCalledWith('analytics.email', expect.anything())
  })

  it('persists no address when the box is unticked', () => {
    withSettings('', false)
    const email = createAttachEmail()
    email.typedEmail = 'alex@example.com'
    email.persist()
    expect(mockSetSetting).not.toHaveBeenCalledWith('analytics.email', expect.anything())
    expect(mockSetSetting).toHaveBeenCalledWith('updates.attachEmailToReports', false)
  })

  it('leaves the address on file alone', () => {
    withSettings('alex@example.com', false)
    const email = createAttachEmail()
    email.attach = true
    email.persist()
    expect(mockSetSetting).not.toHaveBeenCalledWith('analytics.email', expect.anything())
  })
})
