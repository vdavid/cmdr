/**
 * Component tests for `UpdatesSection.svelte`.
 *
 * Verifies the "Check for updates" button + status text behave correctly across update phases,
 * and that the error case renders a "Send error report" link wired to `openErrorReportDialog`.
 */

import { afterEach, beforeEach, describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import type { BetaSignupResult } from '$lib/tauri-commands'

const { openErrorReportDialogMock, checkForUpdatesMock, betaSignupMock } = vi.hoisted(() => ({
  openErrorReportDialogMock: vi.fn(),
  checkForUpdatesMock: vi.fn(() => Promise.resolve()),
  betaSignupMock: vi.fn((): Promise<BetaSignupResult> => Promise.resolve({ kind: 'subscribed' })),
}))

vi.mock('$lib/error-reporter/error-report-flow.svelte', () => ({
  openErrorReportDialog: openErrorReportDialogMock,
}))

// Spread the real barrel and override only `betaSignup`, so other `$lib/tauri-commands` exports the
// mounted tree might reach stay intact (a barrel mock that drops them silently corrupts the Svelte
// 5 reactive graph; see `lib/ipc/CLAUDE.md` § Test-mock upkeep).
vi.mock('$lib/tauri-commands', async () => {
  const real = await vi.importActual<typeof import('$lib/tauri-commands')>('$lib/tauri-commands')
  return { ...real, betaSignup: betaSignupMock }
})

// Stubs the updater module so this test isolates the section's UI logic. We use the real
// updater's reactive `updateState` so the section's `$derived`s react correctly to mutations.
import { updateState as realUpdateState, _resetUpdaterStateForTest } from '$lib/updates/updater.svelte'

vi.mock('$lib/updates/updater.svelte', async () => {
  const real = await vi.importActual<typeof import('$lib/updates/updater.svelte')>('$lib/updates/updater.svelte')
  return {
    ...real,
    checkForUpdates: checkForUpdatesMock,
  }
})

vi.mock('$lib/settings/settings-store', () => ({
  // The email field reads a string; everything else in this section reads booleans.
  getSetting: vi.fn((id: string) => (id === 'analytics.email' ? '' : true)),
  setSetting: vi.fn(() => Promise.resolve()),
  resetSetting: vi.fn(),
  isModified: vi.fn(() => false),
  onSpecificSettingChange: vi.fn(() => () => {}),
  onSettingChange: vi.fn(() => () => {}),
}))

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn(() => Promise.resolve('1.2.3')) }))
vi.mock('@tauri-apps/plugin-updater', () => ({ check: vi.fn() }))
vi.mock('$lib/settings-store', () => ({
  loadSettings: vi.fn(() => Promise.resolve({ isOnboarded: false })),
  saveSettings: vi.fn(() => Promise.resolve()),
}))
vi.mock('$lib/ui/toast', () => ({
  addToast: vi.fn(),
  dismissToast: vi.fn(),
}))
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ debug: () => {}, info: () => {}, warn: () => {}, error: () => {} }),
}))

import UpdatesSection from './UpdatesSection.svelte'

function render() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(UpdatesSection, { target, props: { searchQuery: '' } })
  return target
}

function getCheckButton(target: HTMLElement): HTMLButtonElement {
  const btn = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Check for updates')
  if (!btn) throw new Error('Check for updates button missing')
  return btn
}

describe('UpdatesSection', () => {
  beforeEach(() => {
    _resetUpdaterStateForTest()
    openErrorReportDialogMock.mockClear()
    checkForUpdatesMock.mockClear()
    betaSignupMock.mockClear()
    betaSignupMock.mockResolvedValue({ kind: 'subscribed' })
  })

  afterEach(() => {
    _resetUpdaterStateForTest()
  })

  it('renders the Check for updates button enabled when status is idle', async () => {
    const target = render()
    await tick()
    const btn = getCheckButton(target)
    expect(btn.disabled).toBe(false)
  })

  it('disables the button while status is not idle', async () => {
    realUpdateState.status = 'checking'
    const target = render()
    await tick()
    expect(getCheckButton(target).disabled).toBe(true)
  })

  it('clicking the button calls checkForUpdates', async () => {
    const target = render()
    await tick()
    getCheckButton(target).click()
    expect(checkForUpdatesMock).toHaveBeenCalledTimes(1)
  })

  it('shows the no-updates message after an idle check', async () => {
    realUpdateState.previousVersion = '1.2.3'
    const target = render()
    await tick()
    expect(target.textContent).toContain('No updates found. Current version: v1.2.3')
  })

  it('shows the downloading status with both versions', async () => {
    realUpdateState.previousVersion = '1.2.3'
    realUpdateState.nextVersion = '1.3.0'
    realUpdateState.status = 'downloading'
    const target = render()
    await tick()
    expect(target.textContent).toContain('Update found, downloading v1.3.0 (current: v1.2.3)…')
  })

  it('renders an error and a Send error report link when error is set, calling openErrorReportDialog with the formatted note', async () => {
    realUpdateState.error = 'something exploded'
    const target = render()
    await tick()
    expect(target.textContent).toContain('Error: something exploded')
    const link = Array.from(target.querySelectorAll('button')).find((b) => b.textContent.trim() === 'Send error report')
    expect(link).toBeTruthy()
    link?.click()
    expect(openErrorReportDialogMock).toHaveBeenCalledWith('Update check failed: something exploded')
  })

  function getEmailInput(target: HTMLElement): HTMLInputElement {
    const input = target.querySelector<HTMLInputElement>('input.email-input')
    if (!input) throw new Error('Email input missing')
    return input
  }

  it('calls betaSignup on commit of a valid email and shows the success note', async () => {
    const target = render()
    await tick()
    const input = getEmailInput(target)
    input.value = 'tester@example.com'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    input.dispatchEvent(new Event('blur', { bubbles: true }))
    await tick()
    await tick()

    expect(betaSignupMock).toHaveBeenCalledWith('tester@example.com')
    expect(target.textContent).toContain('Check your inbox to confirm your email')
  })

  it('does not call betaSignup for an invalid email', async () => {
    const target = render()
    await tick()
    const input = getEmailInput(target)
    input.value = 'not-an-email'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    input.dispatchEvent(new Event('blur', { bubbles: true }))
    await tick()

    expect(betaSignupMock).not.toHaveBeenCalled()
  })

  it('shows a gentle try-again on a soft failure', async () => {
    betaSignupMock.mockResolvedValueOnce({ kind: 'softFailure' })
    const target = render()
    await tick()
    const input = getEmailInput(target)
    input.value = 'tester@example.com'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    input.dispatchEvent(new Event('blur', { bubbles: true }))
    await tick()
    await tick()

    expect(target.textContent).toContain("Sorry, we couldn't sign you up right now")
  })

  it('does not resend when the same address is committed again', async () => {
    const target = render()
    await tick()
    const input = getEmailInput(target)
    input.value = 'tester@example.com'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    input.dispatchEvent(new Event('blur', { bubbles: true }))
    await tick()
    await tick()
    input.dispatchEvent(new Event('blur', { bubbles: true }))
    await tick()

    expect(betaSignupMock).toHaveBeenCalledTimes(1)
  })
})

describe('UpdatesSection card groups', () => {
  beforeEach(() => {
    _resetUpdaterStateForTest()
  })
  afterEach(() => {
    _resetUpdaterStateForTest()
  })

  function renderWithQuery(searchQuery: string): HTMLDivElement {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(UpdatesSection, { target, props: { searchQuery } })
    return target
  }

  function cardLabels(target: HTMLElement): string[] {
    return Array.from(target.querySelectorAll('.section-card-label')).map((el) => el.textContent.trim())
  }

  it('renders both cards with no search', async () => {
    const target = renderWithQuery('')
    await tick()
    expect(cardLabels(target)).toEqual(expect.arrayContaining(['Updates', 'Privacy and data sharing']))
    target.remove()
  })

  it('shows only the Updates card when searching an Updates-only term, leaving no empty cards', async () => {
    // Pre-fix each `SectionCard` drew its frame unconditionally, so a search that
    // matched only an Updates-card row would still paint the Privacy card as an
    // empty box. The fix gates each card on `anyVisible(shouldShow, ...memberIds)`,
    // the SAME predicate the rows use, so an all-filtered-out card hides too.
    const target = renderWithQuery('what')
    await tick()
    const labels = cardLabels(target)
    expect(labels).toContain('Updates')
    expect(labels).not.toContain('Privacy and data sharing')
    // Exactly one card frame stands (the Updates card).
    expect(target.querySelectorAll('.section-card')).toHaveLength(1)
    target.remove()
  })

  it('shows only the Privacy card when searching a privacy-only term', async () => {
    const target = renderWithQuery('analytics')
    await tick()
    const labels = cardLabels(target)
    expect(labels).toContain('Privacy and data sharing')
    expect(labels).not.toContain('Updates')
    expect(target.querySelectorAll('.section-card')).toHaveLength(1)
    target.remove()
  })
})
