/**
 * Text-size integration.
 *
 * Compounds two signals into a single effective scale and writes it to the
 * root element as `--font-scale`:
 *
 *   effective = systemMultiplier * (userPercent / 100)
 *
 * - `systemMultiplier` comes from the macOS Accessibility > Display > Text Size
 *   setting (mapped from `UIPreferredContentSizeCategoryName`). Read once at
 *   startup and re-read whenever the backend emits `system-text-size-changed`.
 *   Returns 1.0 on non-macOS.
 * - `userPercent` is `appearance.textSize` (50–200, default 100).
 *
 * **Single source of truth:** `computeAndApply()` is the only place that
 * combines the two values. It writes both:
 *   1. `--font-scale` on `:root` for CSS consumers (`html` font-size, density
 *      vars, calc-ed icon sizes).
 *   2. The `effectiveScale` Svelte `$state` for JS consumers (virtual scroll
 *      math, canvas-based column-width measurement, font-metrics ID).
 *
 * Both paths must agree, so anything else that needs the scale reads
 * `getEffectiveScale()` (or the CSS var) — never recomputes from inputs.
 *
 * The slider re-measurement (font metrics IPC + column re-flow) is debounced
 * 1 s after the last change and dispatched via `requestIdleCallback` so the
 * main thread stays responsive during slider drags. The CSS var and the
 * Svelte $state both update immediately so the user sees text grow live.
 */

import { commands } from '$lib/ipc/bindings'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { SvelteSet } from 'svelte/reactivity'
import { getAppLogger } from '$lib/logging/logger'
import { getSetting, onSpecificSettingChange } from '$lib/settings'
import { ensureFontMetricsLoaded } from '$lib/font-metrics'

const log = getAppLogger('text-size')

const REMEASURE_DEBOUNCE_MS = 1000

/**
 * Reactive snapshot of the current effective scale. Reading this inside a
 * Svelte `$derived` or `$effect` automatically re-runs the consumer when the
 * scale changes. Outside of Svelte runes, prefer `getEffectiveScale()` (which
 * reads this value as a snapshot).
 */

let effectiveScale = $state(1)

let systemMultiplier = 1
let unlistenSystem: UnlistenFn | undefined
let unlistenSetting: (() => void) | undefined
let remeasureTimer: number | undefined
// eslint-disable-next-line prefer-const -- mutated via .add/.delete/.clear, never reassigned
let scaleChangeListeners = new SvelteSet<(scale: number) => void>()

/**
 * Pure compounding function — the single point where system + user inputs
 * become the effective scale. Kept stateless so tests can pin behavior at
 * boundary values without mocking Tauri or DOM.
 *
 * @param systemMultiplier  Multiplier from the macOS Accessibility text size (≥ 0).
 * @param userPercent       The user's `appearance.textSize` slider value (50–200).
 * @returns                 The compounded multiplier, clamped to a sane minimum.
 */
export function compoundScale(systemMultiplier: number, userPercent: number): number {
  const sys = Number.isFinite(systemMultiplier) && systemMultiplier > 0 ? systemMultiplier : 1
  const user = Number.isFinite(userPercent) && userPercent > 0 ? userPercent / 100 : 1
  return Math.max(0.1, sys * user)
}

/**
 * Returns the current effective scale (system × user) as a snapshot. Use this
 * in plain `.ts` modules. Inside `.svelte.ts`/`.svelte` files, calling this
 * inside a `$derived`/`$effect` automatically tracks the underlying state.
 */
export function getEffectiveScale(): number {
  return effectiveScale
}

/**
 * Subscribes a non-Svelte listener to "settled" scale changes — fires after
 * the 1 s debounce + idle-callback used to coalesce expensive re-flows.
 * Returns an unsubscribe function.
 *
 * Inside `.svelte` / `.svelte.ts` modules, prefer
 * `$effect(() => getEffectiveScale())` — that path runs immediately on every
 * change. This API is for plain `.ts` consumers (column-width measurement
 * caches, etc.) that want to invalidate only after the user stops dragging.
 */
export function onDebouncedScaleChange(cb: (scale: number) => void): () => void {
  scaleChangeListeners.add(cb)
  return () => {
    scaleChangeListeners.delete(cb)
  }
}

function getUserPercent(): number {
  return getSetting('appearance.textSize')
}

/**
 * The single point where system + user settings are read and applied. Updates
 * both the CSS variable (immediate) and the reactive `effectiveScale` state
 * (immediate). Schedules a debounced "settled" notification for heavy
 * consumers via `requestIdleCallback` so the main thread isn't blocked
 * mid-drag.
 *
 * Pass `triggerRemeasure = false` to skip the debounced notification (used at
 * startup since `DualPaneExplorer` performs its own initial measurement).
 */
function computeAndApply(triggerRemeasure: boolean): number {
  const effective = compoundScale(systemMultiplier, getUserPercent())
  document.documentElement.style.setProperty('--font-scale', String(effective))
  effectiveScale = effective
  log.debug('Effective text scale: {scale} (system={sys}, user={user}%)', {
    scale: effective.toFixed(3),
    sys: systemMultiplier.toFixed(3),
    user: getUserPercent(),
  })

  if (triggerRemeasure) {
    if (remeasureTimer !== undefined) {
      window.clearTimeout(remeasureTimer)
    }
    remeasureTimer = window.setTimeout(() => {
      remeasureTimer = undefined
      // Defer heavy work to the idle window so the slider release frame is clean.
      const fire = () => {
        void ensureFontMetricsLoaded()
        for (const cb of scaleChangeListeners) {
          try {
            cb(effective)
          } catch (e) {
            log.warn('Scale-change listener threw: {error}', { error: e })
          }
        }
      }
      if ('requestIdleCallback' in window) {
        requestIdleCallback(fire)
      } else {
        setTimeout(fire, 0)
      }
    }, REMEASURE_DEBOUNCE_MS)
  }

  return effective
}

/**
 * Reads the system multiplier from Rust, applies the compounded scale, then
 * subscribes to both the system event and the user setting.
 *
 * Call once per window on startup.
 */
export async function initTextSize(): Promise<void> {
  try {
    systemMultiplier = await commands.getSystemTextSizeMultiplier()
    log.debug('System text size multiplier: {multiplier}', {
      multiplier: systemMultiplier.toFixed(3),
    })
  } catch (error) {
    log.warn('Could not read system text size multiplier, using 1.0: {error}', { error })
    systemMultiplier = 1
  }

  // Apply once at startup. No re-measure — DualPaneExplorer's own
  // `ensureFontMetricsLoaded` call covers the initial measurement.
  computeAndApply(false)

  try {
    unlistenSystem = await listen<number>('system-text-size-changed', (event) => {
      systemMultiplier = event.payload
      log.info('System text size changed: {multiplier}', {
        multiplier: systemMultiplier.toFixed(3),
      })
      // System events are settled; re-measure on the same debounce so a quick
      // burst of multi-step accessibility changes coalesces.
      computeAndApply(true)
    })
  } catch (error) {
    log.warn('Could not subscribe to system text-size changes: {error}', { error })
  }

  unlistenSetting = onSpecificSettingChange('appearance.textSize', () => {
    computeAndApply(true)
  })
}

/** Cleans up event listeners. */
export function cleanupTextSize(): void {
  if (remeasureTimer !== undefined) {
    window.clearTimeout(remeasureTimer)
    remeasureTimer = undefined
  }
  unlistenSystem?.()
  unlistenSystem = undefined
  unlistenSetting?.()
  unlistenSetting = undefined
  scaleChangeListeners.clear()
  log.debug('Text-size listeners cleaned up')
}
