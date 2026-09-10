/**
 * Tier 3 a11y tests for the viewer window: both pickers, the context menu, the copy
 * dialogs, the status bar, the toolbar, and the search-bar fixture.
 *
 * One file per component would cost about seven times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, fixtures, props,
 * and assertions; `choices` and `mountPicker` stay inside their block because two
 * blocks define each with different contents.
 *
 * The two module stubs below don't conflict (different modules, one block each), so
 * they sit at file level. Both spread the real module first, so a block that never
 * stubbed them still sees every un-stubbed export as it was.
 */

import { describe, it, expect, vi, afterEach } from 'vitest'
import { mount, tick, unmount } from 'svelte'

import EncodingPicker from './EncodingPicker.svelte'
import ViewModePicker from './ViewModePicker.svelte'
import ViewerContextMenu from './ViewerContextMenu.svelte'
import ViewerCopyDialogs from './ViewerCopyDialogs.svelte'
import ViewerStatusBar from './ViewerStatusBar.svelte'
import ViewerToolbar from './ViewerToolbar.svelte'
import ViewerTextCursor from './ViewerTextCursor.svelte'
import PullProgressPanel from './PullProgressPanel.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'
import type { EncodingChoice } from '$lib/ipc/bindings'
import type { TextCursorBox } from './viewer-text-cursor.svelte'

// `ModalDialog` notifies the backend on open/close; stub those IPC calls for the
// copy-dialogs block.
vi.mock('$lib/tauri-commands', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  notifyDialogOpened: vi.fn(() => Promise.resolve()),
  notifyDialogClosed: vi.fn(() => Promise.resolve()),
}))

// The status bar formats its byte count through the reactive settings.
vi.mock('$lib/settings/reactive-settings.svelte', async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  getFileSizeFormat: () => 'binary',
}))

// These components share one jsdom document, the dialogs portal into
// `document.body`, and axe resolves ARIA id references document-wide. Clearing
// between tests keeps each audit looking at its own render only.
afterEach(() => {
  document.body.innerHTML = ''
})

describe('EncodingPicker accessibility', () => {
  const choices: EncodingChoice[] = [
    { encoding: 'utf8', label: 'UTF-8', group: 'unicode' },
    { encoding: 'utf16Le', label: 'UTF-16 LE', group: 'unicode' },
    { encoding: 'windows1252', label: 'Western (Windows-1252)', group: 'western' },
  ]

  function mountPicker() {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(EncodingPicker, {
      target,
      props: {
        value: 'utf8',
        detected: 'utf8',
        options: choices,
        onChange: () => {},
      },
    })
    return { target, instance }
  }

  it('has no a11y violations on the closed picker', async () => {
    const { target, instance } = mountPicker()
    await tick()
    await expectNoA11yViolations(target)
    void unmount(instance)
  })

  it('exposes an aria-label on the trigger so AT can identify the picker', async () => {
    const { target, instance } = mountPicker()
    await tick()

    const trigger = target.querySelector('.select-trigger')
    expect(trigger?.getAttribute('aria-label')).toBe('Encoding')

    void unmount(instance)
  })

  it('uses the listbox combobox pattern with grouped options', async () => {
    // The Ark `Select` gives a `role="combobox"` trigger and a `role="listbox"`
    // popover whose options are bucketed under `role="group"` headings, with
    // full keyboard support (Tab focus, arrow-key option change, Enter commit).
    // Pin that the picker renders the accessible widget, not a bare button.
    const { target, instance } = mountPicker()
    // Ark's `Portal` mounts the listbox on the tick after its own effect, into `document.body`.
    await tick()
    await tick()

    expect(target.querySelector('[role="combobox"]')).not.toBeNull()
    expect(document.querySelector('[role="listbox"]')).not.toBeNull()
    expect(document.querySelectorAll('[data-part="item-group-label"]').length).toBeGreaterThan(0)

    void unmount(instance)
  })
})

describe('ViewModePicker accessibility', () => {
  function mountPicker(kind: 'text' | 'image' | 'pdf' = 'text', lastMediaKind: 'text' | 'image' | 'pdf' | null = null) {
    const target = document.createElement('div')
    document.body.appendChild(target)
    const instance = mount(ViewModePicker, {
      target,
      props: { kind, lastMediaKind, onViewAsText: () => {}, onViewAsMedia: () => {} },
    })
    return { target, instance }
  }

  it('has no a11y violations on the closed (disabled) picker', async () => {
    const { target, instance } = mountPicker()
    await tick()
    await expectNoA11yViolations(target)
    void unmount(instance)
  })

  it('exposes aria-label on the trigger so AT can identify the picker', async () => {
    const { target, instance } = mountPicker()
    await tick()

    expect(target.querySelector('.select-trigger')?.getAttribute('aria-label')).toBe('View mode')

    void unmount(instance)
  })

  it('surfaces its disabled state to AT for a genuine text file', async () => {
    // A genuine text file (no remembered media kind) has nothing to switch to, so
    // the picker is disabled. Pin the contract so a future "make it look enabled"
    // refactor can't silently drop the disabled announcement. Ark reflects it as
    // `data-disabled` plus `disabled` on the trigger button.
    const { target, instance } = mountPicker()
    await tick()

    const trigger = target.querySelector<HTMLButtonElement>('.select-trigger')
    expect(trigger).not.toBeNull()
    expect(trigger?.hasAttribute('data-disabled')).toBe(true)

    void unmount(instance)
  })

  it('has no a11y violations on the enabled reverse-switch picker (media file read as text)', async () => {
    const { target, instance } = mountPicker('text', 'image')
    await tick()
    await expectNoA11yViolations(target)
    void unmount(instance)
  })

  it('uses the listbox combobox pattern for keyboard navigation', async () => {
    // The Ark `Select` gives a `role="combobox"` trigger and a `role="listbox"`
    // popover with full keyboard support out of the box. Pin that the picker
    // stays on the accessible widget rather than a bare button.
    const { target, instance } = mountPicker()
    // Ark's `Portal` mounts the listbox on the tick after its own effect, into `document.body`.
    await tick()
    await tick()

    expect(target.querySelector('[role="combobox"]')).not.toBeNull()
    expect(document.querySelector('[role="listbox"]')).not.toBeNull()
    const option = document.querySelector('[data-part="item"][data-value="text"]')
    expect(option?.textContent).toContain('Text')

    void unmount(instance)
  })
})

describe('ViewerContextMenu a11y', () => {
  it('default state (selection present) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ViewerContextMenu, {
      target,
      props: {
        x: 50,
        y: 50,
        hasSelection: true,
        onCopy: () => {},
        onSelectAll: () => {},
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('no-selection state (Copy disabled) has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ViewerContextMenu, {
      target,
      props: {
        x: 50,
        y: 50,
        hasSelection: false,
        onCopy: () => {},
        onSelectAll: () => {},
        onClose: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

describe('ViewerCopyDialogs a11y', () => {
  it('confirm dialog has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ViewerCopyDialogs, {
      target,
      props: {
        confirmBytes: 5000,
        refuseBytes: null,
        onCancelConfirm: () => {},
        onProceedConfirm: () => {},
        onDismissRefuse: () => {},
        onSaveAs: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(document.body)
  })

  it('refuse dialog has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ViewerCopyDialogs, {
      target,
      props: {
        confirmBytes: null,
        refuseBytes: 200_000_000,
        onCancelConfirm: () => {},
        onProceedConfirm: () => {},
        onDismissRefuse: () => {},
        onSaveAs: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(document.body)
  })
})

describe('ViewerStatusBar a11y', () => {
  function mountStatusBar(props: {
    currentMode: 'fullLoad' | 'byteSeek' | 'lineIndex'
    isIndexing: boolean
    wordWrap: boolean
    totalLines: number | null
    kind?: 'text' | 'image' | 'pdf'
    mediaDimensions?: { width: number; height: number } | null
  }) {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ViewerStatusBar, {
      target,
      props: {
        fileName: 'example.txt',
        kind: props.kind ?? 'text',
        mediaDimensions: props.mediaDimensions ?? null,
        totalLines: props.totalLines,
        totalBytes: 2048,
        currentMode: props.currentMode,
        isIndexing: props.isIndexing,
        wordWrap: props.wordWrap,
        indexingTimeoutSecs: 5,
      },
    })
    return target
  }

  it('in-memory state has no a11y violations', async () => {
    const target = mountStatusBar({ currentMode: 'fullLoad', isIndexing: false, wordWrap: false, totalLines: 42 })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('streaming + wrap + unknown line count has no a11y violations', async () => {
    const target = mountStatusBar({ currentMode: 'byteSeek', isIndexing: true, wordWrap: true, totalLines: null })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('image media mode has no a11y violations', async () => {
    const target = mountStatusBar({
      currentMode: 'fullLoad',
      isIndexing: false,
      wordWrap: false,
      totalLines: null,
      kind: 'image',
      mediaDimensions: { width: 800, height: 600 },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

describe('ViewerToolbar a11y', () => {
  const choices: EncodingChoice[] = [
    { encoding: 'utf8', label: 'UTF-8', group: 'unicode' },
    { encoding: 'windows1252', label: 'Western (Windows-1252)', group: 'western' },
  ]

  function mountToolbar(props: {
    isIndexing: boolean
    tailMode: boolean
    kind?: 'text' | 'image' | 'pdf'
    lastMediaKind?: 'text' | 'image' | 'pdf' | null
  }) {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ViewerToolbar, {
      target,
      props: {
        fileName: 'example.txt',
        filePath: '/Users/demo/Documents/example.txt',
        kind: props.kind ?? 'text',
        lastMediaKind: props.lastMediaKind ?? null,
        currentEncoding: 'utf8',
        detectedEncoding: 'utf8',
        encodingChoices: choices,
        isIndexing: props.isIndexing,
        tailMode: props.tailMode,
        onViewAsText: () => {},
        onViewAsMedia: () => {},
        onEncodingChange: () => {},
        onToggleTail: () => {},
      },
    })
    return target
  }

  it('default state has no a11y violations', async () => {
    const target = mountToolbar({ isIndexing: false, tailMode: false })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('indexing + tail-on state has no a11y violations', async () => {
    const target = mountToolbar({ isIndexing: true, tailMode: true })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('media mode (image) has no a11y violations', async () => {
    const target = mountToolbar({ isIndexing: false, tailMode: false, kind: 'image' })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('media file read as text (reverse-switch picker) has no a11y violations', async () => {
    const target = mountToolbar({ isIndexing: false, tailMode: false, kind: 'text', lastMediaKind: 'image' })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier-3 a11y test for the viewer's search-bar toggles.
 *
 * The toggles live inline in `+page.svelte`. Mounting the entire viewer here
 * would pull in the IPC, virtual scroll, and window-management graph; we'd be
 * testing infrastructure, not a11y. Instead we materialize the same markup
 * the viewer renders and run axe against it. The byte-for-byte fidelity is
 * the pre-condition: any future change to the toolbar must also update this
 * fixture.
 */
describe('viewer search-bar a11y', () => {
  function renderToolbarFixture(opts: {
    useRegex: boolean
    caseSensitive: boolean
    searchError?: string | null
  }): HTMLElement {
    const target = document.createElement('div')
    target.innerHTML = `
    <div class="search-bar" role="search">
      <input
        type="text"
        placeholder="Find in file..."
        aria-label="Search text"
        class="text-field-control"
      />
      <button
        type="button"
        class="search-toggle ${opts.caseSensitive ? 'active' : ''}"
        aria-pressed="${String(opts.caseSensitive)}"
        aria-label="Case sensitive"
      >Aa</button>
      <button
        type="button"
        class="search-toggle ${opts.useRegex ? 'active' : ''}"
        aria-pressed="${String(opts.useRegex)}"
        aria-label="Regex"
      >.*</button>
      <span class="match-count" aria-live="polite">
        ${opts.searchError ? `<span class="search-error" role="alert">${opts.searchError}</span>` : ''}
      </span>
      <button type="button" aria-label="Previous match">▲</button>
      <button type="button" aria-label="Next match">▼</button>
      <button type="button" aria-label="Close search">✕</button>
    </div>
  `
    document.body.appendChild(target)
    return target
  }

  it('default state (case on, regex off) has no a11y violations', async () => {
    const target = renderToolbarFixture({ useRegex: false, caseSensitive: true })
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('regex enabled has no a11y violations', async () => {
    const target = renderToolbarFixture({ useRegex: true, caseSensitive: true })
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('invalid-query error variant has no a11y violations', async () => {
    const target = renderToolbarFixture({
      useRegex: true,
      caseSensitive: true,
      searchError: 'Invalid regex: parse error',
    })
    await expectNoA11yViolations(target)
    target.remove()
  })

  it('toggles expose aria-pressed and aria-label', () => {
    const target = renderToolbarFixture({ useRegex: true, caseSensitive: false })
    const caseToggle = target.querySelector<HTMLButtonElement>('button[aria-label="Case sensitive"]')
    const regexToggle = target.querySelector<HTMLButtonElement>('button[aria-label="Regex"]')
    expect(caseToggle?.getAttribute('aria-pressed')).toBe('false')
    expect(regexToggle?.getAttribute('aria-pressed')).toBe('true')
    target.remove()
  })

  it('error variant has role="alert"', () => {
    const target = renderToolbarFixture({
      useRegex: true,
      caseSensitive: true,
      searchError: 'Bad regex',
    })
    const alert = target.querySelector('[role="alert"]')
    expect(alert).not.toBeNull()
    expect(alert?.textContent.trim()).toBe('Bad regex')
    target.remove()
  })
})

/**
 * The mid-screen "fetching this file" state. The bar needs an accessible name (a
 * `progressbar` without one is announced as a bare percentage), and the unknown-size
 * variant swaps the bar for a decorative spinner beside visible text.
 */
describe('PullProgressPanel a11y', () => {
  function mountPanel(props: { bytesTotal: number | null; fraction: number | null; stalled?: boolean }) {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(PullProgressPanel, {
      target,
      props: {
        fileName: 'holiday.mp4',
        bytesDone: 48_000_000,
        bytesTotal: props.bytesTotal,
        fraction: props.fraction,
        stalled: props.stalled ?? false,
        onCancel: () => {},
      },
    })
    return target
  }

  it('known size: a named progress bar, no a11y violations', async () => {
    const target = mountPanel({ bytesTotal: 256_000_000, fraction: 0.1875 })
    await tick()
    const bar = target.querySelector('[role="progressbar"]')
    expect(bar?.getAttribute('aria-label')).toBe('Fetching holiday.mp4 to preview it')
    expect(bar?.getAttribute('aria-valuenow')).toBe('19')
    await expectNoA11yViolations(target)
  })

  it('unknown size: no bar, no a11y violations', async () => {
    const target = mountPanel({ bytesTotal: null, fraction: null, stalled: true })
    await tick()
    expect(target.querySelector('[role="progressbar"]')).toBeNull()
    await expectNoA11yViolations(target)
  })
})

/**
 * The text cursor is a visual echo of a selection screen readers already learn about
 * from the status bar's live region, so it must stay out of the accessibility tree
 * entirely. Mountable on its own precisely so this block can reach it: this file never
 * mounts `+page.svelte`.
 */
describe('ViewerTextCursor a11y', () => {
  function mountCursor(box: TextCursorBox | null) {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(ViewerTextCursor, { target, props: { box, blinkKey: '3:7' } })
    return target
  }

  it('is hidden from assistive tech and has no a11y violations', async () => {
    const target = mountCursor({ top: 120, left: 48, height: 18 })
    await tick()
    const cursor = target.querySelector('.text-cursor')
    expect(cursor).not.toBeNull()
    expect(cursor?.getAttribute('aria-hidden')).toBe('true')
    await expectNoA11yViolations(target)
  })

  it('renders nothing at all when there is no box to paint', async () => {
    const target = mountCursor(null)
    await tick()
    expect(target.querySelector('.text-cursor')).toBeNull()
    await expectNoA11yViolations(target)
  })
})
