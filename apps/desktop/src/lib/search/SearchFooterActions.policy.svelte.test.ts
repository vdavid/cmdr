/**
 * Footer-policy tests for `SearchFooterActions.svelte`.
 *
 *   - Both footer buttons are ALWAYS visible. When there are no results (or the
 *     index isn't ready), they render disabled instead of hidden.
 *   - The "Show all in main window" button surfaces `⌥⏎` as its shortcut hint.
 *   - The "Go to file" button surfaces `⏎` only when `enterAction === 'go-to-file'`.
 *     Otherwise the shortcut hint isn't rendered on this button.
 */
import { describe, expect, it } from 'vitest'
import { mount, tick } from 'svelte'
import SearchFooterActions from './SearchFooterActions.svelte'

function mountFooter(props: Partial<Parameters<typeof mount>[1]['props']> = {}) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  mount(SearchFooterActions, {
    target,
    props: {
      resultCount: 0,
      disabled: false,
      onShowAllInMainWindow: () => {},
      onGoToFile: () => {},
      enterAction: 'run-search',
      ...props,
    },
  })
  return target
}

describe('SearchFooterActions round 2', () => {
  it('D6: both buttons render even when resultCount is 0 (disabled, not hidden)', async () => {
    const target = mountFooter({ resultCount: 0 })
    await tick()
    const buttons = target.querySelectorAll('button')
    expect(buttons.length).toBe(2)
    for (const b of buttons) {
      expect(b.disabled).toBe(true)
    }
  })

  it('D6: both buttons enabled when results present and not disabled', async () => {
    const target = mountFooter({ resultCount: 5 })
    await tick()
    const buttons = target.querySelectorAll('button')
    expect(buttons.length).toBe(2)
    for (const b of buttons) {
      expect(b.disabled).toBe(false)
    }
  })

  it('R3: "Show all in main window" surfaces the ⌥⏎ shortcut, not ⌥A', async () => {
    const target = mountFooter({ resultCount: 5 })
    await tick()
    const text = target.textContent
    expect(text).toContain('Show all in main window')
    expect(text).toContain('⌥⏎')
    // The ⌥A label belonged to round 1; the new owner is mode chip AI.
    // It must not appear on this button anymore.
    const showAllBtn = target.querySelectorAll('button')[1]
    expect(showAllBtn.textContent.includes('⌥A')).toBe(false)
  })

  it('D8: "Go to file" surfaces the ⏎ hint when enterAction is "go-to-file"', async () => {
    const target = mountFooter({ resultCount: 5, enterAction: 'go-to-file' })
    await tick()
    const goToFileBtn = target.querySelectorAll('button')[0]
    expect(goToFileBtn.textContent).toContain('⏎')
  })

  it('D8: "Go to file" does NOT surface the ⏎ hint when enterAction is "run-search"', async () => {
    const target = mountFooter({ resultCount: 5, enterAction: 'run-search' })
    await tick()
    const goToFileBtn = target.querySelectorAll('button')[0]
    expect(goToFileBtn.textContent).not.toContain('⏎')
  })
})
