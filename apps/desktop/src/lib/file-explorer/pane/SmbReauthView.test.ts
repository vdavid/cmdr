/**
 * Behavior tests for SmbReauthView — the "Sign in" prompt shown when an SMB reconnect
 * gave up because the saved password went stale (`needs-auth`). Submitting must call
 * `reconnectSmbVolumeWithCredentials`; a failure surfaces inline without dead-ending.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, unmount, tick } from 'svelte'
import SmbReauthView from './SmbReauthView.svelte'

const h = vi.hoisted(() => ({
  reconnectSmbVolumeWithCredentials: vi.fn(),
}))

vi.mock('$lib/tauri-commands', () => ({
  reconnectSmbVolumeWithCredentials: h.reconnectSmbVolumeWithCredentials,
  getUsernameHints: vi.fn(() => Promise.resolve({})),
  getKnownShareByName: vi.fn(() => Promise.resolve(null)),
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({ debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() }),
}))

/** Narrows a queried element, failing the test with a readable message when absent. */
function must<T>(value: T | null | undefined, what: string): T {
  expect(value, `expected ${what} to be present`).toBeTruthy()
  return value as T
}

function mountView(onCancel: () => void = vi.fn()) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(SmbReauthView, {
    target,
    props: { volumeId: 'smb-naspolya-445-naspi', serverLabel: 'naspi on Naspolya', onCancel },
  })
  return { target, component }
}

async function fillAndSubmit(target: HTMLElement, username: string, password: string) {
  const usernameInput = must(target.querySelector<HTMLInputElement>('#username'), 'the username input')
  const passwordInput = must(target.querySelector<HTMLInputElement>('#password'), 'the password input')
  usernameInput.value = username
  usernameInput.dispatchEvent(new Event('input', { bubbles: true }))
  passwordInput.value = password
  passwordInput.dispatchEvent(new Event('input', { bubbles: true }))
  await tick()
  must(target.querySelector('form'), 'the login form').dispatchEvent(
    new Event('submit', { bubbles: true, cancelable: true }),
  )
}

describe('SmbReauthView', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    document.body.innerHTML = ''
  })

  it('reconnects with the entered credentials on submit', async () => {
    h.reconnectSmbVolumeWithCredentials.mockResolvedValue(undefined)
    const { target, component } = mountView()
    await vi.waitFor(() => {
      expect(target.querySelector('#username')).toBeTruthy()
    })

    await fillAndSubmit(target, 'david', 'hunter2')

    await vi.waitFor(() => {
      expect(h.reconnectSmbVolumeWithCredentials).toHaveBeenCalledWith('smb-naspolya-445-naspi', 'david', 'hunter2')
    })

    await unmount(component)
  })

  it('shows an inline error (no dead end) when the new password is also wrong', async () => {
    h.reconnectSmbVolumeWithCredentials.mockRejectedValue({ type: 'auth_failed', message: 'bad' })
    const { target, component } = mountView()
    await vi.waitFor(() => {
      expect(target.querySelector('#username')).toBeTruthy()
    })

    await fillAndSubmit(target, 'david', 'wrong')

    await vi.waitFor(() => {
      expect(must(target.querySelector('.error-message'), 'the inline error').textContent).toContain('try again')
    })
    // Form is still there for another attempt.
    expect(target.querySelector('#password')).toBeTruthy()

    await unmount(component)
  })

  it('invokes onCancel when the user cancels', async () => {
    const onCancel = vi.fn()
    const { target, component } = mountView(onCancel)
    await vi.waitFor(() => {
      expect(target.querySelector('#username')).toBeTruthy()
    })

    const cancelButton = must(
      Array.from(target.querySelectorAll('button')).find((b) => b.textContent.includes('Cancel')),
      'the Cancel button',
    )
    cancelButton.click()
    await tick()

    expect(onCancel).toHaveBeenCalledTimes(1)

    await unmount(component)
  })
})
