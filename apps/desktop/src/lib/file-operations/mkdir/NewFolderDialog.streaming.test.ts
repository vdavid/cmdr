/**
 * Streaming behavior tests for NewFolderDialog.
 *
 * Mocks `streamFolderSuggestions` so we can fire each event manually and assert the
 * DOM updates incrementally. Covers the contract:
 *  - `suggestion` events render new chips immediately.
 *  - The trailing pulsing chip is present while streaming, gone after `done`.
 *  - `cancelled` and `failed` end streaming the same way as `done` (visually).
 *  - Dialog unmount cancels in-flight streams.
 */

import { describe, expect, it, vi } from 'vitest'
import { mount, unmount, tick } from 'svelte'
import NewFolderDialog from './NewFolderDialog.svelte'

type StreamEvent = { type: 'suggestion'; name: string } | { type: 'done' } | { type: 'cancelled' } | { type: 'failed' }

interface FakeStream {
  send: (event: StreamEvent) => Promise<void>
  cancel: ReturnType<typeof vi.fn>
}

// `vi.hoisted` runs before `vi.mock` so the factory can reference these symbols.
const hoisted = vi.hoisted(() => {
  const state: { active: FakeStream | undefined } = { active: undefined }
  return { state }
})

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  createDirectory: vi.fn(() => Promise.resolve()),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  getAiStatus: vi.fn(() => Promise.resolve('available')),
  getFileAt: vi.fn(() => Promise.resolve(null)),
  getFolderSuggestions: vi.fn(() => Promise.resolve([])),
  streamFolderSuggestions: vi.fn(
    (_listingId: string, _currentPath: string, _includeHidden: boolean, onEvent: (e: StreamEvent) => void) => {
      const cancel = vi.fn(() => Promise.resolve())
      hoisted.state.active = {
        send: async (event) => {
          onEvent(event)
          await tick()
        },
        cancel,
      }
      // Resolve the command promise immediately — the dialog `await`s it but we
      // don't model the "command still pending" state in these tests; events are
      // delivered via the channel, not the promise.
      return { promise: Promise.resolve(), cancel }
    },
  ),
  isIpcError: vi.fn(() => false),
  listen: vi.fn(() => Promise.resolve(() => {})),
  refreshListing: vi.fn(() => Promise.resolve()),
}))

function mountDialog() {
  hoisted.state.active = undefined
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(NewFolderDialog, {
    target,
    props: {
      currentPath: '/Users/test/Projects',
      listingId: 'listing-1',
      showHiddenFiles: false,
      initialName: '',
      volumeId: 'root',
      onCreated: () => {},
      onCancel: () => {},
    },
  })
  return { target, component }
}

async function waitForActiveStream(): Promise<FakeStream> {
  // The dialog calls getAiStatus + opens the stream from onMount; wait a few ticks.
  for (let i = 0; i < 10; i++) {
    if (hoisted.state.active) return hoisted.state.active
    await tick()
    await new Promise((r) => setTimeout(r, 0))
  }
  throw new Error('stream did not open in time')
}

function chipTexts(target: HTMLElement): string[] {
  return Array.from(target.querySelectorAll('button.suggestion-item')).map((el) => (el.textContent ?? '').trim())
}

function pulsingChipPresent(target: HTMLElement): boolean {
  return target.querySelector('.suggestion-pending') !== null
}

describe('NewFolderDialog streaming', () => {
  it('renders suggestions incrementally as they stream in, hides pulse on done', async () => {
    const { target } = mountDialog()
    const stream = await waitForActiveStream()

    // While streaming with no suggestions yet, the pulsing chip is shown.
    expect(pulsingChipPresent(target)).toBe(true)
    expect(chipTexts(target)).toEqual([])

    await stream.send({ type: 'suggestion', name: 'docs' })
    expect(chipTexts(target)).toEqual(['docs'])
    expect(pulsingChipPresent(target)).toBe(true)

    await stream.send({ type: 'suggestion', name: 'tests' })
    expect(chipTexts(target)).toEqual(['docs', 'tests'])

    await stream.send({ type: 'done' })
    expect(chipTexts(target)).toEqual(['docs', 'tests'])
    expect(pulsingChipPresent(target)).toBe(false)
  })

  it('keeps already-streamed suggestions visible after `failed`, no error toast', async () => {
    const { target } = mountDialog()
    const stream = await waitForActiveStream()

    await stream.send({ type: 'suggestion', name: 'docs' })
    await stream.send({ type: 'suggestion', name: 'tests' })
    await stream.send({ type: 'failed' })

    expect(chipTexts(target)).toEqual(['docs', 'tests'])
    expect(pulsingChipPresent(target)).toBe(false)
    // No alert/error region for AI failure (graceful degradation).
    expect(target.querySelector('[role="alert"][data-ai-error]')).toBeNull()
  })

  it('treats `cancelled` like `done` visually', async () => {
    const { target } = mountDialog()
    const stream = await waitForActiveStream()

    await stream.send({ type: 'suggestion', name: 'docs' })
    await stream.send({ type: 'cancelled' })

    expect(chipTexts(target)).toEqual(['docs'])
    expect(pulsingChipPresent(target)).toBe(false)
  })

  it('hides the suggestion section when stream emits zero suggestions before `done`', async () => {
    const { target } = mountDialog()
    const stream = await waitForActiveStream()

    await stream.send({ type: 'done' })

    expect(chipTexts(target)).toEqual([])
    expect(pulsingChipPresent(target)).toBe(false)
  })

  it('cancels the stream on dialog unmount', async () => {
    const { component } = mountDialog()
    const stream = await waitForActiveStream()

    unmount(component)
    await tick()

    expect(stream.cancel).toHaveBeenCalled()
  })
})
