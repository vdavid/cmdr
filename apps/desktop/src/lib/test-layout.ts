/**
 * Gives an element a measured size in the unit-test DOM.
 *
 * happy-dom (like jsdom) has no layout engine, so every `clientHeight` /
 * `clientWidth` / `offsetWidth` / `offsetHeight` reads back `0`. That's fatal for
 * any component that sizes itself from its container: `FullList` and `BriefList`
 * bind their scroll surface's `clientHeight` and hand it to the virtual-window
 * math, so a zero-height surface renders ZERO rows — with no error, no warning,
 * and a spec that goes green while asserting on an empty DOM.
 *
 * This hands those four metrics a number for the elements a test names, and
 * leaves every other element reading whatever the environment says (`0`). The
 * component's own math is untouched: it still computes its window from the
 * height it was given, so a 100 px surface over 20 px rows renders exactly the
 * five rows the real app renders, and a scroll to 200 px lands on the same row.
 *
 * Usage (from any `*.test.ts`):
 *     import { installLayoutMock } from '$lib/test-layout'
 *
 *     const layout = installLayoutMock({
 *         '[data-file-list-surface]': { clientHeight: 400, clientWidth: 785, offsetWidth: 800 },
 *     })
 *     mount(FullList, { target, props })
 *     // …later, to watch the component react to a resize:
 *     layout.resize('[data-file-list-surface]', { clientHeight: 100 })
 *
 * Detail, and what this deliberately does NOT fake: `docs/tooling/testing.md`
 * § "`installLayoutMock()`".
 */

import { onTestFinished } from 'vitest'

/** The metrics a rule can supply. An omitted one falls through to the environment. */
export interface LayoutBox {
  clientHeight?: number
  clientWidth?: number
  offsetHeight?: number
  offsetWidth?: number
}

type Metric = keyof LayoutBox

const METRICS: readonly Metric[] = ['clientHeight', 'clientWidth', 'offsetHeight', 'offsetWidth']

/** Selector → box, in declaration order. The FIRST matching rule wins. */
export type LayoutRules = Record<string, LayoutBox>

export interface LayoutMock {
  /**
   * Replaces a rule's box and notifies every `ResizeObserver` watching a
   * matching element, which is what makes Svelte's `bind:clientHeight` re-read.
   * Merges into the existing box, so a partial `{ clientHeight }` keeps the width.
   */
  resize: (selector: string, box: LayoutBox) => void
  /**
   * Scrolls a matching element and fires the `scroll` event the component
   * listens for. Sets `scrollTop` first, so the handler reads the new value.
   */
  scroll: (selector: string, scrollTop: number) => void
  /** Drops every rule and restores the environment's own metric getters. */
  restore: () => void
}

const rules: { selector: string; box: LayoutBox }[] = []
/** Metric getters as the environment defines them, captured on first patch. */
let originalMetrics: Map<Metric, PropertyDescriptor | undefined> | null = null

function boxFor(element: Element): LayoutBox | undefined {
  return rules.find((rule) => element.matches(rule.selector))?.box
}

function patchMetrics(): void {
  if (originalMetrics) return
  originalMetrics = new Map()
  for (const metric of METRICS) {
    const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, metric)
    originalMetrics.set(metric, original)
    Object.defineProperty(HTMLElement.prototype, metric, {
      configurable: true,
      get(this: HTMLElement): number {
        const stubbed = boxFor(this)?.[metric]
        if (stubbed !== undefined) return stubbed
        return original?.get ? (original.get.call(this) as number) : 0
      },
    })
  }
}

function unpatchMetrics(): void {
  if (!originalMetrics) return
  for (const [metric, original] of originalMetrics) {
    if (original) Object.defineProperty(HTMLElement.prototype, metric, original)
    else Reflect.deleteProperty(HTMLElement.prototype, metric)
  }
  originalMetrics = null
}

/**
 * Installs the rules for the current test and restores them when it finishes.
 * Call it from inside a test or a `beforeEach` (both are contexts where
 * `onTestFinished` can register), or drive `restore()` yourself.
 */
export function installLayoutMock(initialRules: LayoutRules): LayoutMock {
  rules.length = 0
  for (const [selector, box] of Object.entries(initialRules)) {
    rules.push({ selector, box: { ...box } })
  }
  patchMetrics()

  const mock: LayoutMock = {
    resize(selector, box) {
      const rule = rules.find((r) => r.selector === selector)
      if (rule) Object.assign(rule.box, box)
      else rules.push({ selector, box: { ...box } })
      notifyResizeObservers(selector)
    },
    scroll(selector, scrollTop) {
      for (const element of document.querySelectorAll(selector)) {
        element.scrollTop = scrollTop
        element.dispatchEvent(new Event('scroll'))
      }
    },
    restore() {
      rules.length = 0
      unpatchMetrics()
    },
  }

  onTestFinished(mock.restore)
  return mock
}

// ============================================================================
// ResizeObserver
// ============================================================================

/**
 * A `ResizeObserver` that fires only when a test says the layout moved.
 *
 * Nothing measures anything here, so there is no size change to observe on its
 * own; `installLayoutMock().resize()` is the only trigger. Everything else sees
 * the same never-fires behavior the old no-op stub had, which is what keeps this
 * safe for the specs that merely need the constructor to exist.
 *
 * Stubbed in as the global `ResizeObserver` by `src/test-setup.ts`.
 */
export class TestResizeObserver implements ResizeObserver {
  readonly #callback: ResizeObserverCallback
  readonly #targets = new Set<Element>()

  constructor(callback: ResizeObserverCallback) {
    this.#callback = callback
    liveObservers.add(this)
  }

  observe(target: Element): void {
    this.#targets.add(target)
  }

  unobserve(target: Element): void {
    this.#targets.delete(target)
  }

  disconnect(): void {
    this.#targets.clear()
    liveObservers.delete(this)
  }

  /** Fires the callback for the observed targets matching `selector`, if any. */
  notify(selector: string): void {
    const matched = [...this.#targets].filter((target) => target.matches(selector))
    if (matched.length === 0) return
    this.#callback(matched.map(resizeEntryFor), this)
  }
}

const liveObservers = new Set<TestResizeObserver>()

function notifyResizeObservers(selector: string): void {
  for (const observer of liveObservers) observer.notify(selector)
}

/**
 * A `ResizeObserverEntry` built from the element's (now stubbed) metrics, so a
 * component reading `entry.contentRect` sees the same numbers as one reading
 * `element.clientHeight`. Svelte's `bind:clientWidth`-family bindings ignore the
 * entry and re-read the element, so they'd work with a bare `{ target }` too;
 * the full shape is here for `bind:contentRect` and hand-written observers.
 */
function resizeEntryFor(target: Element): ResizeObserverEntry {
  const element = target as HTMLElement
  const width = element.clientWidth
  const height = element.clientHeight
  const size: ResizeObserverSize[] = [{ inlineSize: width, blockSize: height }]
  return {
    target,
    contentRect: {
      x: 0,
      y: 0,
      top: 0,
      left: 0,
      right: width,
      bottom: height,
      width,
      height,
      toJSON: () => ({}),
    },
    borderBoxSize: size,
    contentBoxSize: size,
    devicePixelContentBoxSize: size,
  }
}
