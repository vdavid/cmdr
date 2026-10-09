/**
 * The transfer error dialog answered from the keyboard: R = Retry, A = Copy
 * anyway, C or Enter = Close, each only while its button is on screen. The
 * shared guards are pinned in `../decision-keys.test.ts`.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, unmount } from 'svelte'
import TransferErrorDialog from './TransferErrorDialog.svelte'
import type { WriteOperationError } from '$lib/file-explorer/types'
import { DECISION_KEYS_ARM_MS, ERROR_KEYS } from '../decision-keys'
import { buttonLabel } from '../test-button-label'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  openExternalUrl: vi.fn(() => Promise.resolve()),
  openSystemSettingsUrl: vi.fn(() => Promise.resolve()),
}))

const transient: WriteOperationError = { type: 'connection_interrupted', path: '/p' }
const shortfall: WriteOperationError = {
  type: 'insufficient_space',
  required: 2_000_000_000,
  available: 500_000_000,
  volumeName: 'Backup',
}

const onClose = vi.fn()
const onRetry = vi.fn()
const onCopyAnyway = vi.fn()

let mounted: ReturnType<typeof mount> | null = null

function render(error: WriteOperationError, handlers: { retry?: boolean; copyAnyway?: boolean } = {}): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted = mount(TransferErrorDialog, {
    target,
    props: {
      operationType: 'copy',
      error,
      onClose,
      ...(handlers.retry ? { onRetry } : {}),
      ...(handlers.copyAnyway ? { onCopyAnyway } : {}),
    },
  })
  flushSync()
  return target
}

function arm(): void {
  vi.advanceTimersByTime(DECISION_KEYS_ARM_MS)
  flushSync()
}

function press(target: HTMLElement, init: KeyboardEventInit): void {
  target
    .querySelector('.modal-overlay')
    ?.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }))
  flushSync()
}

function letter(key: string): KeyboardEventInit {
  return { key: key.toLowerCase(), code: `Key${key}` }
}

beforeEach(() => {
  vi.useFakeTimers()
  document.body.innerHTML = ''
  onClose.mockClear()
  onRetry.mockClear()
  onCopyAnyway.mockClear()
})

afterEach(() => {
  if (mounted) void unmount(mounted)
  mounted = null
  vi.useRealTimers()
})

describe('TransferErrorDialog keys', () => {
  it('retries on R, exactly once', () => {
    const target = render(transient, { retry: true })
    arm()
    press(target, letter(ERROR_KEYS.retry))
    expect(onRetry).toHaveBeenCalledTimes(1)
    expect(onClose).not.toHaveBeenCalled()
  })

  it('copies anyway on A after a space shortfall', () => {
    const target = render(shortfall, { copyAnyway: true })
    arm()
    press(target, letter(ERROR_KEYS.copyAnyway))
    expect(onCopyAnyway).toHaveBeenCalledTimes(1)
  })

  it('closes on C and on Enter', () => {
    const target = render(transient, { retry: true })
    arm()
    press(target, letter(ERROR_KEYS.close))
    press(target, { key: 'Enter', code: 'Enter' })
    expect(onClose).toHaveBeenCalledTimes(2)
    expect(onRetry).not.toHaveBeenCalled()
  })

  it('does nothing for a choice the dialog is not offering', () => {
    // A read-only destination offers no Retry, and nothing can start the copy again.
    const target = render({ type: 'read_only_device', path: '/p', deviceName: null, side: 'destination' }, {})
    arm()
    press(target, letter(ERROR_KEYS.retry))
    press(target, letter(ERROR_KEYS.copyAnyway))
    expect(onRetry).not.toHaveBeenCalled()
    expect(onCopyAnyway).not.toHaveBeenCalled()
    expect(onClose).not.toHaveBeenCalled()
  })

  it('ignores keys in the dialog’s first moments, which may belong to whatever was being typed', () => {
    const target = render(transient, { retry: true })
    press(target, letter(ERROR_KEYS.retry))
    press(target, { key: 'Enter', code: 'Enter' })
    expect(onRetry).not.toHaveBeenCalled()
    expect(onClose).not.toHaveBeenCalled()
  })

  it('ignores a held key repeating into the dialog, and a modified one', () => {
    const target = render(transient, { retry: true })
    arm()
    press(target, { key: 'Enter', code: 'Enter', repeat: true })
    press(target, { ...letter(ERROR_KEYS.retry), metaKey: true })
    expect(onClose).not.toHaveBeenCalled()
    expect(onRetry).not.toHaveBeenCalled()
  })

  it('shows each letter on its button and names it for a screen reader', () => {
    const target = render(shortfall, { retry: true, copyAnyway: true })
    const footer = [...target.querySelectorAll('.modal-footer button')]
    expect(
      footer.map((button) => [
        buttonLabel(button),
        button.querySelector('kbd')?.textContent,
        button.getAttribute('aria-keyshortcuts'),
      ]),
    ).toEqual([
      ['Copy anyway', 'A', 'A'],
      ['Close', 'C', 'C Enter'],
    ])
  })
})
