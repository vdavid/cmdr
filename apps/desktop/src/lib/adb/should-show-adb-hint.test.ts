import { describe, expect, it } from 'vitest'
import { shouldShowAdbHint, type AdbHintInputs } from './should-show-adb-hint'

const base: AdbHintInputs = {
  isMtpPane: true,
  deviceName: 'Pixel 7',
  deviceNames: ['Pixel 7'],
  adbEnabled: true,
  dismissed: false,
}

describe('shouldShowAdbHint', () => {
  it('offers the fuller way in on a phone that is only reachable over MTP', () => {
    expect(shouldShowAdbHint(base)).toBe(true)
  })

  it('says nothing anywhere but an MTP pane', () => {
    expect(shouldShowAdbHint({ ...base, isMtpPane: false })).toBe(false)
  })

  it('stays gone once dismissed', () => {
    expect(shouldShowAdbHint({ ...base, dismissed: true })).toBe(false)
  })

  // ❗ The phone already has an ADB row, so USB debugging is ALREADY on and the
  // line would be telling the user to do something they did. This is the arm
  // that makes the hint honest rather than a nag.
  it('says nothing when the same phone is already listed over ADB', () => {
    expect(shouldShowAdbHint({ ...base, deviceNames: ['Pixel 7', 'Pixel 7'] })).toBe(false)
  })

  it('is not fooled by a different phone being on ADB', () => {
    expect(shouldShowAdbHint({ ...base, deviceNames: ['Pixel 7', 'Galaxy S24'] })).toBe(true)
  })

  // Turning USB debugging on would produce no row at all, so the line would be
  // advice that leads nowhere.
  it('says nothing while Cmdr is not following Android devices at all', () => {
    expect(shouldShowAdbHint({ ...base, adbEnabled: false })).toBe(false)
  })
})
