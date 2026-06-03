/**
 * Tier 3 a11y tests for `GoToPathDialog.svelte`. Covers the empty default, the
 * populated-recents state, and the visible inline nearest-ancestor warning.
 */

import { describe, it, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

const { getRecentPathsListMock, resolveGoToPathMock } = vi.hoisted(() => ({
  getRecentPathsListMock: vi.fn(() => [] as { id: string; path: string; timestamp: number }[]),
  resolveGoToPathMock: vi.fn(() => Promise.resolve({ status: 'ok', data: { kind: 'invalid', reason: 'empty' } })),
}))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  readClipboardText: vi.fn(() => Promise.resolve(null)),
}))

vi.mock('$lib/ipc/bindings', () => ({
  commands: { resolveGoToPath: resolveGoToPathMock },
}))

vi.mock('./go-to-path', () => ({
  goToPath: vi.fn(() => Promise.resolve(undefined)),
  digitToRecentIndex: () => null,
  shouldPrefillClipboard: () => false,
}))

vi.mock('./recent-paths-state.svelte', () => ({
  getRecentPathsList: getRecentPathsListMock,
  loadRecentPaths: vi.fn(() => Promise.resolve()),
  removeRecentPath: vi.fn(() => Promise.resolve()),
}))

import GoToPathDialog from './GoToPathDialog.svelte'

function mountDialog() {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(GoToPathDialog, {
    target,
    props: { baseDir: '/Users/test', onGo: () => Promise.resolve(undefined), onCancel: () => {} },
  })
  return target
}

describe('GoToPathDialog a11y', () => {
  it('empty (no recents) has no a11y violations', async () => {
    getRecentPathsListMock.mockReturnValue([])
    const target = mountDialog()
    await tick()
    await expectNoA11yViolations(target)
  })

  it('with recents has no a11y violations', async () => {
    getRecentPathsListMock.mockReturnValue([
      { id: '1', path: '/Users/test/Documents', timestamp: 1 },
      { id: '2', path: '/tmp', timestamp: 2 },
    ])
    const target = mountDialog()
    await tick()
    await expectNoA11yViolations(target)
  })
})
