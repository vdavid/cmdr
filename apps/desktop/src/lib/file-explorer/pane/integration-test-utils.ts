/**
 * Shared helpers for FilePane / VolumeBreadcrumb / selection integration tests.
 *
 * NOTE: vi.mock() calls must remain in each test file — Vitest hoists them and
 * they don't work when imported from a shared module. Only non-mock helpers
 * (waitForUpdates, useMountTarget) are shared here.
 */
import { vi, beforeEach, afterEach } from 'vitest'
import { tick } from 'svelte'

// Helper to wait for async updates
export async function waitForUpdates(ms = 50): Promise<void> {
  await tick()
  await new Promise((r) => setTimeout(r, ms))
  await tick()
}

// Mock scrollIntoView which isn't available in jsdom
Element.prototype.scrollIntoView = vi.fn()

/** Standard beforeEach/afterEach for mounting tests with a target div. */
export function useMountTarget(): { getTarget: () => HTMLDivElement } {
  let target: HTMLDivElement

  beforeEach(() => {
    vi.clearAllMocks()
    target = document.createElement('div')
    document.body.appendChild(target)
  })

  afterEach(() => {
    target.remove()
  })

  return {
    getTarget: () => target,
  }
}
