/**
 * Where an MCP `nav_to_path` actually left the pane.
 *
 * `navigate()`'s two arms settle at different moments (`pane/navigate.ts` § "`settled`
 * resolve point, PER INTENT ARM"). The in-place arm's `settled` IS the listing promise,
 * so awaiting it is enough. The SWITCH arm commits the destination optimistically and
 * resolves `settled` immediately, before the new volume has listed anything: the pane
 * reports the target while the listing is still to come, and if that listing dies, an
 * edge-flow fallback (MTP-fatal, retry, open-home) moves the pane somewhere else
 * entirely. Replying `ok: true` on that resolve is what let a cross-volume navigation
 * ack `OK: Navigated left pane to …` for a pane that never went there.
 *
 * So after a switch the adapter waits for the pane to go QUIET — a listing that both
 * started and came to rest — and then reports the location the pane actually holds. The
 * timing rules live here as pure functions over injected probes, so they're unit-testable
 * without a real pane or a real clock.
 *
 * ❌ Don't replace the quiet wait with a plain path comparison. Both arms commit
 * optimistically (P4), so the pane reports the target from the moment `navigate()`
 * returns; only a settled listing tells success from a navigation still in flight.
 */

/** The three questions the quiet wait asks a pane, plus the clock it runs on. */
export interface PaneQuietProbe {
  /** The pane's live listing handle. A new listing means a new id. */
  getListingId: () => string | null
  /** Whether the pane's listing is mid-load. */
  isLoading: () => boolean
  /** Milliseconds since an arbitrary epoch. Injected so tests drive their own clock. */
  now: () => number
  /** Resolves after `ms`. Injected for the same reason. */
  sleep: (ms: number) => Promise<void>
}

export interface QuietWaitOptions {
  /** The pane's listing id before the navigation started. */
  listingIdBefore: string | null
  /** How long to wait for the pane to come to rest before giving up. */
  budgetMs: number
  /** How often to look. */
  pollMs: number
  /** How long the pane must stay idle on its new listing before it counts as at rest. */
  quietMs: number
}

/**
 * How long the adapter waits for a switched pane to come to rest, and how closely it
 * watches. The budget sits under the backend's 30 s `nav_to_path` round-trip so a pane
 * that never settles is reported as such by the FE, with the location it actually holds,
 * rather than as a bare backend timeout. `quietMs` is the confirmation window: a listing
 * that settles and is immediately replaced (a failing listing, then the edge-flow
 * fallback's) must not be read as an arrival.
 */
export const NAV_QUIET_WAIT = { budgetMs: 20_000, pollMs: 100, quietMs: 250 } as const

/** A pane location, narrowed to what deciding the outcome needs. */
export interface PanePlace {
  volumeId: string
  path: string
}

/**
 * What the pane did with the navigation, as the `mcp-response` reports it. The Rust
 * `nav_to_path` handler turns each into the tool's result — mirrored by `NavAck` in
 * `apps/desktop/src-tauri/src/mcp/executor/mod.rs`, and matched on the discriminant
 * rather than on any message text.
 */
export type NavLandingOutcome = { outcome: 'navigated' | 'fell-back' | 'did-not-settle' } & PanePlace

/**
 * Wait for the listing a volume switch kicks off to start AND come to rest.
 *
 * Returns `true` once the pane has held a listing other than `listingIdBefore` with no
 * load in flight for `quietMs`, `false` when the budget runs out first. The quiet window
 * re-arms whenever a load starts again, so a failing listing followed by a fallback's
 * listing resolves against the FALLBACK's resting place, not the doomed one's.
 */
export async function waitForPaneToGoQuiet(probe: PaneQuietProbe, options: QuietWaitOptions): Promise<boolean> {
  const deadline = probe.now() + options.budgetMs
  let quietSince: number | null = null

  for (;;) {
    const startedANewListing = probe.getListingId() !== options.listingIdBefore
    if (startedANewListing && !probe.isLoading()) {
      if (quietSince === null) quietSince = probe.now()
      else if (probe.now() - quietSince >= options.quietMs) return true
    } else {
      quietSince = null
    }

    if (probe.now() >= deadline) return false
    await probe.sleep(options.pollMs)
  }
}

/** Same place? Volume ids must match exactly; a trailing slash on either path doesn't count. */
function isSamePlace(a: PanePlace, b: PanePlace): boolean {
  return a.volumeId === b.volumeId && withoutTrailingSlash(a.path) === withoutTrailingSlash(b.path)
}

function withoutTrailingSlash(path: string): string {
  return path.length > 1 ? path.replace(/\/+$/, '') : path
}

/**
 * Decide what to tell the agent: the pane reached the target, came to rest somewhere
 * else, or never came to rest at all. `quiet: false` outranks a matching location
 * because an unsettled pane reports its destination optimistically.
 */
export function classifyLanding(args: { target: PanePlace; landed: PanePlace; quiet: boolean }): NavLandingOutcome {
  const { target, landed, quiet } = args
  if (!quiet) return { outcome: 'did-not-settle', ...landed }
  return { outcome: isSamePlace(landed, target) ? 'navigated' : 'fell-back', ...landed }
}
