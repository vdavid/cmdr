/**
 * Streaming behavior tests for NewFolderDialog.
 *
 * Mocks `streamFolderSuggestions` so we can fire each event manually and assert the
 * DOM updates incrementally. Covers the contract:
 *  - `suggestion` events render new chips immediately.
 *  - The trailing pulsing chip is present while streaming, gone after `done`.
 *  - `cancelled` and `failed` end streaming the same way as `done` (visually).
 *  - Dialog unmount cancels in-flight streams.
 *  - On Cloud without "Allow cloud AI", no stream opens and no suggestion strip shows.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
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
  // Local by default, so the streaming cases below never meet the cloud gate.
  const ai: { provider: string; cloudBlocked: boolean } = { provider: 'local', cloudBlocked: false }
  return { state, ai }
})

vi.mock('$lib/settings', async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>()
  const getSetting = actual.getSetting as (id: string) => unknown
  return {
    ...actual,
    getSetting: (id: string): unknown => (id === 'ai.provider' ? hoisted.ai.provider : getSetting(id)),
  }
})
vi.mock('$lib/ai/cloud-consent.svelte', () => ({
  refreshCloudConsent: vi.fn(() => Promise.resolve()),
  cloudAiBlocked: (provider: string) => provider === 'cloud' && hoisted.ai.cloudBlocked,
}))

vi.mock('$lib/tauri-commands', () => ({
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
  createDirectory: vi.fn(() => Promise.resolve()),
  findFileIndex: vi.fn(() => Promise.resolve(null)),
  getAiStatus: vi.fn(() => Promise.resolve('available')),
  getFileAt: vi.fn(() => Promise.resolve(null)),
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
      // Resolve the command promise immediately. The dialog `await`s it but we
      // don't model the "command still pending" state in these tests; events are
      // delivered via the channel, not the promise.
      return { promise: Promise.resolve(), cancel }
    },
  ),
  onDirectoryDiff: vi.fn(() => Promise.resolve(() => {})),
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
  return Array.from(target.querySelectorAll('button.suggestion-item')).map((el) => el.textContent.trim())
}

function pulsingChipPresent(target: HTMLElement): boolean {
  return target.querySelector('.suggestion-pending') !== null
}

beforeEach(() => {
  hoisted.ai.provider = 'local'
  hoisted.ai.cloudBlocked = false
})

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

    void unmount(component)
    await tick()

    expect(stream.cancel).toHaveBeenCalled()
  })
})

describe('NewFolderDialog without cloud AI consent', () => {
  it('sends nothing and shows no suggestions on Cloud until the user allows cloud AI', async () => {
    const { streamFolderSuggestions } = await import('$lib/tauri-commands')
    vi.mocked(streamFolderSuggestions).mockClear()
    hoisted.ai.provider = 'cloud'
    hoisted.ai.cloudBlocked = true
    const { target } = mountDialog()
    for (let i = 0; i < 10; i++) await tick()
    await new Promise((r) => setTimeout(r, 0))

    expect(streamFolderSuggestions).not.toHaveBeenCalled()
    expect(target.querySelector('.suggestion-pending')).toBeNull()
    expect(chipTexts(target)).toEqual([])
    target.remove()
  })

  it('streams as usual on Cloud once cloud AI is allowed', async () => {
    hoisted.ai.provider = 'cloud'
    const { target } = mountDialog()
    await waitForActiveStream()
    target.remove()
  })
})
