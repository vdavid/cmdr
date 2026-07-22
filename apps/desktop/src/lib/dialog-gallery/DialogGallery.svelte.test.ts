/**
 * The harness's render decision: it shows the requested fixture, and shows
 * NOTHING (with a warning) when a request doesn't resolve to one. Half-filled
 * dialogs are the one thing a design-review instrument must never produce.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount, tick, unmount } from 'svelte'
import DialogGallery from './DialogGallery.svelte'
import { closeGalleryDialog, openGalleryDialog } from './gallery-state.svelte'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

const warn = vi.fn()
vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({
    warn: (...args: unknown[]) => {
      warn(...args)
    },
    info: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
  }),
}))

// The store is module-level, so a harness left mounted from an earlier test would
// react to the next test's store writes (and re-warn). Every mount is torn down.
let mounted: Record<string, unknown> | undefined

function mountGallery(): HTMLElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mounted = mount(DialogGallery, { target })
  return target
}

beforeEach(() => {
  warn.mockClear()
})

afterEach(() => {
  if (mounted) void unmount(mounted)
  mounted = undefined
  closeGalleryDialog()
})

describe('DialogGallery', () => {
  it('renders nothing while no preview is open', async () => {
    const target = mountGallery()
    await tick()
    expect(target.querySelector('[role="alertdialog"]')).toBeNull()
    expect(warn).not.toHaveBeenCalled()
  })

  it('renders the requested alert fixture', async () => {
    openGalleryDialog('alert', 'short')
    const target = mountGallery()
    await tick()
    const dialog = target.querySelector('[role="alertdialog"]')
    expect(dialog).not.toBeNull()
    expect(dialog?.textContent).toContain('Nothing to copy')
    expect(warn).not.toHaveBeenCalled()
  })

  it('swaps to another state without leaving the previous one mounted', async () => {
    openGalleryDialog('alert', 'short')
    const target = mountGallery()
    await tick()
    openGalleryDialog('alert', 'custom-button')
    await tick()
    expect(target.querySelectorAll('[role="alertdialog"]')).toHaveLength(1)
    expect(target.textContent).toContain('Indexing paused')
    expect(target.textContent).not.toContain('Nothing to copy')
  })

  it('renders nothing and warns when the state id has no fixture', async () => {
    openGalleryDialog('alert', 'no-such-state')
    const target = mountGallery()
    await tick()
    expect(target.querySelector('[role="alertdialog"]')).toBeNull()
    expect(warn).toHaveBeenCalledTimes(1)
  })

  it('renders nothing and warns for a dialog the harness has no case for', async () => {
    openGalleryDialog('whats-new', 'default')
    const target = mountGallery()
    await tick()
    expect(target.querySelector('[role="alertdialog"]')).toBeNull()
    expect(warn).toHaveBeenCalledTimes(1)
  })

  it('closes back to rendering nothing', async () => {
    openGalleryDialog('alert', 'short')
    const target = mountGallery()
    await tick()
    closeGalleryDialog()
    await tick()
    expect(target.querySelector('[role="alertdialog"]')).toBeNull()
  })
})
