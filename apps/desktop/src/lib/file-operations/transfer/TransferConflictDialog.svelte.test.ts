/**
 * What the conflict body says about WHICH file it is asking about.
 *
 * The a11y sibling audits the same component's structure; this one is about the
 * question being answerable: a bare filename isn't, once two folders hold that
 * name.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'
import { mount, tick } from 'svelte'
import TransferConflictDialog from './TransferConflictDialog.svelte'
import type { WriteConflictEvent } from '$lib/tauri-commands'

vi.mock('$lib/settings/reactive-settings.svelte', () => ({
  formatFileSize: vi.fn((n: number) => `${String(n)} B`),
  getFileSizeFormat: vi.fn(() => 'binary'),
  getFileSizeUnit: vi.fn(() => 'bytes'),
}))

function conflict(destinationPath: string): WriteConflictEvent {
  return {
    operationId: 'op-1',
    conflictId: 1,
    sourcePath: '/Users/test/set-0417/f001',
    destinationPath,
    sourceSize: 2048,
    destinationSize: 1024,
    sourceModified: 1_700_000_000,
    destinationModified: 1_699_000_000,
    destinationIsNewer: false,
    sizeDifference: -1024,
    sourceIsDirectory: false,
    destinationIsDirectory: false,
  }
}

async function mountDialog(conflictEvent: WriteConflictEvent): Promise<HTMLElement> {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(TransferConflictDialog, {
    target,
    props: {
      conflictEvent,
      isCopy: true,
      isMove: false,
      rollbackUnavailable: false,
      isCancelling: false,
      isResolvingConflict: false,
      onResolve: () => {},
      onCancel: () => {},
    },
  })
  await tick()
  return target
}

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('the file the prompt is about', () => {
  it('names the folder under the filename, so two clashes over one name differ on screen', async () => {
    // A QA pass over 1,600 folders that each held an `f001` got 1,600 prompts
    // that read identically. The folder is the only thing that differs.
    const first = await mountDialog(conflict('/Volumes/Backup/2026/set-0417/f001'))
    expect(first.querySelector('.conflict-filename')?.textContent.trim()).toBe('f001')
    expect(first.querySelector('.conflict-folder')?.textContent).toBe('/Volumes/Backup/2026/set-0417')

    const second = await mountDialog(conflict('/Volumes/Backup/2026/set-0418/f001'))
    expect(second.querySelector('.conflict-folder')?.textContent).toBe('/Volumes/Backup/2026/set-0418')
  })

  it('shows the name alone when there is no folder to be sure of', async () => {
    // A relative path is a bug somewhere upstream, and a dialog asking about
    // the user's files is the wrong place to guess at one.
    const target = await mountDialog(conflict('f001'))
    expect(target.querySelector('.conflict-filename')?.textContent.trim()).toBe('f001')
    expect(target.querySelector('.conflict-folder')).toBeNull()
  })
})
