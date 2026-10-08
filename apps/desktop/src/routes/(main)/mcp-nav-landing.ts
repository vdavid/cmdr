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
  /** Whether the pane's folder stopped answering mid-read (`listing-stalled`); its load stays in flight. */
  isStalled: () => boolean
  /** Milliseconds since an arbitrary epoch. Injected so tests drive their own clock. */
  now: () => number
  /** Resolves after `ms`. Injected for the same reason. */
  sleep: (ms: number) => Promise<void>
}

export interface QuietWaitOptions {
  /** The pane's listing id before the navigation started. */
  listingIdBefore: string | null
  /**
   * Whether the navigation must show a listing other than `listingIdBefore` before the
   * pane counts as at rest. Off only when nothing new will list (`expectsNewListing`):
   * then an idle pane on the listing it already had is where the navigation ended.
   */
  requireNewListing: boolean
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
export type NavLandingOutcome = { outcome: 'navigated' | 'fell-back' | 'did-not-settle' | 'stalled' } & PanePlace

/**
 * How a wait for the pane ended: at rest, on a folder that stopped answering, or still
 * restless when the budget ran out. A stalled listing stays in flight and retries until
 * the server answers, so it never goes quiet: it's reported the moment the pane shows
 * it, ❌ never waited out.
 */
export type PaneRest = 'quiet' | 'stalled' | 'restless'

/**
 * What `mcp-nav-to-path` and `mcp-volume-select` put on the wire. The plain `{ ok, error }`
 * shape covers the declines that happen before the pane moves (no explorer, an
 * unresolvable path or unknown volume, a synchronous refusal); the landing shapes carry
 * a typed `outcome` plus the location the pane came to rest on, which the Rust handler
 * turns into the tool result. ❌ The backend branches on `outcome`, never on the message.
 */
export type NavReplyBody = { ok: false; error: string } | ({ ok: boolean } & NavLandingOutcome)

/**
 * Wait for the listing a volume switch kicks off to start AND come to rest.
 *
 * Returns `'quiet'` once the pane has held a listing other than `listingIdBefore` with no
 * load in flight for `quietMs`, `'stalled'` as soon as that listing stops answering, and
 * `'restless'` when the budget runs out first. The quiet window re-arms whenever a load
 * starts again, so a failing listing followed by a fallback's listing resolves against
 * the FALLBACK's resting place, not the doomed one's.
 */
export async function waitForPaneToGoQuiet(probe: PaneQuietProbe, options: QuietWaitOptions): Promise<PaneRest> {
  const deadline = probe.now() + options.budgetMs
  let quietSince: number | null = null

  for (;;) {
    const hasTheListingItNeeds = !options.requireNewListing || probe.getListingId() !== options.listingIdBefore
    if (hasTheListingItNeeds && probe.isStalled()) return 'stalled'
    if (hasTheListingItNeeds && !probe.isLoading()) {
      if (quietSince === null) quietSince = probe.now()
      else if (probe.now() - quietSince >= options.quietMs) return 'quiet'
    } else {
      quietSince = null
    }

    if (probe.now() >= deadline) return 'restless'
    await probe.sleep(options.pollMs)
  }
}

/**
 * Wait for an in-place navigation's listing (`settled`) to land, or for its folder to
 * stop answering, whichever comes first. A stall only counts once the pane holds a
 * listing other than `listingIdBefore`, so the folder it's leaving can't answer for the
 * one it's going to. Rejects when `settled` does, as awaiting it directly would.
 */
export async function waitForListingOrStall(
  settled: Promise<unknown>,
  probe: Pick<PaneQuietProbe, 'getListingId' | 'isStalled' | 'sleep'>,
  options: { listingIdBefore: string | null; pollMs: number },
): Promise<'quiet' | 'stalled'> {
  // A property, not a `let`: the callbacks below flip it, which flow analysis can't see
  // from the loop, so a bare boolean reads to it as forever `false`.
  const watch = { landed: false }
  const listing = settled.then(
    () => {
      watch.landed = true
      return 'quiet' as const
    },
    (e: unknown) => {
      watch.landed = true
      throw e
    },
  )
  // The race below reports a failure; this only keeps one arriving AFTER a stall
  // (the user pressing Esc on the stalled screen) from going unhandled.
  listing.catch(() => undefined)
  const stall = (async () => {
    while (!watch.landed) {
      if (probe.getListingId() !== options.listingIdBefore && probe.isStalled()) return 'stalled' as const
      await probe.sleep(options.pollMs)
    }
    return 'quiet' as const
  })()
  return Promise.race([listing, stall])
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
 * else, sits on a folder that stopped answering, or never came to rest at all. A rest
 * other than `'quiet'` outranks a matching location because an unsettled pane reports
 * its destination optimistically.
 */
export function classifyLanding(args: { target: PanePlace; landed: PanePlace; rest: PaneRest }): NavLandingOutcome {
  const { target, landed, rest } = args
  if (rest === 'stalled') return { outcome: 'stalled', ...landed }
  if (rest === 'restless') return { outcome: 'did-not-settle', ...landed }
  return { outcome: isSamePlace(landed, target) ? 'navigated' : 'fell-back', ...landed }
}

/**
 * Whether a volume switch has to show a new listing before the pane counts as at rest.
 *
 * Not when its destination, once the background correction has decided it, is where the
 * pane already was: the pane's props don't change, so nothing re-lists (re-selecting
 * the volume a pane shows). Not on a volume with no backend listing either (the servers
 * hub). Everywhere else a new listing is the only evidence of arrival, and waiting for it
 * is what spans an undialed phone's connect.
 */
export function expectsNewListing(args: { before: PanePlace; landed: PanePlace; hasBackendListing: boolean }): boolean {
  return args.hasBackendListing && !isSamePlace(args.before, args.landed)
}

/**
 * Decide what to tell an agent that selected a volume. Only the volume is compared: the
 * select doesn't pick the folder (the switch reopens the one last used there), so any
 * path on the selected volume is an arrival.
 */
export function classifyVolumeLanding(args: {
  targetVolumeId: string
  landed: PanePlace
  rest: PaneRest
}): NavLandingOutcome {
  const { targetVolumeId, landed, rest } = args
  if (rest === 'stalled') return { outcome: 'stalled', ...landed }
  if (rest === 'restless') return { outcome: 'did-not-settle', ...landed }
  return { outcome: landed.volumeId === targetVolumeId ? 'navigated' : 'fell-back', ...landed }
}
