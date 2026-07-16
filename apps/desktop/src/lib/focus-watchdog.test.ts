/**
 * Tests for the focus watchdog. Each case sets up the DOM, installs the
 * watchdog, drives focus, and asserts whether a WARN was logged after the
 * timer fires.
 *
 * Uses fake timers so the 500 ms wait doesn't bloat the test runtime.
 *
 * Listeners persist across tests via the shared `document` even after a
 * module re-import, so each test runs `_resetForTests()` in afterEach to
 * detach them before the next install.
 */

import { describe, it, expect, vi, beforeEach, afterEach, type MockInstance } from 'vitest'

// `vi.mock` is hoisted above imports; the factory needs to reach into a
// hoisted scope to share the spy with the test body. `vi.hoisted` is the
// supported escape hatch. Typing the spy's signature here lets each test's
// `warnSpy.mock.calls[0]` destructure as a real tuple instead of `unknown[]`.
const { warnSpy } = vi.hoisted(() => ({
  warnSpy: vi.fn<(message: string, ctx: { ae: string }) => void>(),
}))

vi.mock('$lib/logging/logger', () => ({
  getAppLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: warnSpy,
    error: vi.fn(),
  }),
}))

// Mock the quick-look state so the watchdog can read `isOpen` without dragging
// in the Tauri event API or the IPC command surface. Tests that exercise the
// Quick Look suppression branch flip `quickLookState.isOpen` directly.
const { quickLookState } = vi.hoisted(() => ({ quickLookState: { isOpen: false } }))
vi.mock('$lib/file-explorer/quick-look/quick-look-state.svelte', () => ({ quickLookState }))

import { initFocusWatchdog, _resetForTests } from './focus-watchdog'

describe('focus watchdog', () => {
  let hasFocusSpy: MockInstance<() => boolean>

  beforeEach(() => {
    warnSpy.mockClear()
    vi.useFakeTimers()
    document.body.innerHTML = ''
    // jsdom's `document.hasFocus()` returns `false` by default. Force `true`
    // for these tests so the watchdog's "main window not focused" suppression
    // doesn't swallow every case.
    hasFocusSpy = vi.spyOn(document, 'hasFocus').mockReturnValue(true)
    // Reset the Quick Look mock between tests so a flip in one case doesn't
    // leak into the next.
    quickLookState.isOpen = false
  })

  afterEach(() => {
    _resetForTests()
    hasFocusSpy.mockRestore()
    vi.useRealTimers()
    document.body.innerHTML = ''
  })

  it('does not warn when focus is inside the explorer', () => {
    const explorer = document.createElement('div')
    explorer.className = 'dual-pane-explorer'
    explorer.tabIndex = -1
    document.body.appendChild(explorer)
    explorer.focus()

    initFocusWatchdog()
    // Run the initial-check timer and any timer the watchdog might queue.
    vi.advanceTimersByTime(2000)

    expect(warnSpy).not.toHaveBeenCalled()
  })

  it('does not warn when focus is inside the Ask Cmdr rail', () => {
    // The rail is a keyboard home alongside the panes, so focus in its
    // composer (or any other rail chrome) is a valid resting place.
    const rail = document.createElement('aside')
    rail.className = 'ask-cmdr-rail'
    const composer = document.createElement('textarea')
    composer.className = 'composer-input'
    rail.appendChild(composer)
    document.body.appendChild(rail)
    composer.focus()

    initFocusWatchdog()
    vi.advanceTimersByTime(2000)

    expect(warnSpy).not.toHaveBeenCalled()
  })

  it('warns when the rail is open but focus falls to body (the bug it must still catch)', () => {
    // Rail being open is NOT a suppressor: if focus lands on <body> while the
    // rail is open, that is still the restore-focus bug the watchdog exists to
    // catch. Only focus actually INSIDE the rail counts as a home.
    const rail = document.createElement('aside')
    rail.className = 'ask-cmdr-rail'
    const composer = document.createElement('textarea')
    composer.className = 'composer-input'
    rail.appendChild(composer)
    document.body.appendChild(rail)

    document.body.focus()

    initFocusWatchdog()
    vi.advanceTimersByTime(2000)

    expect(warnSpy).toHaveBeenCalledTimes(1)
  })

  it('does not warn when a dialog is open even if focus is loose', () => {
    const dialog = document.createElement('div')
    dialog.setAttribute('role', 'dialog')
    document.body.appendChild(dialog)

    // Focus on body (the symptom we'd otherwise warn about), but the dialog
    // is open, so we should suppress.
    document.body.focus()

    initFocusWatchdog()
    vi.advanceTimersByTime(2000)

    expect(warnSpy).not.toHaveBeenCalled()
  })

  it('warns when focus leaves the explorer, no dialog open, after 500 ms', () => {
    const explorer = document.createElement('div')
    explorer.className = 'dual-pane-explorer'
    explorer.tabIndex = -1
    document.body.appendChild(explorer)
    const stray = document.createElement('input')
    document.body.appendChild(stray)

    // Start with focus in the explorer so the initial check passes.
    explorer.focus()

    initFocusWatchdog()
    vi.advanceTimersByTime(150) // past initial-check timer

    // Move focus to an element outside the explorer (simulates a modal that
    // closed and left focus on some stray input).
    stray.focus()

    // Just before the 500 ms threshold, no warn yet.
    vi.advanceTimersByTime(499)
    expect(warnSpy).not.toHaveBeenCalled()

    // Cross the threshold.
    vi.advanceTimersByTime(2)
    expect(warnSpy).toHaveBeenCalledTimes(1)
    const [, ctx] = warnSpy.mock.calls[0]
    expect(ctx.ae).toContain('input')
  })

  it('warns only once per episode and resets when focus returns to a pane', () => {
    const explorer = document.createElement('div')
    explorer.className = 'dual-pane-explorer'
    explorer.tabIndex = -1
    document.body.appendChild(explorer)
    const stray = document.createElement('input')
    document.body.appendChild(stray)
    const otherStray = document.createElement('button')
    document.body.appendChild(otherStray)

    explorer.focus()

    initFocusWatchdog()
    vi.advanceTimersByTime(150)

    // Episode 1: lose focus.
    stray.focus()
    vi.advanceTimersByTime(600)
    expect(warnSpy).toHaveBeenCalledTimes(1)

    // Moving focus between stray elements while still loose should NOT
    // trigger a second warn during the same episode.
    otherStray.focus()
    vi.advanceTimersByTime(600)
    expect(warnSpy).toHaveBeenCalledTimes(1)

    // Focus returns to the explorer, resetting the warned flag.
    explorer.focus()
    vi.advanceTimersByTime(100)

    // Lose focus again: should warn anew (new episode).
    stray.focus()
    vi.advanceTimersByTime(600)
    expect(warnSpy).toHaveBeenCalledTimes(2)
  })

  it('does not warn while the Quick Look panel is open', () => {
    // QLPreviewPanel takes key focus, so `document.activeElement` falls back
    // to `<body>` — the same symptom the watchdog warns about. The panel
    // open-state acts as a "dialog" for suppression purposes.
    const stray = document.createElement('input')
    document.body.appendChild(stray)
    document.body.focus()

    quickLookState.isOpen = true
    initFocusWatchdog()
    vi.advanceTimersByTime(2000)

    expect(warnSpy).not.toHaveBeenCalled()
  })

  it('does not warn during the initial paint if focus is already inside the explorer', () => {
    const explorer = document.createElement('div')
    explorer.className = 'dual-pane-explorer'
    explorer.tabIndex = -1
    document.body.appendChild(explorer)
    explorer.focus()

    initFocusWatchdog()
    vi.advanceTimersByTime(2000)

    expect(warnSpy).not.toHaveBeenCalled()
  })
})
