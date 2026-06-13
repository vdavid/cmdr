/**
 * Pure trigger logic for the "What's new" popup. No `$state`, no IPC: this module is
 * deliberately pure so the decision truth table is unit-testable in isolation. The
 * reactive startup effect that calls it lives in `whats-new-trigger.svelte.ts`.
 *
 * The full intention behind each branch is in `docs/specs/whats-new-popup-plan.md`
 * § "When the popup shows" and the colocated `CLAUDE.md`.
 */

/** What the startup effect should do with this launch. */
export type WhatsNewDecision =
  /** Open the dialog with this slice, then stamp `lastSeen = current`. */
  | { action: 'show'; since: string | null; max: number }
  /** Write `lastSeen = current`, no dialog (fresh install, downgrade, or feature off). */
  | { action: 'stamp' }
  /** A would-show is blocked by onboarding or another startup modal. Don't stamp; retry later. */
  | { action: 'wait' }
  /** Version unchanged: nothing to do. */
  | { action: 'none' }

export interface DecideWhatsNewArgs {
  /** The version we last showed the user, or `''` when never stamped. */
  lastSeen: string
  /** The version running right now (`getVersion()`). */
  current: string
  /** `whatsNew.showOnUpdate`: governs only the automatic popup. */
  enabled: boolean
  /** `isOnboarded`: the discriminator between a fresh install and an existing user. */
  onboarded: boolean
  /** The onboarding wizard (or legacy FDA modal) is on screen. */
  onboardingShowing: boolean
  /** Another startup modal (crash report, expiration) is on screen. */
  otherStartupModalOpen: boolean
}

/** A would-show is gated when onboarding or another startup modal is up. */
function isGated(args: DecideWhatsNewArgs): boolean {
  return args.onboardingShowing || args.otherStartupModalOpen
}

export function decideWhatsNew(args: DecideWhatsNewArgs): WhatsNewDecision {
  // No stored version means "never stamped". `isOnboarded` disambiguates the two cases:
  // a genuine fresh install (not onboarded) versus an existing user updating into the
  // feature (onboarded). Get this backwards and either every fresh install eats a
  // changelog popup, or the release that ships the feature never demonstrates it.
  if (args.lastSeen === '') {
    if (!args.onboarded) {
      // Fresh install: onboarding owns the first launch, not a changelog. Silent stamp.
      return { action: 'stamp' }
    }
    // Inaugural showcase: show off the feature with the current release only.
    if (!args.enabled) return { action: 'stamp' }
    if (isGated(args)) return { action: 'wait' }
    return { action: 'show', since: null, max: 1 }
  }

  const order = compareVersions(args.current, args.lastSeen)

  // Version unchanged: nothing to do (don't even re-stamp).
  if (order === 0) return { action: 'none' }

  // Downgrade: rewrite `lastSeen` to current so a later re-upgrade behaves sanely.
  if (order < 0) return { action: 'stamp' }

  // Upgrade. Feature off → stamp silently (no backlog replay when re-enabled later).
  if (!args.enabled) return { action: 'stamp' }
  if (isGated(args)) return { action: 'wait' }
  return { action: 'show', since: args.lastSeen, max: 5 }
}

/**
 * Compares two semver strings by their numeric `major.minor.patch` core. Returns a
 * negative number if `a < b`, positive if `a > b`, and 0 if equal. A leading `v` and
 * any pre-release / build suffix are ignored (we only ever compare released versions).
 *
 * Numeric per-component comparison is load-bearing: a string compare would order
 * `0.10.0` before `0.9.0` and misread an upgrade as a downgrade.
 */
export function compareVersions(a: string, b: string): number {
  const coreA = parseVersionCore(a)
  const coreB = parseVersionCore(b)
  for (let i = 0; i < 3; i++) {
    if (coreA[i] !== coreB[i]) return coreA[i] - coreB[i]
  }
  return 0
}

function parseVersionCore(version: string): [number, number, number] {
  const core = version.replace(/^v/, '').split(/[-+]/, 1)[0]
  const parts = core.split('.')
  const major = Number.parseInt(parts[0] ?? '0', 10) || 0
  const minor = Number.parseInt(parts[1] ?? '0', 10) || 0
  const patch = Number.parseInt(parts[2] ?? '0', 10) || 0
  return [major, minor, patch]
}
