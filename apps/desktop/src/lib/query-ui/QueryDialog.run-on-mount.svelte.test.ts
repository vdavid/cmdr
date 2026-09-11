/**
 * A run someone asked for at mount starts once.
 *
 * An MCP `open_search_dialog` with `autoRun` mounts the dialog with `runOnMount` already set.
 * The backend silences every dialog run but the last to register, so a second start racing
 * the first can leave the dialog tracking a run that never reports, stuck on its first phase.
 */

import { describe, expect, it, vi } from 'vitest'
import { mount, unmount, tick } from 'svelte'
import QueryDialog from './QueryDialog.svelte'
import type { QueryStreamSource } from './query-stream'
import { makeQueryDialogConfig } from './test-helpers'

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

vi.mock('$lib/settings', () => ({
  getSetting: vi.fn((key: string) => (key === 'search.autoApply' ? true : undefined)),
  onSpecificSettingChange: vi.fn(() => () => {}),
}))

vi.mock('$lib/icon-cache', async () => {
  const { writable } = await import('svelte/store')
  return { getCachedIcon: () => undefined, getCachedCustomFolderIcon: () => undefined, iconCacheVersion: writable(0) }
})

async function settle(): Promise<void> {
  await tick()
  await new Promise((r) => setTimeout(r, 0))
  await tick()
}

describe('a run asked for at mount', () => {
  it('starts a prefilled streaming run exactly once', async () => {
    const starts: string[] = []
    const streamingSource: QueryStreamSource = {
      start: (runId) => {
        starts.push(runId)
        return Promise.resolve(() => {})
      },
      cancel: () => {},
    }
    const config = makeQueryDialogConfig({ streamingSource })
    config.state.setQuery('file')
    config.state.setRunOnMount(true)

    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(QueryDialog, { target, props: { config } })
    await settle()

    expect(starts).toHaveLength(1)
    void unmount(component)
    target.remove()
  })
})
