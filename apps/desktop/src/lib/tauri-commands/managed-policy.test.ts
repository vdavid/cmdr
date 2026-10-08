/**
 * Tests for the managed policy (MDM) wrappers: the command answer and the change event both hand
 * back the plain `ManagedPolicyView`.
 */

import { describe, it, expect, vi } from 'vitest'
import type { ManagedPolicyView } from '$lib/ipc/bindings'

const ipc = vi.hoisted(() => ({
  listener: null as ((event: { payload: unknown }) => void) | null,
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: { getManagedPolicy: vi.fn() },
  events: {
    managedPolicyChanged: {
      listen: vi.fn((listener: (event: { payload: unknown }) => void) => {
        ipc.listener = listener
        return Promise.resolve(() => {})
      }),
    },
  },
}))

import { commands } from '$lib/ipc/bindings'
import { getManagedPolicy, onManagedPolicyChanged } from './managed-policy'

const view: ManagedPolicyView = {
  managed: true,
  usageStatsDisabled: true,
  reportsDisabled: false,
  updates: { kind: 'disabled' },
  ai: { mode: 'off', allowedCloudHosts: null },
  lockedSettings: [{ id: 'analytics.enabled', lock: { kind: 'fixed', value: false } }],
}

describe('managed policy wrappers', () => {
  it('returns the backend’s view as is', async () => {
    vi.mocked(commands.getManagedPolicy).mockResolvedValueOnce(view)
    await expect(getManagedPolicy()).resolves.toEqual(view)
  })

  it('unwraps the change event to the new view', async () => {
    const handler = vi.fn()
    await onManagedPolicyChanged(handler)
    ipc.listener?.({ payload: { policy: view } })
    expect(handler).toHaveBeenCalledWith(view)
  })
})
