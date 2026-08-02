/**
 * The attach-email opt-in shared by the crash-report, error-report, and feedback dialogs.
 *
 * The load-bearing behaviors: the checkbox is offered only when a contact email is on
 * file, `emailToAttach` is `undefined` unless the box is ticked, and `persist()` writes
 * the sticky choice back only when an email exists (so a user with no email on file
 * never has the setting flipped underneath them).
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
  it('is unavailable when no contact email is on file', () => {
    withSettings('', false)
    const email = createAttachEmail()
    expect(email.available).toBe(false)
    expect(email.contactEmail).toBe('')
  })

  it('treats a whitespace-only email as none on file', () => {
    withSettings('   ', false)
    expect(createAttachEmail().available).toBe(false)
  })

  it('trims the contact email it exposes', () => {
    withSettings('  alex@example.com  ', false)
    const email = createAttachEmail()
    expect(email.available).toBe(true)
    expect(email.contactEmail).toBe('alex@example.com')
  })

  it('seeds the checkbox from the sticky setting', () => {
    withSettings('alex@example.com', true)
    expect(createAttachEmail().attach).toBe(true)
  })

  it('withholds the email until the box is ticked', () => {
    withSettings('alex@example.com', false)
    const email = createAttachEmail()
    expect(email.emailToAttach).toBeUndefined()
    email.attach = true
    expect(email.emailToAttach).toBe('alex@example.com')
  })

  it('withholds the email when ticked but none is on file', () => {
    withSettings('', false)
    const email = createAttachEmail()
    email.attach = true
    expect(email.emailToAttach).toBeUndefined()
  })

  it('persists the sticky choice when an email is on file', () => {
    withSettings('alex@example.com', false)
    const email = createAttachEmail()
    email.attach = true
    email.persist()
    expect(mockSetSetting).toHaveBeenCalledWith('updates.attachEmailToReports', true)
  })

  it('persists nothing when no email is on file', () => {
    withSettings('', true)
    createAttachEmail().persist()
    expect(mockSetSetting).not.toHaveBeenCalled()
  })
})
