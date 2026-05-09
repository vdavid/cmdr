/**
 * Tier 3 a11y tests for `FallbackErrorContent.svelte`.
 *
 * Renders variant-derived copy (title + suggestion) for `WriteOperationError`
 * variants when the backend didn't attach a `FriendlyError`. Pinned across
 * the variants the parent dialog's a11y suite covers (permission_denied,
 * insufficient_space, read_only_device, device_disconnected).
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import FallbackErrorContent from './FallbackErrorContent.svelte'
import type { WriteOperationError } from '$lib/file-explorer/types'
import { expectNoA11yViolations } from '$lib/test-a11y'

vi.mock('$lib/tauri-commands', () => ({
  formatBytes: vi.fn((n: number) => `${String(n)} B`),
}))

function mountFallback(error: WriteOperationError, operationType: 'copy' | 'move' | 'delete' | 'trash' = 'copy') {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(FallbackErrorContent, { target, props: { error, operationType } })
  return target
}

describe('FallbackErrorContent a11y', () => {
  it('permission_denied (copy) has no a11y violations', async () => {
    const target = mountFallback(
      { type: 'permission_denied', path: '/Users/test/protected.txt', message: 'EACCES' },
      'copy',
    )
    await tick()
    await expectNoA11yViolations(target)
  })

  it('insufficient_space (move) has no a11y violations', async () => {
    const target = mountFallback(
      {
        type: 'insufficient_space',
        required: 1024 * 1024 * 500,
        available: 1024 * 1024 * 42,
        volumeName: 'External',
      },
      'move',
    )
    await tick()
    await expectNoA11yViolations(target)
  })

  it('read_only_device (delete) has no a11y violations', async () => {
    const target = mountFallback(
      { type: 'read_only_device', path: '/Volumes/ReadOnly/file.txt', deviceName: 'ReadOnly' },
      'delete',
    )
    await tick()
    await expectNoA11yViolations(target)
  })

  it('device_disconnected (trash) has no a11y violations', async () => {
    const target = mountFallback({ type: 'device_disconnected', path: '/Volumes/External/file.txt' }, 'trash')
    await tick()
    await expectNoA11yViolations(target)
  })
})
