/**
 * Tier 3 a11y tests for the pane's chrome: the function-key bar, the resizer,
 * the type-to-jump indicator, the unreachable-volume banner, and the
 * double-click hint toast.
 *
 * One file per component would cost about five times as much: `svelte-tests`
 * charges per test FILE, not per test (`docs/testing.md` § "What a test actually
 * costs"). Each block below keeps its component's own doc comment, props, and
 * assertions.
 *
 * The connection and error views stay in their own files: each mocks
 * `$lib/tauri-commands` with a different shape, and `vi.mock` hoists per module,
 * so merging them would change what they exercise.
 */

import { describe, it, expect, vi } from 'vitest'
import { mount, tick } from 'svelte'
import { readFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { expectNoA11yViolations } from '$lib/test-a11y'

// Only the hint toast reaches for these two; nothing else in this file imports
// either module.
vi.mock('$lib/settings', () => ({ setSetting: vi.fn(() => Promise.resolve()) }))
vi.mock('$lib/ui/toast', () => ({ dismissToast: vi.fn(() => undefined) }))

import DoubleClickPaneHintToastContent from './DoubleClickPaneHintToastContent.svelte'
import FunctionKeyBar from './FunctionKeyBar.svelte'
import PaneResizer from './PaneResizer.svelte'
import TypeToJumpIndicator from './TypeToJumpIndicator.svelte'
import VolumeUnreachableBanner from './VolumeUnreachableBanner.svelte'

const here = path.dirname(fileURLToPath(import.meta.url))

/** A fresh container, appended to the document and ready to mount into. */
function container(): HTMLDivElement {
  const target = document.createElement('div')
  document.body.appendChild(target)
  return target
}

/**
 * Tier 3 a11y tests for `FunctionKeyBar.svelte`.
 *
 * F1-F10 toolbar at the bottom of the pane. Tests cover the visible
 * and hidden states. Shift-held variant uses `<svelte:document>` so
 * we can't easily toggle it in jsdom, so auditing the default variant
 * is sufficient for structural a11y.
 */
describe('FunctionKeyBar a11y', () => {
  it('visible (default keys) has no a11y violations', async () => {
    const target = container()
    mount(FunctionKeyBar, {
      target,
      props: {
        visible: true,
        onCommand: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('hidden (visible=false) has no a11y violations', async () => {
    const target = container()
    mount(FunctionKeyBar, {
      target,
      props: { visible: false },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `PaneResizer.svelte`.
 *
 * Thin drag handle between panes with `role="separator"` and
 * `aria-orientation="vertical"`.
 */
describe('PaneResizer a11y', () => {
  it('default render has no a11y violations', async () => {
    const target = container()
    mount(PaneResizer, {
      target,
      props: {
        onResize: () => {},
        onResizeEnd: () => {},
        onReset: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/**
 * Tier 3 a11y tests for `TypeToJumpIndicator.svelte`.
 *
 * Tooltip-like overlay surfaced in the bottom-right of the pane while the
 * user is typing for in-directory navigation. Three states:
 *
 * - Hidden: the component renders nothing (no DOM node).
 * - Active: visible with a fresh buffer, indicator says "Jump: …".
 * - Stale: buffer reset fired, indicator still visible, italic + dim. Still
 *   needs to announce (the live region must stay polite, not off).
 *
 * Plus a `prefers-reduced-motion: reduce` check that the CSS turns off the
 * opacity/font-style transitions.
 */
describe('TypeToJumpIndicator a11y', () => {
  it('hidden state renders nothing (no DOM node)', async () => {
    const target = container()
    mount(TypeToJumpIndicator, {
      target,
      props: { buffer: '', visible: false, stale: false },
    })
    await tick()
    // Nothing visible: the {#if visible} guard removes the element entirely.
    expect(target.querySelector('.type-to-jump-indicator')).toBeNull()
    await expectNoA11yViolations(target)
  })

  it('active state carries role="status", aria-live="polite", and the buffer in its accessible name', async () => {
    const target = container()
    mount(TypeToJumpIndicator, {
      target,
      props: { buffer: 'fil', visible: true, stale: false },
    })
    await tick()

    const el = target.querySelector('.type-to-jump-indicator')
    expect(el).not.toBeNull()
    expect(el?.getAttribute('role')).toBe('status')
    expect(el?.getAttribute('aria-live')).toBe('polite')
    // Accessible name surfaces the buffer so screen-reader users hear "Jump to fil".
    expect(el?.getAttribute('aria-label')).toBe('Jump to fil')
    // Visible text still includes the buffer for sighted users.
    expect(el?.textContent).toContain('fil')

    await expectNoA11yViolations(target)
  })

  it('stale state still announces (live region stays polite, not off)', async () => {
    const target = container()
    mount(TypeToJumpIndicator, {
      target,
      props: { buffer: 'co', visible: true, stale: true },
    })
    await tick()

    const el = target.querySelector('.type-to-jump-indicator')
    expect(el).not.toBeNull()
    expect(el?.getAttribute('role')).toBe('status')
    // Critical: the live region must NOT be flipped to `aria-live="off"` when
    // the indicator shifts to stale, which would suppress the announcement
    // for the next keystroke. The component leaves it polite.
    expect(el?.getAttribute('aria-live')).toBe('polite')
    expect(el?.classList.contains('is-stale')).toBe(true)

    await expectNoA11yViolations(target)
  })

  it('prefers-reduced-motion: reduce disables the CSS transition', () => {
    // jsdom doesn't evaluate `prefers-reduced-motion` against `getComputedStyle`,
    // and the Svelte vite plugin processes the component's scoped CSS through
    // a separate stylesheet that doesn't materialize as a `<style>` tag in
    // jsdom either. So we assert the contract at the source: the component
    // contains a `prefers-reduced-motion: reduce` block setting `transition:
    // none` on the indicator. If the rule disappears, this catches it.
    const source = readFileSync(path.join(here, 'TypeToJumpIndicator.svelte'), 'utf8')
    expect(source).toMatch(/prefers-reduced-motion:\s*reduce/)
    expect(source).toMatch(/transition:\s*none/)
  })
})

/**
 * Tier 3 a11y tests for `VolumeUnreachableBanner.svelte`.
 *
 * Full-pane "couldn't reach X" banner with retry and "Open home folder"
 * actions. Tests cover idle and retrying states.
 */
describe('VolumeUnreachableBanner a11y', () => {
  it('idle state (retry enabled) has no a11y violations', async () => {
    const target = container()
    mount(VolumeUnreachableBanner, {
      target,
      props: {
        originalPath: '/Volumes/Backup',
        retrying: false,
        onRetry: () => {},
        onOpenHome: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })

  it('retrying state (retry disabled) has no a11y violations', async () => {
    const target = container()
    mount(VolumeUnreachableBanner, {
      target,
      props: {
        originalPath: '/Volumes/Backup',
        retrying: true,
        onRetry: () => {},
        onOpenHome: () => {},
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})

/** Tier 3 a11y tests for `DoubleClickPaneHintToastContent.svelte`. */
describe('DoubleClickPaneHintToastContent a11y', () => {
  it('default has no a11y violations', async () => {
    const target = container()
    mount(DoubleClickPaneHintToastContent, { target, props: { toastId: 'hint-1' } })
    await tick()
    await expectNoA11yViolations(target)
  })
})
