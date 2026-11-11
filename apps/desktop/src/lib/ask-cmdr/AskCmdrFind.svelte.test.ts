/**
 * The rail's in-chat find as the person drives it: ⌘F inside the rail opens it (and is
 * claimed, so Search files never runs) while ⌘F elsewhere is left alone; typing counts the
 * hits; ⌘G / ⇧⌘G and Enter / ⇧Enter step and wrap; Escape closes and hands focus back; a
 * streaming re-render keeps the count live.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushSync, mount, tick, unmount } from 'svelte'

// The registry's ⌘ means Command on a Mac; pin the platform before the registry loads.
vi.hoisted(() => vi.stubGlobal('__CMDR_PLATFORM__', 'macos'))

import AskCmdrFind from './AskCmdrFind.svelte'
import { FIND_TYPING_DEBOUNCE_MS } from './ask-cmdr-find.svelte'

let rail: HTMLElement
let thread: HTMLElement
let composer: HTMLTextAreaElement
let component: ReturnType<typeof mount> | null = null

function setup(threadHtml: string, partial = false): void {
  rail = document.createElement('aside')
  rail.tabIndex = -1
  const barSlot = document.createElement('div')
  thread = document.createElement('div')
  // Test fixture markup, written here; no message text reaches it.
  thread.innerHTML = threadHtml
  composer = document.createElement('textarea')
  rail.append(barSlot, thread, composer)
  document.body.appendChild(rail)
  component = mount(AskCmdrFind, { target: barSlot, props: { rail, thread, partial } })
  flushSync()
}

function press(target: EventTarget, key: string, mods: Partial<KeyboardEventInit> = {}): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...mods })
  target.dispatchEvent(event)
  return event
}

function field(): HTMLInputElement | null {
  return rail.querySelector<HTMLInputElement>('[role="search"] input')
}

/** The field, which the test expects to be there. */
function openField(): HTMLInputElement {
  const input = field()
  if (!input) throw new Error('find field missing')
  return input
}

function counter(): string {
  return rail.querySelector('.match-count')?.textContent.trim() ?? ''
}

async function type(text: string): Promise<void> {
  const input = openField()
  input.value = text
  input.dispatchEvent(new Event('input', { bubbles: true }))
  vi.advanceTimersByTime(FIND_TYPING_DEBOUNCE_MS + 1)
  flushSync()
  await tick()
}

async function openFind(from: Element = composer): Promise<KeyboardEvent> {
  const event = press(from, 'f', { metaKey: true })
  flushSync()
  await tick()
  await tick()
  return event
}

const THREAD = `
  <div data-find-region>Find my report please</div>
  <div data-find-region><strong>Report</strong> one is in Documents</div>
  <span data-find-region>Listed Reports</span>
  <button>report button, not searched</button>
`

beforeEach(() => {
  vi.useFakeTimers()
})

afterEach(() => {
  if (component) void unmount(component)
  component = null
  document.body.innerHTML = ''
  vi.useRealTimers()
})

describe('AskCmdrFind', () => {
  it('opens on ⌘F inside the rail, claims the key, and focuses its field', async () => {
    setup(THREAD)
    const documentHeard = vi.fn()
    document.addEventListener('keydown', documentHeard)
    const event = await openFind()
    document.removeEventListener('keydown', documentHeard)

    expect(event.defaultPrevented).toBe(true)
    expect(documentHeard).not.toHaveBeenCalled()
    expect(field()).not.toBeNull()
    expect(document.activeElement).toBe(field())
    expect(rail.querySelector('[role="search"]')?.getAttribute('aria-label')).toBe('Find in chat')
  })

  it('leaves ⌘F outside the rail to the app', async () => {
    setup(THREAD)
    const outside = document.createElement('div')
    document.body.appendChild(outside)
    const event = press(outside, 'f', { metaKey: true })
    flushSync()
    await tick()
    expect(event.defaultPrevented).toBe(false)
    expect(field()).toBeNull()
  })

  it('leaves ⌘G to Go to path while the bar is closed', () => {
    setup(THREAD)
    const event = press(composer, 'g', { metaKey: true })
    expect(event.defaultPrevented).toBe(false)
  })

  it('counts the hits in find regions only', async () => {
    setup(THREAD)
    await openFind()
    await type('report')
    expect(counter()).toBe('1 of 3')
    await type('nothing like this')
    expect(counter()).toBe('No matches')
  })

  it('steps with Enter / ⇧Enter in the field and wraps at both ends', async () => {
    setup(THREAD)
    await openFind()
    await type('report')
    const input = openField()
    press(input, 'Enter')
    flushSync()
    expect(counter()).toBe('2 of 3')
    press(input, 'Enter')
    press(input, 'Enter')
    flushSync()
    expect(counter()).toBe('1 of 3')
    press(input, 'Enter', { shiftKey: true })
    flushSync()
    expect(counter()).toBe('3 of 3')
  })

  it('steps with ⌘G / ⇧⌘G from anywhere in the rail, and leaves a bare Enter in the composer alone', async () => {
    setup(THREAD)
    await openFind()
    await type('report')
    composer.focus()
    const next = press(composer, 'g', { metaKey: true })
    flushSync()
    expect(next.defaultPrevented).toBe(true)
    expect(counter()).toBe('2 of 3')
    press(composer, 'G', { metaKey: true, shiftKey: true })
    flushSync()
    expect(counter()).toBe('1 of 3')
    const enter = press(composer, 'Enter')
    flushSync()
    expect(enter.defaultPrevented).toBe(false)
    expect(counter()).toBe('1 of 3')
  })

  it('closes on Escape in the field and hands focus back to where it was', async () => {
    setup(THREAD)
    composer.focus()
    await openFind()
    await type('report')
    const escape = press(openField(), 'Escape')
    flushSync()
    await tick()
    expect(escape.defaultPrevented).toBe(true)
    expect(field()).toBeNull()
    expect(document.activeElement).toBe(composer)
  })

  it('falls back to the rail when the element focus came from is gone', async () => {
    setup(THREAD)
    composer.focus()
    await openFind()
    composer.remove()
    press(openField(), 'Escape')
    flushSync()
    await tick()
    expect(document.activeElement).toBe(rail)
  })

  it('reopening puts the cursor back in the field with the query kept', async () => {
    setup(THREAD)
    await openFind()
    await type('report')
    composer.focus()
    await openFind()
    expect(document.activeElement).toBe(field())
    expect(field()?.value).toBe('report')
  })

  it('follows a streaming re-render of the thread', async () => {
    setup(THREAD)
    await openFind()
    await type('report')
    expect(counter()).toBe('1 of 3')
    const streamed = document.createElement('div')
    streamed.setAttribute('data-find-region', '')
    streamed.textContent = 'Another report arrives'
    thread.appendChild(streamed)
    // The observer's callback is a microtask; its refresh waits a frame.
    await Promise.resolve()
    vi.advanceTimersByTime(50)
    flushSync()
    expect(counter()).toBe('1 of 4')
  })

  it('notes that only loaded messages are searched when older ones exist', async () => {
    setup(THREAD, true)
    await openFind()
    expect(rail.querySelector('[role="search"]')?.textContent).toContain('Searching loaded messages')
  })
})
